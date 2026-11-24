//! Datos por usuario: biblioteca (estado, favorito), progreso por capítulo, historial ("Seguir
//! leyendo"). Y la copia de las tarjetas de manga (`works`) y sus capítulos (`chapters`), que se
//! guardan al abrir una ficha, al agregar a la biblioteca o al leer, para que la biblioteca y el
//! historial carguen sin preguntarle a Suwayomi.
use std::collections::HashMap;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    auth::AuthUser,
    db::{now_ms, Db},
    error::{ApiError, ApiResult, Json, Path, Query},
    state::AppState,
    suwayomi::{manga_id, Chapter, MangaDetail, WorkCard},
};

pub const STATUSES: &[&str] = &["reading", "planned", "completed", "onhold", "dropped"];

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/library", get(list_library))
        .route("/library/{work}", axum::routing::put(put_library).delete(delete_library))
        .route("/progress/{work}", get(get_progress).put(put_progress))
        .route("/progress/{work}/mark", post(mark))
        .route("/history", get(history))
        .route("/history/{work}", axum::routing::delete(hide_history))
}

// ---------- tarjetas ----------

/// Qué obra es un id: manga de Suwayomi o libro subido.
pub enum WorkRef {
    Manga(i64),
    Book(i64),
}

pub fn parse_work(id: &str) -> ApiResult<WorkRef> {
    if let Some(m) = manga_id(id) {
        return Ok(WorkRef::Manga(m));
    }
    if let Some(b) = id.strip_prefix('b').and_then(|n| n.parse::<i64>().ok()).filter(|n| *n > 0) {
        return Ok(WorkRef::Book(b));
    }
    Err(ApiError::not_found("Not found"))
}

pub fn book_card(id: i64, title: String, format: String, has_cover: bool) -> WorkCard {
    WorkCard {
        id: format!("b{id}"),
        kind: "book".into(),
        title,
        cover: has_cover.then(|| format!("/api/books/{id}/cover")),
        source: Some("Upload".into()),
        format: Some(format),
    }
}

/// Guarda la tarjeta y los capítulos de una ficha recién pedida.
pub async fn save_manga(db: &Db, d: &MangaDetail) -> ApiResult<()> {
    let card = d.card.clone();
    let chapters = d.chapters.clone();
    db.call(move |c| {
        let tx = c.transaction()?;
        save_card(&tx, &card)?;
        save_chapters(&tx, &card.id, &chapters)?;
        tx.commit()?;
        Ok(())
    })
    .await
}

pub fn save_card(c: &Connection, card: &WorkCard) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO works (id, title, cover, source, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET title = excluded.title, cover = excluded.cover, source = excluded.source, updated_at = excluded.updated_at",
        params![card.id, card.title, card.cover, card.source, now_ms()],
    )?;
    Ok(())
}

/// Reemplaza los capítulos conocidos de un manga y anota la fecha del más nuevo.
pub fn save_chapters(c: &Connection, work: &str, chapters: &[Chapter]) -> rusqlite::Result<()> {
    c.execute("DELETE FROM chapters WHERE work_id = ?1", [work])?;
    let mut st = c.prepare_cached("INSERT OR REPLACE INTO chapters (work_id, id, name, number, ord, date) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
    for ch in chapters {
        st.execute(params![work, ch.id, ch.name, ch.number, ch.ord, ch.date])?;
    }
    let latest = chapters.iter().map(|c| c.date).filter(|d| *d > 0).max();
    c.execute(
        "INSERT INTO manga_meta (work_id, latest_at, checked_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(work_id) DO UPDATE SET latest_at = excluded.latest_at, checked_at = excluded.checked_at",
        params![work, latest, now_ms()],
    )?;
    Ok(())
}

/// Se asegura de que haya una tarjeta guardada de la obra (manga: la pide a Suwayomi si falta;
/// libro: 404 si no existe).
async fn ensure_card(s: &AppState, work: &str) -> ApiResult<()> {
    match parse_work(work)? {
        WorkRef::Book(id) => {
            let exists = s.db.call(move |c| Ok(c.query_row("SELECT 1 FROM books WHERE id = ?1", [id], |_| Ok(())).optional()?.is_some())).await?;
            if !exists {
                return Err(ApiError::not_found("Book not found"));
            }
        }
        WorkRef::Manga(id) => {
            let w = work.to_string();
            let have = s.db.call(move |c| Ok(c.query_row("SELECT 1 FROM works WHERE id = ?1", [w], |_| Ok(())).optional()?.is_some())).await?;
            if !have {
                let card = s.suwa.card(id).await?;
                s.db.call(move |c| Ok(save_card(c, &card)?)).await?;
            }
        }
    }
    Ok(())
}

/// Fuentes fijadas por el usuario para la búsqueda global (`prefs.sources`).
pub async fn pinned_sources(db: &Db, uid: i64) -> ApiResult<Vec<String>> {
    db.call(move |c| {
        let raw: Option<String> = c.query_row("SELECT data FROM prefs WHERE user_id = ?1", [uid], |r| r.get(0)).optional()?;
        let v = raw.and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok());
        Ok(v.and_then(|v| v.get("sources").and_then(|s| s.as_array()).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect())
    })
    .await
}

/// Tarjeta de una obra desde lo guardado (para biblioteca e historial).
fn card_from_row(work: &str, title: Option<String>, cover: Option<String>, source: Option<String>, book: Option<(String, String, bool)>) -> WorkCard {
    if let (Some(id), Some((bt, fmt, has_cover))) = (work.strip_prefix('b').and_then(|n| n.parse().ok()), book) {
        return book_card(id, bt, fmt, has_cover);
    }
    WorkCard { id: work.to_string(), kind: "manga".into(), title: title.unwrap_or_else(|| work.to_string()), cover, source, format: None }
}

// ---------- biblioteca ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    work: WorkCard,
    status: String,
    favorite: bool,
    added_at: i64,
    last_read_at: Option<i64>,
    unread: Option<i64>,
    latest_chapter_at: Option<i64>,
}

