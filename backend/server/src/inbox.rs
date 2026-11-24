//! Carpeta de entrada: lo que se deje en `<datos>/inbox` (a mano, por SFTP o con Syncthing) se
//! importa solo. Se revisa cada `inbox_every` (30 s); un archivo se toma recién cuando dejó de
//! cambiar (mismo tamaño que en la revisión anterior y sin tocarse hace `inbox_stable`, 10 s), así
//! no se importa uno a medio copiar. Lo que no se puede importar va a `inbox/failed/` con un
//! `.error.txt` al lado que dice por qué.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use serde::Serialize;

use crate::{
    bookmeta::format_for,
    books::{self, ImportFrom},
    catalogs::Hint,
    state::AppState,
};

/// Nombre de quien "subió" lo que entra por la carpeta.
pub const UPLOADER: &str = "inbox";

#[derive(Clone, Default)]
pub struct Inbox {
    /// Tamaño visto en la revisión anterior de cada archivo.
    seen: Arc<Mutex<HashMap<PathBuf, u64>>>,
    /// Una revisión a la vez (la periódica y la que pide el admin).
    busy: Arc<tokio::sync::Mutex<()>>,
}

pub fn dir(s: &AppState) -> PathBuf {
    s.cfg.data_dir.join("inbox")
}

fn failed_dir(s: &AppState) -> PathBuf {
    dir(s).join("failed")
}

pub async fn run(s: AppState, every: Duration) {
    let _ = tokio::fs::create_dir_all(dir(&s)).await;
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let n = scan(&s).await;
        if n > 0 {
            tracing::info!("carpeta de entrada: {n} libro(s) importado(s)");
        }
    }
}

fn hidden(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_none_or(|n| n.starts_with('.') || n.starts_with('~') || n.ends_with(".part") || n.ends_with(".tmp") || n.ends_with(".crdownload") || n.contains(".syncthing."))
}

/// Una revisión. Devuelve cuántos importó.
pub async fn scan(s: &AppState) -> usize {
    scan_with(s, false).await
}

/// `now`: no esperar a que el archivo esté quieto `inbox_stable` (alcanza con que no cambie de
/// tamaño entre dos pasadas).
pub async fn scan_with(s: &AppState, now: bool) -> usize {
    let _g = s.inbox.busy.lock().await;
    let d = dir(s);
    let _ = tokio::fs::create_dir_all(&d).await;
    let Ok(mut rd) = tokio::fs::read_dir(&d).await else { return 0 };
    let mut ready = Vec::new();
    let mut present = Vec::new();
    while let Ok(Some(e)) = rd.next_entry().await {
        let p = e.path();
        let Ok(md) = e.metadata().await else { continue };
        if !md.is_file() || hidden(&p) {
            continue;
        }
        present.push(p.clone());
        let len = md.len();
        let quiet = md.modified().ok().and_then(|m| SystemTime::now().duration_since(m).ok()).is_some_and(|age| now || age >= s.cfg.inbox_stable);
        let prev = s.inbox.seen.lock().unwrap().insert(p.clone(), len);
        if prev == Some(len) && quiet && len > 0 {
            ready.push((p, len));
        }
    }
    // Olvidar los que ya no están.
    s.inbox.seen.lock().unwrap().retain(|p, _| present.contains(p));
    let mut n = 0;
    for (p, len) in ready {
        s.inbox.seen.lock().unwrap().remove(&p);
        match import_one(s, &p, len).await {
            Ok(()) => n += 1,
            Err(msg) => {
                tracing::warn!(file = %p.display(), "carpeta de entrada: {msg}");
                fail(s, &p, &msg).await;
            }
        }
    }
    n
}

