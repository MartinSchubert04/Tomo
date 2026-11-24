//! Autenticación: contraseñas Argon2id, tokens de sesión aleatorios (en la base solo su sha256),
//! cookie `lib_session` (HttpOnly) o `Authorization: Bearer`, extractores `AuthUser`/`AdminUser`,
//! rate limit de login por IP y las rutas `/api/auth/*` y `/api/me*`.
//!
//! Una sesión dura 180 días desde el último uso; la renovación se escribe en la base como mucho
//! una vez por día, y `GET /api/me` (que la UI pide al abrir) vuelve a mandar la cookie.
use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Argon2,
};
use axum::{
    extract::{ConnectInfo, FromRef, FromRequestParts, State},
    http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    db::{now_ms, now_unix, Db},
    error::{ApiError, ApiResult, Json},
    state::AppState,
};

pub const COOKIE: &str = "lib_session";
/// Las sesiones duran 180 días desde el último uso.
pub const SESSION_TTL_S: i64 = 180 * 24 * 3600;
const RENEW_EVERY_S: i64 = 24 * 3600;
/// Caché en memoria token → usuario, para no ir a SQLite en cada imagen.
const AUTH_CACHE_TTL: Duration = Duration::from_secs(60);
/// Rate limit de login: 8 intentos fallidos por IP cada 10 minutos.
const LOGIN_MAX_FAILS: usize = 8;
const LOGIN_WINDOW: Duration = Duration::from_secs(600);
/// Tope de las preferencias de la UI.
pub const PREFS_MAX: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: i64,
    pub name: String,
    pub admin: bool,
}

pub const USER_COLS: &str = "id, name, admin";

pub fn user_from_row(r: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User { id: r.get(0)?, name: r.get(1)?, admin: r.get(2)? })
}

/// Estado de auth: la base, la caché de tokens y el rate limiter.
#[derive(Clone)]
pub struct Auth {
    pub db: Db,
    inner: Arc<AuthInner>,
}

struct AuthInner {
    cache: Mutex<HashMap<String, (User, Instant)>>,
    fails: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    /// Hash de una contraseña cualquiera: si el usuario no existe se verifica contra este, así el
    /// tiempo de respuesta no revela qué usuarios existen.
    dummy_hash: String,
}

impl Auth {
    pub fn new(db: Db) -> Self {
        let dummy_hash = hash_password("library-dummy-password").expect("argon2");
        Self {
            db,
            inner: Arc::new(AuthInner { cache: Default::default(), fails: Default::default(), dummy_hash }),
        }
    }

    /// Olvida todas las sesiones cacheadas (al borrar usuarios o cambiar contraseñas).
    pub fn clear_cache(&self) {
        self.inner.cache.lock().unwrap().clear();
    }

    /// El usuario dueño del token, o `None` si no es válido o venció.
    pub async fn authenticate(&self, token: &str) -> ApiResult<Option<User>> {
        let hash = token_hash(token);
        if let Some((user, at)) = self.inner.cache.lock().unwrap().get(&hash) {
            if at.elapsed() < AUTH_CACHE_TTL {
                return Ok(Some(user.clone()));
            }
        }
        let h = hash.clone();
        let user = self
            .db
            .call(move |c| {
                let now = now_unix();
                let row = c
                    .query_row(
                        "SELECT u.id, u.name, u.admin, s.expires_at FROM sessions s JOIN users u ON u.id = s.user_id
                         WHERE s.token_hash = ?1 AND s.expires_at > ?2",
                        params![h, now],
                        |r| Ok((user_from_row(r)?, r.get::<_, i64>(3)?)),
                    )
                    .optional()?;
                let Some((user, expires_at)) = row else { return Ok(None) };
                // Renovar solo si al vencimiento le queda menos que TTL − 1 día.
                if expires_at - now < SESSION_TTL_S - RENEW_EVERY_S {
                    c.execute(
                        "UPDATE sessions SET last_used = ?2, expires_at = ?3 WHERE token_hash = ?1",
                        params![h, now, now + SESSION_TTL_S],
                    )?;
                }
                Ok(Some(user))
            })
            .await?;
        let mut cache = self.inner.cache.lock().unwrap();
        if cache.len() > 2000 {
            cache.clear();
        }
        match &user {
            Some(u) => {
                cache.insert(hash, (u.clone(), Instant::now()));
            }
            None => {
                cache.remove(&hash);
            }
        }
        Ok(user)
    }

