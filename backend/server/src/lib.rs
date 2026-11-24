//! El server entero como biblioteca: `run(cfg)` abre/migra la base, crea el primer admin, arranca
//! las tareas de fondo (sesiones vencidas, repo de extensiones en Suwayomi, búsqueda de capítulos
//! nuevos) y escucha HTTP. `app(state)` arma las rutas (también lo usan los tests).
//!
//! Rutas: `/api/*` (ver `docs/API.md`) y, si hay `web_dir`, la UI compilada en `/`.
pub mod admin;
pub mod anilist;
pub mod auth;
pub mod bookmeta;
pub mod books;
pub mod config;
pub mod db;
pub mod error;
pub mod library;
pub mod manga;
pub mod state;
pub mod suggest;
pub mod suwayomi;
pub mod updater;
pub mod web;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderValue, Method},
    middleware,
    routing::get,
    Router,
};
use rusqlite::params;
use serde_json::json;
use tower_http::{
    compression::{
        predicate::{NotForContentType, Predicate, SizeAbove},
        CompressionLayer,
    },
    cors::CorsLayer,
    trace::TraceLayer,
};

pub use config::Config;
use db::{now_unix, Db};
use error::ApiError;
use state::AppState;

/// El server corriendo.
pub struct Handle {
    addr: SocketAddr,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    server: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    background: Vec<tokio::task::JoinHandle<()>>,
}

impl Handle {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Espera a que el server termine solo (error al escuchar).
    pub async fn stopped(&mut self) -> anyhow::Result<()> {
        match self.server.as_mut() {
            Some(s) => {
                let r = s.await;
                self.server = None;
                r??;
                Ok(())
            }
            None => std::future::pending().await,
        }
    }

    /// Apagado ordenado: deja de aceptar conexiones y corta las tareas de fondo.
    pub async fn shutdown(mut self) -> anyhow::Result<()> {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        for t in &self.background {
            t.abort();
        }
        if let Some(s) = self.server.take() {
            let _ = tokio::time::timeout(Duration::from_secs(10), s).await;
        }
        tracing::info!("{} apagado", state::brand_name());
        Ok(())
    }
}

/// Arma el estado (base, auth, cliente de Suwayomi). Separado de `run` para los tests.
pub fn build_state(cfg: Config, db: Db) -> anyhow::Result<AppState> {
    Ok(AppState {
        auth: auth::Auth::new(db.clone()),
        suwa: suwayomi::Suwayomi::new(&cfg.suwayomi_url)?,
        anilist: anilist::AniList::new(&cfg.anilist_url)?,
        anilist_img: reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?,
        db,
        cfg: Arc::new(cfg),
    })
}

/// Arranca el server con `cfg`. Vuelve en cuanto escucha (con `Handle::addr` real).
pub async fn run(cfg: Config) -> anyhow::Result<Handle> {
    std::fs::create_dir_all(&cfg.data_dir).with_context(|| format!("no se pudo crear {}", cfg.data_dir.display()))?;
    let db = Db::open(&cfg.data_dir.join("library.db"))?;
    bootstrap_admin(&db, &cfg).await?;
    // Restos de subidas cortadas por un reinicio.
    if let Ok(rd) = std::fs::read_dir(cfg.data_dir.join("books")) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().ends_with(".part") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let state = build_state(cfg.clone(), db.clone())?;

    let mut background = vec![tokio::spawn(cleanup_sessions(db))];
    {
        let (suwa, repo) = (state.suwa.clone(), cfg.extension_repo.clone());
        background.push(tokio::spawn(async move { suwa.ensure_repo(repo).await }));
    }
    if let Some(every) = cfg.update_every {
        background.push(tokio::spawn(updater::run(state.clone(), every)));
    }

    let listener = tokio::net::TcpListener::bind(cfg.bind).await.with_context(|| format!("no se pudo escuchar en {}", cfg.bind))?;
    let addr = listener.local_addr()?;
    tracing::info!(
        "{} {} escuchando en http://{} (datos: {}, UI: {}, Suwayomi: {})",
        state::brand_name(),
        env!("CARGO_PKG_VERSION"),
        addr,
        cfg.data_dir.display(),
        cfg.web_dir.as_ref().map(|d| d.display().to_string()).unwrap_or_else(|| "no".into()),
        cfg.suwayomi_url,
    );
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, app(state).into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(async {
        rx.await.ok();
    });
    let server = tokio::spawn(std::future::IntoFuture::into_future(server));
    Ok(Handle { addr, stop: Some(tx), server: Some(server), background })
}

async fn health(State(s): State<AppState>) -> axum::Json<serde_json::Value> {
    let suwayomi = s.suwa.ping().await;
    axum::Json(json!({ "ok": true, "name": state::brand_name(), "version": env!("CARGO_PKG_VERSION"), "suwayomi": suwayomi }))
}