async fn import_one(s: &AppState, p: &Path, len: u64) -> Result<(), String> {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("book").to_string();
    let (format, ext) = format_for(&name).ok_or_else(|| "Unsupported file type (use EPUB, PDF, CBZ, MOBI, AZW3, FB2 or TXT)".to_string())?;
    if len > s.cfg.max_upload_bytes {
        return Err(format!("Larger than {} MB", s.cfg.max_upload_bytes / (1024 * 1024)));
    }
    let mut head = vec![0u8; 1024];
    let got = {
        use tokio::io::AsyncReadExt;
        let mut f = tokio::fs::File::open(p).await.map_err(|e| e.to_string())?;
        f.read(&mut head).await.map_err(|e| e.to_string())?
    };
    head.truncate(got);
    if !crate::bookmeta::magic_ok(format, &head) {
        return Err(format!("Not a valid {} file", format.to_uppercase()));
    }
    // Se importa desde acá mismo (misma carpeta de datos = mismo disco: `import_file` lo mueve).
    // Si falla, queda donde estaba y `fail` lo pasa a failed/.
    tokio::fs::create_dir_all(books::books_dir(s)).await.map_err(|e| e.to_string())?;
    let from = ImportFrom { origin: Some(UPLOADER.into()), source_url: None, uploader_id: None, uploader: UPLOADER.into(), hint: Hint::default(), cover_url: None };
    books::import_file(s, p, &name, format, ext, len, from).await.map(|_| ()).map_err(|e| e.message)
}

async fn fail(s: &AppState, p: &Path, msg: &str) {
    let fd = failed_dir(s);
    let _ = tokio::fs::create_dir_all(&fd).await;
    let name = p.file_name().map(|n| n.to_owned()).unwrap_or_default();
    let dest = fd.join(&name);
    if tokio::fs::rename(p, &dest).await.is_ok() {
        let mut err = dest.into_os_string();
        err.push(".error.txt");
        let _ = tokio::fs::write(PathBuf::from(err), format!("{msg}\n")).await;
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Dónde está la carpeta para el server (dentro del contenedor).
    pub path: String,
    /// Dónde está en la máquina (`LIBRARY_INBOX_LABEL`), si se sabe.
    pub host_path: Option<String>,
    pub pending: Vec<String>,
    pub failed: Vec<Failed>,
}

#[derive(Serialize)]
pub struct Failed {
    pub name: String,
    pub error: String,
}

pub async fn status(s: &AppState) -> Status {
    let d = dir(s);
    let mut pending = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(&d).await {
        while let Ok(Some(e)) = rd.next_entry().await {
            if e.metadata().await.is_ok_and(|m| m.is_file()) && !hidden(&e.path()) {
                pending.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    let mut failed = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(failed_dir(s)).await {
        while let Ok(Some(e)) = rd.next_entry().await {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".error.txt") || hidden(&e.path()) {
                continue;
            }
            let mut ep = e.path().into_os_string();
            ep.push(".error.txt");
            let error = tokio::fs::read_to_string(PathBuf::from(ep)).await.unwrap_or_default().trim().to_string();
            failed.push(Failed { name, error });
        }
    }
    pending.sort();
    failed.sort_by(|a, b| a.name.cmp(&b.name));
    Status { path: d.display().to_string(), host_path: s.cfg.inbox_label.clone(), pending, failed }
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing::{get, post};
    axum::Router::new().route("/admin/inbox", get(get_status)).route("/admin/inbox/scan", post(scan_now))
}

async fn get_status(axum::extract::State(s): axum::extract::State<AppState>, _a: crate::auth::AdminUser) -> crate::error::Json<Status> {
    crate::error::Json(status(&s).await)
}

/// Revisar ya (sin esperar a que el archivo se quede quieto 10 s: el admin dice que terminó).
async fn scan_now(axum::extract::State(s): axum::extract::State<AppState>, _a: crate::auth::AdminUser) -> crate::error::Json<Status> {
    // Dos pasadas: la primera anota los tamaños, la segunda importa los que no cambiaron.
    scan_with(&s, true).await;
    scan_with(&s, true).await;
    crate::error::Json(status(&s).await)
}