fn library_entries(c: &Connection, uid: i64, only: Option<&str>) -> rusqlite::Result<Vec<LibraryEntry>> {
    let sql = format!(
        "SELECT l.work_id, l.status, l.favorite, l.added_at, lr.updated_at,
                w.title, w.cover, w.source,
                b.title, b.format, b.cover_ext IS NOT NULL,
                mm.latest_at,
                (SELECT COUNT(*) FROM chapters ch WHERE ch.work_id = l.work_id),
                (SELECT COUNT(*) FROM chapters ch WHERE ch.work_id = l.work_id AND NOT EXISTS (
                    SELECT 1 FROM progress p WHERE p.user_id = l.user_id AND p.work_id = ch.work_id AND p.chapter = ch.id AND p.done = 1))
         FROM library l
         LEFT JOIN last_read lr ON lr.user_id = l.user_id AND lr.work_id = l.work_id
         LEFT JOIN works w ON w.id = l.work_id
         LEFT JOIN books b ON l.work_id = 'b' || b.id
         LEFT JOIN manga_meta mm ON mm.work_id = l.work_id
         WHERE l.user_id = ?1 {}
         ORDER BY lr.updated_at IS NULL, lr.updated_at DESC, l.added_at DESC",
        if only.is_some() { "AND l.work_id = ?2" } else { "" }
    );
    let mut st = c.prepare(&sql)?;
    let map = |r: &rusqlite::Row| -> rusqlite::Result<LibraryEntry> {
        let work: String = r.get(0)?;
        let book = match (r.get::<_, Option<String>>(8)?, r.get::<_, Option<String>>(9)?) {
            (Some(t), Some(f)) => Some((t, f, r.get::<_, Option<bool>>(10)?.unwrap_or(false))),
            _ => None,
        };
        let is_manga = work.starts_with('s');
        let total: i64 = r.get(12)?;
        let unread: i64 = r.get(13)?;
        Ok(LibraryEntry {
            work: card_from_row(&work, r.get(5)?, r.get(6)?, r.get(7)?, book),
            status: r.get(1)?,
            favorite: r.get(2)?,
            added_at: r.get(3)?,
            last_read_at: r.get(4)?,
            unread: (is_manga && total > 0).then_some(unread),
            latest_chapter_at: if is_manga { r.get(11)? } else { None },
        })
    };
    let rows = match only {
        Some(w) => st.query_map(params![uid, w], map)?.collect::<Result<Vec<_>, _>>()?,
        None => st.query_map(params![uid], map)?.collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

async fn list_library(State(s): State<AppState>, u: AuthUser) -> ApiResult<Json<Vec<LibraryEntry>>> {
    let uid = u.user.id;
    Ok(Json(s.db.call(move |c| Ok(library_entries(c, uid, None)?)).await?))
}

#[derive(Deserialize)]
struct LibraryBody {
    status: Option<String>,
    favorite: Option<bool>,
}

async fn put_library(State(s): State<AppState>, u: AuthUser, Path(work): Path<String>, Json(b): Json<LibraryBody>) -> ApiResult<Json<LibraryEntry>> {
    if let Some(st) = &b.status {
        if !STATUSES.contains(&st.as_str()) {
            return Err(ApiError::bad_request("Unknown status"));
        }
    }
    ensure_card(&s, &work).await?;
    let uid = u.user.id;
    let entry = s
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO library (user_id, work_id, status, favorite, added_at) VALUES (?1, ?2, COALESCE(?3, 'reading'), COALESCE(?4, 0), ?5)
                 ON CONFLICT(user_id, work_id) DO UPDATE SET status = COALESCE(?3, status), favorite = COALESCE(?4, favorite)",
                params![uid, work, b.status, b.favorite, now_ms()],
            )?;
            library_entries(c, uid, Some(&work))?.pop().ok_or_else(|| ApiError::internal("library entry vanished"))
        })
        .await?;
    Ok(Json(entry))
}

