//! Binario `library` (Docker / server de casa): lee la configuración del entorno (ver
//! `deploy/.env.example`), arranca el server (`library::run`) y espera Ctrl+C / SIGTERM.
//! Comandos de mantenimiento:
//!   library adduser <usuario> <contraseña> [--admin]
//!   library passwd  <usuario> <contraseña>
//!   library health                 (healthcheck de Docker: GET /api/health local)
use std::{net::SocketAddr, time::Duration};

use anyhow::Context;
use library::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        // Sin colores cuando la salida no es una terminal (logs de Docker).
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "health") {
        return health_check().await;
    }
    let cfg = config_from_env()?;
    if !args.is_empty() {
        if let Err(e) = library::cli(&cfg, &args).await {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
        return Ok(());
    }

    let mut handle = library::run(cfg).await?;
    tokio::select! {
        r = handle.stopped() => r,
        _ = shutdown_signal() => {
            tracing::info!("apagando…");
            handle.shutdown().await
        }
    }
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// La configuración desde las variables `LIBRARY_*` (por defecto, los de `Config::new`).
fn config_from_env() -> anyhow::Result<Config> {
    let mut cfg = Config::new(var("LIBRARY_DATA").unwrap_or_else(|| "./data".into()));
    if let Some(b) = var("LIBRARY_BIND") {
        cfg.bind = b.parse().with_context(|| format!("LIBRARY_BIND inválido: {b:?}"))?;
    }
    if let Some(w) = var("LIBRARY_WEB_DIR") {
        cfg.web_dir = Some(w.into());
    }
    if let Some(u) = var("LIBRARY_SUWAYOMI_URL") {
        cfg.suwayomi_url = u.trim_end_matches('/').to_string();
    }
    if let Ok(r) = std::env::var("LIBRARY_EXTENSION_REPO") {
        cfg.extension_repo = r.trim().to_string();
    }
    if let Some(v) = var("LIBRARY_MAX_UPLOAD_MB") {
        let mb: u64 = v.parse().with_context(|| format!("LIBRARY_MAX_UPLOAD_MB inválido: {v:?}"))?;
        cfg.max_upload_bytes = mb.max(1) * 1024 * 1024;
    }
    if let Some(v) = var("LIBRARY_UPDATE_HOURS") {
        let h: f64 = v.parse().with_context(|| format!("LIBRARY_UPDATE_HOURS inválido: {v:?} (horas, 0 = nunca)"))?;
        cfg.update_every = (h > 0.0).then(|| Duration::from_secs_f64(h.max(0.25) * 3600.0));
    }
    if let Ok(a) = std::env::var("LIBRARY_ANILIST_URL") {
        cfg.anilist_url = a.trim().to_string();
    }
    cfg.admin_user = var("LIBRARY_ADMIN_USER");
    cfg.admin_password = var("LIBRARY_ADMIN_PASSWORD");
    Ok(cfg)
}

/// `library health`: pide `/api/health` al server local (sin curl en la imagen). Exit 1 si falla.
async fn health_check() -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let bind: SocketAddr = var("LIBRARY_BIND").unwrap_or_else(|| "127.0.0.1:7710".into()).parse()?;
    let addr = SocketAddr::from(([127, 0, 0, 1], bind.port()));
    let check = async {
        let mut s = tokio::net::TcpStream::connect(addr).await?;
        s.write_all(b"GET /api/health HTTP/1.0\r\nHost: localhost\r\n\r\n").await?;
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await?;
        anyhow::Ok(buf.starts_with(b"HTTP/1.0 200") || buf.starts_with(b"HTTP/1.1 200"))
    };
    match tokio::time::timeout(Duration::from_secs(4), check).await {
        Ok(Ok(true)) => Ok(()),
        _ => {
            eprintln!("el server no responde en {addr}");
            std::process::exit(1);
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
