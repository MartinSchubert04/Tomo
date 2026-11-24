//! Libros y cómics subidos (compartidos por todos los usuarios): subida en streaming a
//! `data/books/<id>.<ext>` (nunca entero en memoria), datos del EPUB/CBZ (`bookmeta.rs`), portadas
//! en `data/covers/`, el archivo con `Range` (para pdf.js y foliate-js) y las páginas de un CBZ.
use std::{
    collections::HashMap,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
};

use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Multipart, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::{
    auth::{new_token, AuthUser},
    bookmeta::{self, content_type, format_for, magic_ok, title_from_name},
    db::now_ms,
    error::{ApiError, ApiResult, Json, Path},
    library::book_card,
    state::AppState,
    suwayomi::WorkCard,
};

/// Tope de una portada subida por la UI.
const MAX_COVER: usize = 2 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    // La subida no tiene tope de cuerpo (pueden ser varios archivos): el tope es por archivo
    // (`LIBRARY_MAX_UPLOAD_MB`) y se controla mientras se escribe.
    Router::new()
        .route("/books", get(list).post(upload).layer(DefaultBodyLimit::disable()))
        .route("/books/{id}", get(one).patch(edit).delete(remove))
        .route("/books/{id}/file", get(file))
        .route("/books/{id}/cover", get(cover).put(put_cover).layer(DefaultBodyLimit::max(MAX_COVER)))
        .route("/books/{id}/pages", get(pages).put(put_pages))
        .route("/books/{id}/pages/{n}", get(page))
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    #[serde(flatten)]
    card: WorkCard,
    author: Option<String>,
    size: i64,
    added_at: i64,
    uploader: String,
    pages: Option<i64>,
}

const BOOK_COLS: &str = "id, title, format, cover_ext IS NOT NULL, author, size, added_at, uploader, pages";

fn book_from_row(r: &rusqlite::Row) -> rusqlite::Result<Book> {
    Ok(Book {
        card: book_card(r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?),
        author: r.get(4)?,
        size: r.get(5)?,
        added_at: r.get(6)?,
        uploader: r.get(7)?,
        pages: r.get(8)?,
    })
}

fn books_dir(s: &AppState) -> PathBuf {
    s.cfg.data_dir.join("books")
}

fn covers_dir(s: &AppState) -> PathBuf {
    s.cfg.data_dir.join("covers")
}

/// `b12` o `12` → 12.
fn parse_id(id: &str) -> ApiResult<i64> {
    id.strip_prefix('b').unwrap_or(id).parse::<i64>().ok().filter(|n| *n > 0).ok_or_else(|| ApiError::not_found("Book not found"))
}

/// Datos guardados de un libro: (formato, extensión, extensión de la portada, id de quien lo subió).
struct Row {
    format: String,
    ext: String,
    cover_ext: Option<String>,
    uploader_id: Option<i64>,
    title: String,
}

async fn row(s: &AppState, id: i64) -> ApiResult<Row> {
    s.db
        .call(move |c| {
            c.query_row("SELECT format, ext, cover_ext, uploader_id, title FROM books WHERE id = ?1", [id], |r| {
                Ok(Row { format: r.get(0)?, ext: r.get(1)?, cover_ext: r.get(2)?, uploader_id: r.get(3)?, title: r.get(4)? })
            })
            .optional()?
            .ok_or_else(|| ApiError::not_found("Book not found"))
        })
        .await
}

async fn get_book(s: &AppState, id: i64) -> ApiResult<Book> {
    s.db
        .call(move |c| {
            c.query_row(&format!("SELECT {BOOK_COLS} FROM books WHERE id = ?1"), [id], book_from_row)
                .optional()?
                .ok_or_else(|| ApiError::not_found("Book not found"))
        })
        .await
}

