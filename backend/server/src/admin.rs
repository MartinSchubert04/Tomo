//! Rutas de admin (`/api/admin/*`, solo admins): usuarios, extensiones de Suwayomi (instalar,
//! actualizar, desinstalar) y los repos de extensiones.
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, patch, post},
    Router,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{create_user, hash_password, validate_password, AdminUser, User},
    error::{ApiError, ApiResult, Json, Path, Query},
    state::AppState,
    suwayomi::Extension,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users).post(new_user))
        .route("/admin/users/{id}", patch(edit_user).delete(delete_user))
        .route("/admin/extensions", get(list_extensions))
        .route("/admin/extensions/refresh", post(refresh_extensions))
        .route("/admin/extensions/{pkg}", post(extension_action))
        .route("/admin/repos", get(get_repos).put(put_repos))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserRow {
    #[serde(flatten)]
    user: User,
    created_at: i64,
}

async fn list_users(State(s): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<UserRow>>> {
    let v = s
        .db
        .call(|c| {
            let mut st = c.prepare("SELECT id, name, admin, created_at FROM users ORDER BY name COLLATE NOCASE")?;
            let rows = st
                .query_map([], |r| Ok(UserRow { user: User { id: r.get(0)?, name: r.get(1)?, admin: r.get(2)? }, created_at: r.get(3)? }))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
    password: String,
    #[serde(default)]
    admin: bool,
}

async fn new_user(State(s): State<AppState>, a: AdminUser, Json(b): Json<NewUser>) -> ApiResult<Json<User>> {
    let u = s.db.call(move |c| create_user(c, &b.name, &b.password, b.admin)).await?;
    tracing::info!(by = %a.0.user.name, user = %u.name, admin = u.admin, "usuario creado");
    Ok(Json(u))
}

#[derive(Deserialize)]
struct EditUser {
    password: Option<String>,
    admin: Option<bool>,
}

async fn edit_user(State(s): State<AppState>, _a: AdminUser, Path(id): Path<i64>, Json(b): Json<EditUser>) -> ApiResult<StatusCode> {
    if let Some(p) = &b.password {
        validate_password(p)?;
    }
    s.db
        .call(move |c| {
            let tx = c.transaction()?;
            let is_admin: bool = tx
                .query_row("SELECT admin FROM users WHERE id = ?1", [id], |r| r.get(0))
                .optional()?
                .ok_or_else(|| ApiError::not_found("User not found"))?;
            if let Some(admin) = b.admin {
                if is_admin && !admin {
                    let admins: i64 = tx.query_row("SELECT COUNT(*) FROM users WHERE admin = 1", [], |r| r.get(0))?;
                    if admins <= 1 {
                        return Err(ApiError::conflict("Can't remove the last admin"));
                    }
                }
                tx.execute("UPDATE users SET admin = ?2 WHERE id = ?1", params![id, admin])?;
            }
            if let Some(p) = b.password {
                tx.execute("UPDATE users SET password_hash = ?2 WHERE id = ?1", params![id, hash_password(&p)?])?;
                tx.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
    s.auth.clear_cache();
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(State(s): State<AppState>, a: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if id == a.0.user.id {
        return Err(ApiError::conflict("You can't delete yourself"));
    }
    let n = s.db.call(move |c| Ok(c.execute("DELETE FROM users WHERE id = ?1", [id])?)).await?;
    if n == 0 {
        return Err(ApiError::not_found("User not found"));
    }
    s.auth.clear_cache();
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ExtQuery {
    lang: Option<String>,
    q: Option<String>,
    installed: Option<String>,
}

/// `lang`: uno o varios separados por coma; "es" incluye "es-419".
fn lang_matches(lang: &str, wanted: &[String]) -> bool {
    wanted.iter().any(|w| lang.eq_ignore_ascii_case(w) || lang.to_ascii_lowercase().starts_with(&format!("{}-", w.to_ascii_lowercase())))
}

fn filter_extensions(mut v: Vec<Extension>, q: &ExtQuery) -> Vec<Extension> {
    let langs: Vec<String> = q.lang.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
    let needle = q.q.as_deref().unwrap_or("").trim().to_lowercase();
    let only_installed = q.installed.as_deref().is_some_and(|v| v == "1" || v == "true");
    v.retain(|e| {
        (langs.is_empty() || lang_matches(&e.lang, &langs))
            && (needle.is_empty() || e.name.to_lowercase().contains(&needle) || e.pkg.to_lowercase().contains(&needle))
            && (!only_installed || e.installed)
    });
    v.sort_by(|a, b| b.installed.cmp(&a.installed).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

async fn list_extensions(State(s): State<AppState>, _a: AdminUser, Query(q): Query<ExtQuery>) -> ApiResult<Json<Vec<Extension>>> {
    let mut all = s.suwa.extensions().await?;
    // Suwayomi recién instalado no conoce ninguna hasta leer el índice del repo.
    if all.is_empty() {
        all = s.suwa.refresh_extensions().await?;
    }
    Ok(Json(filter_extensions(all, &q)))
}

async fn refresh_extensions(State(s): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<Extension>>> {
    let all = s.suwa.refresh_extensions().await?;
    Ok(Json(filter_extensions(all, &ExtQuery { lang: None, q: None, installed: None })))
}

#[derive(Deserialize)]
struct ExtAction {
    action: String,
}

async fn extension_action(State(s): State<AppState>, a: AdminUser, Path(pkg): Path<String>, Json(b): Json<ExtAction>) -> ApiResult<Json<Extension>> {
    let e = s.suwa.update_extension(&pkg, &b.action).await?;
    tracing::info!(by = %a.0.user.name, %pkg, action = %b.action, "extensión");
    Ok(Json(e))
}

async fn get_repos(State(s): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<String>>> {
    Ok(Json(s.suwa.repos().await?))
}

async fn put_repos(State(s): State<AppState>, _a: AdminUser, Json(wanted): Json<Vec<String>>) -> ApiResult<StatusCode> {
    let wanted: Vec<String> = wanted.into_iter().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect();
    for u in &wanted {
        if !(u.starts_with("https://") || u.starts_with("http://")) {
            return Err(ApiError::bad_request("Repos must be http(s) URLs"));
        }
    }
    let current = s.suwa.repos().await?;
    let wanted_keys: Vec<String> = wanted.iter().map(|u| repo_key(u)).collect();
    let current_keys: Vec<String> = current.iter().map(|u| repo_key(u)).collect();
    for (u, k) in current.iter().zip(&current_keys) {
        if !wanted_keys.contains(k) {
            s.suwa.remove_repo(u).await?;
        }
    }
    for (u, k) in wanted.iter().zip(&wanted_keys) {
        if !current_keys.contains(k) {
            s.suwa.add_repo(u).await?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Identidad de un repo para comparar: Suwayomi guarda otra URL que la que se le dio
/// (`raw.githubusercontent.com/o/r/repo/index.min.json` queda `github.com/o/r/raw/repo/index.pb`),
/// así que en GitHub se compara por `dueño/repo`.
fn repo_key(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let mut parts = rest.split('/');
    let host = parts.next().unwrap_or("").to_ascii_lowercase();
    if host == "github.com" || host == "raw.githubusercontent.com" {
        let owner = parts.next().unwrap_or("");
        let repo = parts.next().unwrap_or("");
        return format!("github:{}/{}", owner.to_ascii_lowercase(), repo.to_ascii_lowercase());
    }
    rest.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ext(name: &str, lang: &str, installed: bool) -> Extension {
        Extension {
            pkg: format!("p.{name}"),
            name: name.into(),
            lang: lang.into(),
            version: "1".into(),
            icon: String::new(),
            installed,
            has_update: false,
            obsolete: false,
            nsfw: "safe",
        }
    }

    #[test]
    fn filters_by_lang_and_sorts_installed_first() {
        let v = vec![ext("b", "es", false), ext("a", "en", false), ext("c", "es-419", true), ext("d", "all", false)];
        let q = ExtQuery { lang: Some("es,all".into()), q: None, installed: None };
        let names: Vec<_> = filter_extensions(v, &q).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["c", "b", "d"]);
    }

    #[test]
    fn repo_keys_match_across_github_urls() {
        assert_eq!(
            repo_key("https://raw.githubusercontent.com/keiyoushi/extensions/repo/index.min.json"),
            repo_key("https://github.com/keiyoushi/extensions/raw/repo/index.pb")
        );
        assert_ne!(repo_key("https://a.com/x.json"), repo_key("https://b.com/x.json"));
    }
}