async fn delete_library(State(s): State<AppState>, u: AuthUser, Path(work): Path<String>) -> ApiResult<StatusCode> {
    let uid = u.user.id;
    s.db.call(move |c| Ok(c.execute("DELETE FROM library WHERE user_id = ?1 AND work_id = ?2", params![uid, work])?)).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- progreso ----------

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    #[serde(default)]
    chapter: String,
    position: String,
    fraction: f64,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    updated_at: i64,
}

fn progress_from_row(r: &rusqlite::Row, off: usize) -> rusqlite::Result<Progress> {
    Ok(Progress { chapter: r.get(off)?, position: r.get(off + 1)?, fraction: r.get(off + 2)?, done: r.get(off + 3)?, updated_at: r.get(off + 4)? })
}

#[derive(Serialize)]
struct ProgressOut {
    last: Option<Progress>,
    chapters: HashMap<String, Progress>,
}

async fn get_progress(State(s): State<AppState>, u: AuthUser, Path(work): Path<String>) -> ApiResult<Json<ProgressOut>> {
    parse_work(&work)?;
    let uid = u.user.id;
    let out = s
        .db
        .call(move |c| {
            let mut st = c.prepare("SELECT chapter, position, fraction, done, updated_at FROM progress WHERE user_id = ?1 AND work_id = ?2")?;
            let chapters: HashMap<String, Progress> = st
                .query_map(params![uid, work], |r| progress_from_row(r, 0))?
                .map(|p| p.map(|p| (p.chapter.clone(), p)))
                .collect::<Result<_, _>>()?;
            let last_ch: Option<String> = c
                .query_row("SELECT chapter FROM last_read WHERE user_id = ?1 AND work_id = ?2", params![uid, work], |r| r.get(0))
                .optional()?;
            let last = last_ch.and_then(|ch| chapters.get(&ch).cloned());
            Ok(ProgressOut { last, chapters })
        })
        .await?;
    Ok(Json(out))
}