async fn list(State(s): State<AppState>, _u: AuthUser) -> ApiResult<Json<Vec<Book>>> {
    let v = s
        .db
        .call(|c| {
            let mut st = c.prepare(&format!("SELECT {BOOK_COLS} FROM books ORDER BY added_at DESC, id DESC"))?;
            let rows = st.query_map([], book_from_row)?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(Json(v))
}

async fn one(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Book>> {
    Ok(Json(get_book(&s, parse_id(&id)?).await?))
}

/// Borra el temporal si la subida no termina bien (también si se corta la conexión).
struct TmpGuard(Option<PathBuf>);

impl Drop for TmpGuard {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

async fn upload(State(s): State<AppState>, u: AuthUser, mut mp: Multipart) -> ApiResult<Json<Vec<Book>>> {
    let (books, covers) = (books_dir(&s), covers_dir(&s));
    tokio::fs::create_dir_all(&books).await?;
    tokio::fs::create_dir_all(&covers).await?;
    let max = s.cfg.max_upload_bytes;
    let mut out = Vec::new();
    while let Some(mut field) = mp.next_field().await? {
        if field.name() != Some("file") {
            continue;
        }
        let name = field.file_name().unwrap_or("").to_string();
        let (format, ext) = format_for(&name).ok_or_else(|| ApiError::bad_request(format!("Unsupported file type: {name}")))?;
        let tmp = books.join(format!("upload-{}.part", new_token()));
        let mut guard = TmpGuard(Some(tmp.clone()));
        let mut f = tokio::io::BufWriter::with_capacity(256 * 1024, tokio::fs::File::create(&tmp).await?);
        let mut size: u64 = 0;
        let mut head: Vec<u8> = Vec::new();
        while let Some(chunk) = field.chunk().await? {
            size += chunk.len() as u64;
            if size > max {
                return Err(ApiError::too_large(format!("{name} is larger than {} MB", max / (1024 * 1024))));
            }
            if head.len() < 1024 {
                head.extend_from_slice(&chunk[..chunk.len().min(1024 - head.len())]);
            }
            f.write_all(&chunk).await?;
        }
        f.flush().await?;
        drop(f);
        if size == 0 {
            return Err(ApiError::bad_request(format!("{name} is empty")));
        }
        if !magic_ok(format, &head) {
            return Err(ApiError::bad_request(format!("{name} is not a valid {} file", format.to_uppercase())));
        }
        let meta = {
            let p = tmp.clone();
            tokio::task::spawn_blocking(move || bookmeta::read_meta(&p, format)).await.map_err(ApiError::internal)?
        };
        let title = meta.title.clone().unwrap_or_else(|| title_from_name(&name));
        let (uid, uname) = (u.user.id, u.user.name.clone());
        let author = meta.author.clone();
        let pages = meta.pages;
        let id: i64 = s
            .db
            .call(move |c| {
                Ok(c.query_row(
                    "INSERT INTO books (title, author, format, ext, size, added_at, uploader_id, uploader, pages)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) RETURNING id",
                    params![title, author, format, ext, size as i64, now_ms(), uid, uname, pages],
                    |r| r.get(0),
                )?)
            })
            .await?;
        let dest = books.join(format!("{id}.{ext}"));
        if let Err(e) = tokio::fs::rename(&tmp, &dest).await {
            s.db.call(move |c| Ok(c.execute("DELETE FROM books WHERE id = ?1", [id])?)).await?;
            return Err(e.into());
        }
        guard.0 = None;
        if let Some((bytes, cext)) = meta.cover {
            if tokio::fs::write(covers.join(format!("b{id}.{cext}")), bytes).await.is_ok() {
                s.db.call(move |c| Ok(c.execute("UPDATE books SET cover_ext = ?2 WHERE id = ?1", params![id, cext])?)).await?;
            }
        }
        tracing::info!(user = %u.user.name, %id, %format, size, "libro subido");
        out.push(get_book(&s, id).await?);
    }
    if out.is_empty() {
        return Err(ApiError::bad_request("No file was sent (field name: file)"));
    }
    Ok(Json(out))
}

fn can_edit(u: &AuthUser, uploader_id: Option<i64>) -> ApiResult<()> {
    if u.user.admin || uploader_id == Some(u.user.id) {
        Ok(())
    } else {
        Err(ApiError::forbidden("Only who uploaded it or an admin can change it"))
    }
}

#[derive(Deserialize)]
struct EditBody {
    title: Option<String>,
    author: Option<String>,
}

async fn edit(State(s): State<AppState>, u: AuthUser, Path(id): Path<String>, Json(b): Json<EditBody>) -> ApiResult<Json<Book>> {
    let id = parse_id(&id)?;
    can_edit(&u, row(&s, id).await?.uploader_id)?;
    let clean = |v: Option<String>| v.map(|t| t.trim().chars().take(300).collect::<String>());
    let (title, author) = (clean(b.title), clean(b.author));
    if title.as_deref() == Some("") {
        return Err(ApiError::bad_request("The title can't be empty"));
    }
    s.db
        .call(move |c| {
            if let Some(t) = title {
                c.execute("UPDATE books SET title = ?2 WHERE id = ?1", params![id, t])?;
            }
            if let Some(a) = author {
                c.execute("UPDATE books SET author = NULLIF(?2, '') WHERE id = ?1", params![id, a])?;
            }
            Ok(())
        })
        .await?;
    Ok(Json(get_book(&s, id).await?))
}

async fn remove(State(s): State<AppState>, u: AuthUser, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let id = parse_id(&id)?;
    let r = row(&s, id).await?;
    can_edit(&u, r.uploader_id)?;
    let work = format!("b{id}");
    s.db
        .call(move |c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM books WHERE id = ?1", [id])?;
            for t in ["library", "progress", "last_read"] {
                tx.execute(&format!("DELETE FROM {t} WHERE work_id = ?1"), [&work])?;
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
    let _ = tokio::fs::remove_file(books_dir(&s).join(format!("{id}.{}", r.ext))).await;
    if let Some(ce) = r.cover_ext {
        let _ = tokio::fs::remove_file(covers_dir(&s).join(format!("b{id}.{ce}"))).await;
    }
    pages_cache().lock().unwrap().remove(&id);
    tracing::info!(user = %u.user.name, %id, "libro borrado");
    Ok(StatusCode::NO_CONTENT)
}

/// `filename*` de Content-Disposition (RFC 5987).
fn disposition(title: &str, ext: &str) -> HeaderValue {
    let name = format!("{title}.{ext}");
    let enc: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    HeaderValue::from_str(&format!("inline; filename*=UTF-8''{enc}")).unwrap_or(HeaderValue::from_static("inline"))
}

async fn file(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>, req: Request) -> ApiResult<Response> {
    let id = parse_id(&id)?;
    let r = row(&s, id).await?;
    let path = books_dir(&s).join(format!("{id}.{}", r.ext));
    // ServeFile hace Range, If-Modified-Since y HEAD; el tipo lo ponemos nosotros.
    let mut res = ServeFile::new(&path).oneshot(req).await.map_err(ApiError::internal)?.into_response();
    if res.status() == StatusCode::NOT_FOUND {
        return Err(ApiError::not_found("The book file is missing"));
    }
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type(&r.format)));
    h.insert(header::CONTENT_DISPOSITION, disposition(&r.title, &r.ext));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"));
    Ok(res)
}

async fn cover(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>) -> ApiResult<Response> {
    let id = parse_id(&id)?;
    let r = row(&s, id).await?;
    let ext = r.cover_ext.ok_or_else(|| ApiError::not_found("No cover"))?;
    let bytes = tokio::fs::read(covers_dir(&s).join(format!("b{id}.{ext}"))).await.map_err(|_| ApiError::not_found("No cover"))?;
    let ct = bookmeta::image_type(&format!("x.{ext}")).unwrap_or("application/octet-stream");
    Ok(([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "private, max-age=86400")], bytes).into_response())
}

async fn put_cover(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>, headers: HeaderMap, body: Bytes) -> ApiResult<StatusCode> {
    let id = parse_id(&id)?;
    let r = row(&s, id).await?;
    let ct = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").split(';').next().unwrap_or("").trim().to_string();
    let ext = match ct.as_str() {
        "image/jpeg" | "image/webp" | "image/png" => bookmeta::ext_for_mime(&ct).unwrap_or("jpg"),
        _ => return Err(ApiError::bad_request("The cover must be image/jpeg, image/webp or image/png")),
    };
    if body.is_empty() || body.len() > MAX_COVER {
        return Err(ApiError::too_large("The cover must be at most 2 MB"));
    }
    let dir = covers_dir(&s);
    tokio::fs::create_dir_all(&dir).await?;
    if let Some(old) = r.cover_ext.filter(|o| o != ext) {
        let _ = tokio::fs::remove_file(dir.join(format!("b{id}.{old}"))).await;
    }
    tokio::fs::write(dir.join(format!("b{id}.{ext}")), &body).await?;
    s.db.call(move |c| Ok(c.execute("UPDATE books SET cover_ext = ?2 WHERE id = ?1", params![id, ext])?)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PagesBody {
    pages: i64,
}

async fn put_pages(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>, Json(b): Json<PagesBody>) -> ApiResult<StatusCode> {
    let id = parse_id(&id)?;
    if !(1..=100_000).contains(&b.pages) {
        return Err(ApiError::bad_request("Invalid page count"));
    }
    let n = s.db.call(move |c| Ok(c.execute("UPDATE books SET pages = ?2 WHERE id = ?1", params![id, b.pages])?)).await?;
    if n == 0 {
        return Err(ApiError::not_found("Book not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Lista de imágenes de cada CBZ ya abierto (leer el índice del zip en cada página es caro).
type PagesCache = Mutex<HashMap<i64, Arc<Vec<String>>>>;

fn pages_cache() -> &'static PagesCache {
    static CACHE: std::sync::OnceLock<PagesCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

async fn cbz_list(s: &AppState, id: i64) -> ApiResult<(PathBuf, Arc<Vec<String>>)> {
    let r = row(s, id).await?;
    if r.format != "cbz" {
        return Err(ApiError::bad_request("Only CBZ books have pages"));
    }
    let path = books_dir(s).join(format!("{id}.{}", r.ext));
    if let Some(v) = pages_cache().lock().unwrap().get(&id) {
        return Ok((path, v.clone()));
    }
    let p = path.clone();
    let list = tokio::task::spawn_blocking(move || bookmeta::cbz_pages(&p))
        .await
        .map_err(ApiError::internal)?
        .map_err(|e| ApiError::internal(format!("cbz {id}: {e:#}")))?;
    let list = Arc::new(list);
    let mut cache = pages_cache().lock().unwrap();
    if cache.len() > 64 {
        cache.clear();
    }
    cache.insert(id, list.clone());
    Ok((path, list))
}

#[derive(Serialize)]
struct PagesOut {
    pages: Vec<String>,
}

async fn pages(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>) -> ApiResult<Json<PagesOut>> {
    let id = parse_id(&id)?;
    let (_, list) = cbz_list(&s, id).await?;
    Ok(Json(PagesOut { pages: (0..list.len()).map(|n| format!("/api/books/{id}/pages/{n}")).collect() }))
}

async fn page(State(s): State<AppState>, _u: AuthUser, Path((id, n)): Path<(String, usize)>) -> ApiResult<Response> {
    let id = parse_id(&id)?;
    let (path, list) = cbz_list(&s, id).await?;
    let name = list.get(n).cloned().ok_or_else(|| ApiError::not_found("Page not found"))?;
    let ct = bookmeta::image_type(&name).unwrap_or("application/octet-stream");
    let bytes = tokio::task::spawn_blocking(move || bookmeta::cbz_page(FsPath::new(&path), &name))
        .await
        .map_err(ApiError::internal)?
        .map_err(|e| ApiError::internal(format!("cbz {id} página {n}: {e:#}")))?;
    let mut res = Response::new(Body::from(bytes));
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
    // El id de un libro nunca se reusa (AUTOINCREMENT): su contenido no cambia.
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"));
    Ok(res)
}