    /// Verifica usuario y contraseña. `Ok(None)` = no coinciden.
    pub async fn verify(&self, name: &str, password: &str) -> ApiResult<Option<User>> {
        let name = name.trim().to_string();
        let row = self
            .db
            .call(move |c| {
                Ok(c.query_row(
                    &format!("SELECT {USER_COLS}, password_hash FROM users WHERE name = ?1"),
                    [name],
                    |r| Ok((user_from_row(r)?, r.get::<_, String>(3)?)),
                )
                .optional()?)
            })
            .await?;
        let (user, stored) = match row {
            Some((u, h)) => (Some(u), h),
            None => (None, self.inner.dummy_hash.clone()),
        };
        let pw = password.to_string();
        let ok = tokio::task::spawn_blocking(move || verify_password(&pw, &stored)).await.map_err(ApiError::internal)?;
        Ok(if ok { user } else { None })
    }

    /// Login con rate limit por IP: abre una sesión y devuelve el token.
    pub async fn login(&self, ip: IpAddr, name: &str, password: &str) -> ApiResult<(String, User)> {
        if let Some(wait) = self.blocked_for(ip) {
            let mins = wait.as_secs().div_ceil(60).max(1);
            return Err(ApiError::rate_limited(format!("Too many failed login attempts. Try again in {mins} minute(s).")));
        }
        let Some(user) = self.verify(name, password).await? else {
            self.record_fail(ip);
            let shown: String = name.trim().chars().filter(|c| !c.is_control()).take(64).collect();
            tracing::warn!(%ip, user = ?shown, "login fallido");
            return Err(ApiError::unauthorized("Invalid name or password"));
        };
        self.inner.fails.lock().unwrap().remove(&ip);
        let token = new_token();
        let (h, uid) = (token_hash(&token), user.id);
        self.db
            .call(move |c| {
                let now = now_unix();
                c.execute(
                    "INSERT INTO sessions (token_hash, user_id, created_at, last_used, expires_at) VALUES (?1, ?2, ?3, ?3, ?4)",
                    params![h, uid, now, now + SESSION_TTL_S],
                )?;
                Ok(())
            })
            .await?;
        tracing::info!(user = %user.name, %ip, "login");
        Ok((token, user))
    }

    pub async fn logout(&self, token_hash: String) -> ApiResult<()> {
        self.inner.cache.lock().unwrap().remove(&token_hash);
        self.db
            .call(move |c| {
                c.execute("DELETE FROM sessions WHERE token_hash = ?1", [token_hash])?;
                Ok(())
            })
            .await
    }

    fn blocked_for(&self, ip: IpAddr) -> Option<Duration> {
        let mut fails = self.inner.fails.lock().unwrap();
        let q = fails.get_mut(&ip)?;
        while q.front().is_some_and(|t| t.elapsed() > LOGIN_WINDOW) {
            q.pop_front();
        }
        if q.len() >= LOGIN_MAX_FAILS {
            q.front().map(|t| LOGIN_WINDOW.saturating_sub(t.elapsed()))
        } else {
            None
        }
    }

    fn record_fail(&self, ip: IpAddr) {
        let mut fails = self.inner.fails.lock().unwrap();
        if fails.len() > 10_000 {
            fails.retain(|_, q| q.back().is_some_and(|t| t.elapsed() < LOGIN_WINDOW));
        }
        let q = fails.entry(ip).or_default();
        q.push_back(Instant::now());
        if q.len() > LOGIN_MAX_FAILS {
            q.pop_front();
        }
    }
}