/// Compresión solo para lo que gana: JSON y la UI. Nunca imágenes, libros ni el NDJSON de la
/// búsqueda (comprimirlo juntaría las líneas en vez de mandarlas a medida que llegan).
fn compression() -> CompressionLayer<impl Predicate> {
    let p = SizeAbove::new(512)
        .and(NotForContentType::IMAGES)
        .and(NotForContentType::new("application/x-ndjson"))
        .and(NotForContentType::new("application/pdf"))
        .and(NotForContentType::new("application/epub+zip"))
        .and(NotForContentType::new("application/vnd"))
        .and(NotForContentType::new("application/x-mobipocket-ebook"))
        .and(NotForContentType::new("application/octet-stream"));
    CompressionLayer::new().compress_when(p)
}

pub fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .merge(auth::routes())
        .merge(admin::routes())
        .merge(manga::routes())
        .merge(suggest::routes())
        .merge(library::routes())
        .merge(books::routes())
        // Ningún cuerpo JSON necesita más (las preferencias tienen su propio tope de 64 KB). La
        // subida de libros y la portada ponen el suyo.
        .layer(DefaultBodyLimit::max(256 * 1024))
        .fallback(|| async { ApiError::not_found("Unknown API route") })
        .with_state(state.clone());
    let mut root = Router::new().nest("/api", api);
    if let Some(dir) = &state.cfg.web_dir {
        root = root.fallback_service(web::router(dir));
    }
    root.layer(compression())
        .layer(middleware::map_response(security_headers))
        .layer(cors())
        .layer(TraceLayer::new_for_http().make_span_with(|req: &axum::http::Request<_>| {
            tracing::debug_span!("request", method = %req.method(), path = %req.uri().path())
        }))
}

/// Headers de seguridad para todas las respuestas (la CSP la pone `web` en el HTML).
async fn security_headers(mut res: axum::response::Response) -> axum::response::Response {
    let h = res.headers_mut();
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    res
}

/// CORS solo para el dev server de Vite (con cookies).
fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin([HeaderValue::from_static("http://localhost:5173")])
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::RANGE, header::CONTENT_TYPE])
        .expose_headers([header::CONTENT_RANGE, header::ACCEPT_RANGES, header::CONTENT_LENGTH])
        .max_age(Duration::from_secs(3600))
}

/// Primer arranque: crea el admin de `admin_user`/`admin_password` si no hay usuarios.
async fn bootstrap_admin(db: &Db, cfg: &Config) -> anyhow::Result<()> {
    let users: i64 = db
        .call(|c| Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?))
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    if users > 0 {
        return Ok(());
    }
    match (&cfg.admin_user, &cfg.admin_password) {
        (Some(u), Some(p)) => {
            let (u, p) = (u.clone(), p.clone());
            let user = db
                .call(move |c| auth::create_user(c, &u, &p, true))
                .await
                .map_err(|e| anyhow::anyhow!("no se pudo crear el admin inicial: {}", e.message))?;
            tracing::info!("admin inicial creado: {}", user.name);
        }
        _ => {
            tracing::warn!("no hay ningún usuario: definí LIBRARY_ADMIN_USER y LIBRARY_ADMIN_PASSWORD y reiniciá");
            tracing::warn!("(o: `library adduser <usuario> <contraseña> --admin`)");
        }
    }
    Ok(())
}

/// Comandos de mantenimiento sobre la base de `cfg.data_dir`:
/// `adduser <usuario> <contraseña> [--admin]` y `passwd <usuario> <contraseña>`.
pub async fn cli(cfg: &Config, args: &[String]) -> anyhow::Result<()> {
    std::fs::create_dir_all(&cfg.data_dir).with_context(|| format!("no se pudo crear {}", cfg.data_dir.display()))?;
    let db = Db::open(&cfg.data_dir.join("library.db"))?;
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let res = match a.as_slice() {
        ["adduser", user, pass, rest @ ..] => {
            let admin = rest.contains(&"--admin");
            let (u, p) = (user.to_string(), pass.to_string());
            db.call(move |c| auth::create_user(c, &u, &p, admin))
                .await
                .map(|u| println!("usuario creado: {} (admin: {})", u.name, u.admin))
        }
        ["passwd", user, pass] => {
            let (u, p) = (user.to_string(), pass.to_string());
            db.call(move |c| {
                auth::validate_password(&p)?;
                let hash = auth::hash_password(&p)?;
                let n = c.execute("UPDATE users SET password_hash = ?2 WHERE name = ?1", params![u, hash])?;
                if n == 0 {
                    return Err(ApiError::not_found("User not found"));
                }
                c.execute("DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE name = ?1)", [u])?;
                Ok(())
            })
            .await
            .map(|_| println!("contraseña cambiada"))
        }
        _ => anyhow::bail!(
            "uso:\n  library                                    arranca el server\n  library health\n  library adduser <usuario> <contraseña> [--admin]\n  library passwd <usuario> <contraseña>"
        ),
    };
    res.map_err(|e| anyhow::anyhow!(e.message))
}

/// Borra sesiones vencidas cada hora.
async fn cleanup_sessions(db: Db) {
    let mut tick = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tick.tick().await;
        let _ = db.call(|c| Ok(c.execute("DELETE FROM sessions WHERE expires_at < ?1", [now_unix()])?)).await;
    }
}