async fn put_progress(State(s): State<AppState>, u: AuthUser, Path(work): Path<String>, Json(p): Json<Progress>) -> ApiResult<StatusCode> {
    let wr = parse_work(&work)?;
    if p.position.len() > 2048 || p.chapter.len() > 64 || !p.fraction.is_finite() {
        return Err(ApiError::bad_request("Invalid progress"));
    }
    if matches!(wr, WorkRef::Manga(_)) && p.chapter.is_empty() {
        return Err(ApiError::bad_request("chapter is required for manga"));
    }
    // La tarjeta hace falta para el historial; si Suwayomi no responde, el progreso se guarda igual.
    if let Err(e) = ensure_card(&s, &work).await {
        if e.status == StatusCode::NOT_FOUND {
            return Err(e);
        }
        tracing::debug!("progreso sin tarjeta de {work}: {}", e.message);
    }
    let uid = u.user.id;
    s.db
        .call(move |c| {
            let now = now_ms();
            let tx = c.transaction()?;
            tx.execute(
                "INSERT INTO progress (user_id, work_id, chapter, position, fraction, done, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(user_id, work_id, chapter) DO UPDATE SET position = excluded.position, fraction = excluded.fraction,
                   done = excluded.done, updated_at = excluded.updated_at",
                params![uid, work, p.chapter, p.position, p.fraction.clamp(0.0, 1.0), p.done, now],
            )?;
            tx.execute(
                "INSERT INTO last_read (user_id, work_id, chapter, updated_at, hidden) VALUES (?1, ?2, ?3, ?4, 0)
                 ON CONFLICT(user_id, work_id) DO UPDATE SET chapter = excluded.chapter, updated_at = excluded.updated_at, hidden = 0",
                params![uid, work, p.chapter, now],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MarkBody {
    chapters: Vec<String>,
    done: bool,
}

async fn mark(State(s): State<AppState>, u: AuthUser, Path(work): Path<String>, Json(b): Json<MarkBody>) -> ApiResult<StatusCode> {
    parse_work(&work)?;
    if b.chapters.len() > 10_000 {
        return Err(ApiError::bad_request("Too many chapters"));
    }
    let uid = u.user.id;
    s.db
        .call(move |c| {
            let now = now_ms();
            let tx = c.transaction()?;
            {
                let mut st = if b.done {
                    tx.prepare_cached(
                        "INSERT INTO progress (user_id, work_id, chapter, position, fraction, done, updated_at) VALUES (?1, ?2, ?3, '0', 1, 1, ?4)
                         ON CONFLICT(user_id, work_id, chapter) DO UPDATE SET done = 1, fraction = 1, updated_at = excluded.updated_at",
                    )?
                } else {
                    tx.prepare_cached(
                        "UPDATE progress SET done = 0, fraction = 0, position = '0', updated_at = ?4
                         WHERE user_id = ?1 AND work_id = ?2 AND chapter = ?3",
                    )?
                };
                for ch in &b.chapters {
                    st.execute(params![uid, work, ch, now])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- historial ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryEntry {
    work: WorkCard,
    progress: Progress,
    chapter_name: Option<String>,
    next_chapter: Option<String>,
}

#[derive(Deserialize)]
struct HistoryQuery {
    limit: Option<u32>,
}

async fn history(State(s): State<AppState>, u: AuthUser, Query(q): Query<HistoryQuery>) -> ApiResult<Json<Vec<HistoryEntry>>> {
    let uid = u.user.id;
    let limit = q.limit.unwrap_or(30).clamp(1, 200);
    let v = s
        .db
        .call(move |c| {
            let mut st = c.prepare(
                "SELECT lr.work_id, p.chapter, p.position, p.fraction, p.done, p.updated_at,
                        w.title, w.cover, w.source, b.title, b.format, b.cover_ext IS NOT NULL,
                        ch.name,
                        (SELECT n.id FROM chapters n WHERE n.work_id = lr.work_id AND n.ord > ch.ord ORDER BY n.ord LIMIT 1)
                 FROM last_read lr
                 JOIN progress p ON p.user_id = lr.user_id AND p.work_id = lr.work_id AND p.chapter = lr.chapter
                 LEFT JOIN works w ON w.id = lr.work_id
                 LEFT JOIN books b ON lr.work_id = 'b' || b.id
                 LEFT JOIN chapters ch ON ch.work_id = lr.work_id AND ch.id = lr.chapter
                 WHERE lr.user_id = ?1 AND lr.hidden = 0
                   AND (lr.work_id NOT LIKE 'b%' OR b.id IS NOT NULL)
                 ORDER BY lr.updated_at DESC LIMIT ?2",
            )?;
            let rows = st
                .query_map(params![uid, limit], |r| {
                    let work: String = r.get(0)?;
                    let book = match (r.get::<_, Option<String>>(9)?, r.get::<_, Option<String>>(10)?) {
                        (Some(t), Some(f)) => Some((t, f, r.get::<_, Option<bool>>(11)?.unwrap_or(false))),
                        _ => None,
                    };
                    let progress = progress_from_row(r, 1)?;
                    let next: Option<String> = r.get(13)?;
                    Ok(HistoryEntry {
                        work: card_from_row(&work, r.get(6)?, r.get(7)?, r.get(8)?, book),
                        next_chapter: if progress.done { next } else { None },
                        chapter_name: r.get(12)?,
                        progress,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(Json(v))
}

async fn hide_history(State(s): State<AppState>, u: AuthUser, Path(work): Path<String>) -> ApiResult<StatusCode> {
    let uid = u.user.id;
    s.db.call(move |c| Ok(c.execute("UPDATE last_read SET hidden = 1 WHERE user_id = ?1 AND work_id = ?2", params![uid, work])?)).await?;
    Ok(StatusCode::NO_CONTENT)
}