// ---------- primitivas ----------

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("argon2: {e}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

/// 32 bytes aleatorios en base64url sin padding.
pub fn new_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("getrandom");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub fn token_hash(token: &str) -> String {
    let d = Sha256::digest(token.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn validate_name(u: &str) -> ApiResult<()> {
    let ok = (1..=32).contains(&u.chars().count())
        && u.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad_request("Name must be 1-32 characters: letters, numbers, '_', '-' or '.'"))
    }
}

pub fn validate_password(p: &str) -> ApiResult<()> {
    match p.chars().count() {
        0..6 => Err(ApiError::bad_request("Password must be at least 6 characters")),
        6..=256 => Ok(()),
        _ => Err(ApiError::bad_request("Password is too long")),
    }
}

/// Crea un usuario (bloqueante: hashea con argon2). 409 si el nombre ya existe.
pub fn create_user(c: &Connection, name: &str, password: &str, admin: bool) -> ApiResult<User> {
    let name = name.trim();
    validate_name(name)?;
    validate_password(password)?;
    let hash = hash_password(password)?;
    let res = c.query_row(
        &format!("INSERT INTO users (name, password_hash, admin, created_at) VALUES (?1, ?2, ?3, ?4) RETURNING {USER_COLS}"),
        params![name, hash, admin, now_ms()],
        user_from_row,
    );
    match res {
        Ok(u) => Ok(u),
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => {
            Err(ApiError::conflict("That name is already taken"))
        }
        Err(e) => Err(e.into()),
    }
}

/// Cookie de sesión (sin `Secure`: la app también se usa por http en la red de casa).
pub fn session_cookie(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={SESSION_TTL_S}"))
        .expect("cookie válida")
}

fn clear_cookie() -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")).expect("cookie válida")
}

// ---------- extractores ----------

/// Usuario logueado (cookie o Bearer). 401 si no hay sesión válida.
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user: User,
    pub token: String,
}

/// Usuario logueado y admin. 403 si no es admin.
pub struct AdminUser(pub AuthUser);

fn bearer(headers: &HeaderMap) -> Option<String> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = v.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim().to_string()).filter(|t| !t.is_empty())
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// La IP del cliente. Detrás de `tailscale serve` o del proxy de Docker la conexión viene de
/// loopback o de una IP privada: ahí se cree en `X-Forwarded-For`.
pub fn client_ip(parts_ext: &axum::http::Extensions, headers: &HeaderMap) -> IpAddr {
    let peer = parts_ext
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        .unwrap_or(IpAddr::from([127, 0, 0, 1]));
    let trusted = match peer {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v| v.is_loopback() || v.is_private()),
    };
    if !trusted {
        return peer;
    }
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(peer)
}

impl<S> FromRequestParts<S> for AuthUser
where
    Auth: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(u) = parts.extensions.get::<AuthUser>() {
            return Ok(u.clone());
        }
        let auth = Auth::from_ref(state);
        let token = bearer(&parts.headers)
            .or_else(|| cookie_token(&parts.headers))
            .ok_or_else(|| ApiError::unauthorized("Not logged in"))?;
        let user = auth.authenticate(&token).await?.ok_or_else(|| ApiError::unauthorized("Session expired, please log in again"))?;
        let u = AuthUser { user, token };
        parts.extensions.insert(u.clone());
        Ok(u)
    }
}

impl<S> FromRequestParts<S> for AdminUser
where
    Auth: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let u = AuthUser::from_request_parts(parts, state).await?;
        if !u.user.admin {
            return Err(ApiError::forbidden("Admin access required"));
        }
        Ok(AdminUser(u))
    }
}

/// IP del cliente como extractor.
pub struct ClientIp(pub IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientIp(client_ip(&parts.extensions, &parts.headers)))
    }
}

// ---------- rutas ----------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/me", get(me))
        .route("/me/prefs", put(put_prefs))
        .route("/me/password", post(change_password))
}

#[derive(Deserialize)]
struct LoginBody {
    name: String,
    password: String,
}

async fn login(State(s): State<AppState>, ClientIp(ip): ClientIp, Json(b): Json<LoginBody>) -> ApiResult<Response> {
    let (token, user) = s.auth.login(ip, &b.name, &b.password).await?;
    let mut res = Json(serde_json::json!({ "user": user })).into_response();
    res.headers_mut().insert(header::SET_COOKIE, session_cookie(&token));
    Ok(res)
}

async fn logout(State(s): State<AppState>, u: AuthUser) -> ApiResult<Response> {
    s.auth.logout(token_hash(&u.token)).await?;
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut().insert(header::SET_COOKIE, clear_cookie());
    Ok(res)
}

async fn me(State(s): State<AppState>, u: AuthUser) -> ApiResult<Response> {
    let uid = u.user.id;
    let prefs = s
        .db
        .call(move |c| {
            let raw: Option<String> = c.query_row("SELECT data FROM prefs WHERE user_id = ?1", [uid], |r| r.get(0)).optional()?;
            Ok(raw.and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok()).unwrap_or_else(|| serde_json::json!({})))
        })
        .await?;
    let mut res = Json(serde_json::json!({ "user": u.user, "prefs": prefs })).into_response();
    // Renueva la cookie (la sesión en la base se renueva sola al usarla). Un cliente con Bearer
    // la ignora.
    res.headers_mut().insert(header::SET_COOKIE, session_cookie(&u.token));
    Ok(res)
}

async fn put_prefs(State(s): State<AppState>, u: AuthUser, body: axum::body::Bytes) -> ApiResult<StatusCode> {
    if body.len() > PREFS_MAX {
        return Err(ApiError::too_large("Preferences are too large"));
    }
    let v: serde_json::Value = serde_json::from_slice(&body).map_err(|e| ApiError::bad_request(format!("Invalid JSON body: {e}")))?;
    if !v.is_object() {
        return Err(ApiError::bad_request("Preferences must be a JSON object"));
    }
    let uid = u.user.id;
    let raw = v.to_string();
    s.db
        .call(move |c| {
            c.execute(
                "INSERT INTO prefs (user_id, data) VALUES (?1, ?2) ON CONFLICT(user_id) DO UPDATE SET data = excluded.data",
                params![uid, raw],
            )?;
            Ok(())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PasswordBody {
    current: String,
    next: String,
}

async fn change_password(State(s): State<AppState>, u: AuthUser, Json(b): Json<PasswordBody>) -> ApiResult<StatusCode> {
    validate_password(&b.next)?;
    if s.auth.verify(&u.user.name, &b.current).await?.is_none() {
        return Err(ApiError::unauthorized("The current password is wrong"));
    }
    let (uid, keep) = (u.user.id, token_hash(&u.token));
    let next = b.next;
    s.db
        .call(move |c| {
            let hash = hash_password(&next)?;
            c.execute("UPDATE users SET password_hash = ?2 WHERE id = ?1", params![uid, hash])?;
            // Se cierran las otras sesiones del usuario; la actual sigue.
            c.execute("DELETE FROM sessions WHERE user_id = ?1 AND token_hash != ?2", params![uid, keep])?;
            Ok(())
        })
        .await?;
    s.auth.clear_cache();
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_parsing() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("a=1; lib_session=abc ; b=2"));
        assert_eq!(cookie_token(&h).as_deref(), Some("abc"));
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer xyz"));
        assert_eq!(bearer(&h).as_deref(), Some("xyz"));
    }

    #[test]
    fn password_roundtrip() {
        let h = hash_password("secreto1").unwrap();
        assert!(verify_password("secreto1", &h));
        assert!(!verify_password("otro", &h));
    }
}
