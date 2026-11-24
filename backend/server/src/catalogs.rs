//! Catálogos de libros gratis y legales: de dónde sacar libros además de subirlos. Uno por fila de
//! la tabla `catalogs`: los de fábrica (Internet Archive en español, Elejandría, textos.info y
//! Project Gutenberg, este último apagado de entrada) y los OPDS que agregue el admin (Calibre, otras bibliotecas). Cada tipo tiene su adaptador
//! (`opds.rs`, `archive.rs`, `elejandria.rs`) y acá está lo común: rutas, búsqueda en todos a la
//! vez (NDJSON como la de manga), importar un libro (se baja en streaming al almacén de libros, con
//! tope de tamaño, y no se importa dos veces lo mismo) y el proxy de portadas.
//!
//! Nada de lo que viene de afuera se pide sin control: las URLs que la UI devuelve (páginas OPDS,
//! portadas, enlaces de descarga) van firmadas por el server (HMAC con una clave al azar de cada
//! arranque), así el proxy de portadas y el importador nunca son un proxy abierto.
use std::{
    collections::HashSet,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Router,
};
use base64::Engine;
use futures_util::{stream, StreamExt};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    archive,
    auth::{AdminUser, AuthUser},
    books::{self, Book, ImportFrom},
    db::now_ms,
    elejandria, opds,
    error::{ApiError, ApiResult, Json, Path, Query},
    state::AppState,
};

/// Los de fábrica que son sobre todo en español (la UI los muestra primero).
const SPANISH: &[&str] = &["archive", "elejandria", "textos"];

/// Cuánto espera la búsqueda a cada catálogo, y cuántos a la vez.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const SEARCH_PARALLEL: usize = 6;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/catalogs", get(list))
        .route("/catalogs/search", get(search))
        .route("/catalogs/{id}/browse", get(browse))
        .route("/catalogs/{id}/entry/{entry}", get(entry))
        .route("/catalogs/{id}/import", post(import))
        .route("/catalogs/{id}/cover/{entry}", get(lazy_cover))
        .route("/img/cat", get(cover_proxy))
        .route("/admin/catalogs", get(admin_list).post(admin_add))
        .route("/admin/catalogs/{id}", patch(admin_edit).delete(admin_remove))
}

/// Los de fábrica. Se crean (o se les actualiza la URL) en cada arranque; el admin puede
/// apagarlos pero no borrarlos.
pub fn presets(s: &AppState) -> Vec<(&'static str, &'static str, Kind, String, bool)> {
    let c = &s.cfg;
    let mut v = Vec::new();
    if !c.archive_url.is_empty() {
        v.push(("archive", "Internet Archive", Kind::Archive, c.archive_url.clone(), true));
    }
    if !c.elejandria_url.is_empty() {
        v.push(("elejandria", "Elejandría", Kind::Elejandria, c.elejandria_url.clone(), true));
    }
    if !c.textos_url.is_empty() {
        v.push(("textos", "textos.info", Kind::Opds, c.textos_url.clone(), true));
    }
    // Casi todo en inglés y lo de español ya está espejado en Internet Archive: apagado de entrada
    // (el admin lo prende en Admin → Catálogos).
    if !c.gutenberg_url.is_empty() {
        v.push(("gutenberg", "Project Gutenberg", Kind::Opds, c.gutenberg_url.clone(), false));
    }
    v
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Opds,
    Archive,
    Elejandria,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Opds => "opds",
            Kind::Archive => "archive",
            Kind::Elejandria => "elejandria",
        }
    }
    fn parse(s: &str) -> Kind {
        match s {
            "archive" => Kind::Archive,
            "elejandria" => Kind::Elejandria,
            _ => Kind::Opds,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    /// Solo para el admin (los usuarios no necesitan ver de dónde sale).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub enabled: bool,
    pub preset: bool,
    /// ¿Trae sobre todo libros en español? (la UI los muestra primero)
    pub spanish: bool,
    #[serde(skip)]
    pub base: String,
    #[serde(skip)]
    pub k: Kind,
}

/// Un libro de un catálogo, como lo ve la UI.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Opaco: la UI lo devuelve tal cual para importarlo o pedir el detalle.
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub cover: Option<String>,
    pub language: Option<String>,
    pub year: Option<i64>,
    pub summary: Option<String>,
    /// "epub", "pdf"... los que se pueden bajar.
    pub formats: Vec<String>,
    /// Id del libro (`b12`) si ya se importó.
    pub imported: Option<String>,
    /// Página del libro en el sitio original.
    pub url: Option<String>,
    #[serde(skip)]
    pub key: String,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Nav {
    pub title: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub title: Option<String>,
    pub entries: Vec<Entry>,
    pub navigation: Vec<Nav>,
    /// Para pedir la página siguiente: se manda como `path`.
    pub next: Option<String>,
}

/// Un valor y cuándo se obtuvo (cachés con vencimiento).
pub type Timed<T> = (Instant, T);

/// Lo que cada adaptador necesita: el cliente HTTP, la clave para firmar y cachés.
#[derive(Clone)]
pub struct Catalogs {
    pub http: reqwest::Client,
    key: Arc<[u8; 32]>,
    pub eleja: elejandria::Index,
    pub osd: Arc<Mutex<std::collections::HashMap<String, Timed<Option<String>>>>>,
}

impl Catalogs {
    pub fn new() -> anyhow::Result<Self> {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|e| anyhow::anyhow!("getrandom: {e}"))?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("Mozilla/5.0 (compatible; library-reader/", env!("CARGO_PKG_VERSION"), ")"))
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(6))
            .build()?;
        Ok(Self { http, key: Arc::new(key), eleja: elejandria::Index::default(), osd: Default::default() })
    }

    fn mac(&self, msg: &str) -> String {
        // HMAC-SHA256 (RFC 2104) a mano: sha2 ya está y no vale la pena otra dependencia.
        let mut k = [0u8; 64];
        k[..32].copy_from_slice(&self.key[..]);
        let ipad: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
        let opad: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
        let inner = Sha256::new().chain_update(&ipad).chain_update(msg.as_bytes()).finalize();
        let out = Sha256::new().chain_update(&opad).chain_update(inner).finalize();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&out[..16])
    }

    /// `texto` → `b64(texto).firma`, para que vuelva a la UI y después se pueda creer.
    pub fn sign(&self, text: &str) -> String {
        let b = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text.as_bytes());
        let m = self.mac(&b);
        format!("{b}.{m}")
    }

    pub fn verify(&self, token: &str) -> Option<String> {
        let (b, m) = token.rsplit_once('.')?;
        if !constant_eq(self.mac(b).as_bytes(), m.as_bytes()) {
            return None;
        }
        String::from_utf8(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(b).ok()?).ok()
    }

    /// URL de portada que la UI puede pedir (`/api/img/cat?t=...`).
    pub fn cover_url(&self, url: &str) -> String {
        format!("/api/img/cat?t={}", self.sign(url))
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Crea o actualiza los de fábrica (al arrancar) y prepara el índice de Elejandría de fondo (la
/// primera vez tarda unos segundos: lista del sitio + una página por letra).
pub async fn ensure_presets(s: &AppState) -> ApiResult<()> {
    let r = save_presets(s).await;
    if let Ok(cats) = all_catalogs(s, true).await {
        if let Some(c) = cats.into_iter().find(|c| c.k == Kind::Elejandria) {
            let s = s.clone();
            tokio::spawn(async move {
                let _ = elejandria::warm(&s, &c).await;
            });
        }
    }
    r
}

async fn save_presets(s: &AppState) -> ApiResult<()> {
    let v: Vec<(String, String, String, String, bool)> = presets(s)
        .into_iter()
        .map(|(id, name, k, url, on)| (id.to_string(), name.to_string(), k.as_str().to_string(), url, on))
        .collect();
    let ids: Vec<String> = v.iter().map(|x| x.0.clone()).collect();
    s.db
        .call(move |c| {
            let tx = c.transaction()?;
            for (id, name, kind, url, on) in v {
                tx.execute(
                    "INSERT INTO catalogs (id, name, kind, url, enabled, preset, added_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)
                     ON CONFLICT(id) DO UPDATE SET name = excluded.name, kind = excluded.kind, url = excluded.url, preset = 1",
                    params![id, name, kind, url, on, now_ms()],
                )?;
            }
            // Los de fábrica que ya no están en la configuración (URL vacía) desaparecen.
            let mut st = tx.prepare("SELECT id FROM catalogs WHERE preset = 1")?;
            let old: Vec<String> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
            drop(st);
            for id in old.into_iter().filter(|i| !ids.contains(i)) {
                tx.execute("DELETE FROM catalogs WHERE id = ?1", [id])?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
}

pub async fn all_catalogs(s: &AppState, only_enabled: bool) -> ApiResult<Vec<Catalog>> {
    s.db
        .call(move |c| {
            let mut st = c.prepare_cached("SELECT id, name, kind, url, enabled, preset FROM catalogs ORDER BY preset DESC, CASE id WHEN 'elejandria' THEN 0 WHEN 'textos' THEN 1 WHEN 'archive' THEN 2 ELSE 3 END, added_at, id")?;
            let rows = st
                .query_map([], |r| {
                    let kind: String = r.get(2)?;
                    let k = Kind::parse(&kind);
                    let id: String = r.get(0)?;
                    Ok(Catalog {
                        spanish: SPANISH.contains(&id.as_str()),
                        name: r.get(1)?,
                        kind: k.as_str(),
                        url: None,
                        base: r.get(3)?,
                        enabled: r.get(4)?,
                        preset: r.get(5)?,
                        k,
                        id,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows.into_iter().filter(|c| c.enabled || !only_enabled).collect())
        })
        .await
}

async fn get_catalog(s: &AppState, id: &str) -> ApiResult<Catalog> {
    all_catalogs(s, false)
        .await?
        .into_iter()
        .find(|c| c.id == id && c.enabled)
        .ok_or_else(|| ApiError::not_found("Catalog not found"))
}

async fn list(State(s): State<AppState>, _u: AuthUser) -> ApiResult<Json<Vec<Catalog>>> {
    Ok(Json(all_catalogs(&s, true).await?))
}

#[derive(Deserialize)]
struct BrowseQuery {
    q: Option<String>,
    path: Option<String>,
    /// "es" (por defecto) o "all".
    lang: Option<String>,
}

fn spanish_only(lang: &Option<String>) -> bool {
    lang.as_deref() != Some("all")
}

/// Una página de un catálogo, ya con `imported` marcado.
async fn page_of(s: &AppState, cat: &Catalog, q: Option<&str>, path: Option<&str>, es: bool) -> ApiResult<Page> {
    let q = q.map(str::trim).filter(|v| !v.is_empty());
    let mut page = match cat.k {
        Kind::Opds => opds::browse(s, cat, q, path, es).await?,
        Kind::Archive => archive::browse(s, cat, q, path, es).await?,
        Kind::Elejandria => elejandria::browse(s, cat, q, path).await?,
    };
    mark_imported(s, &mut page.entries).await?;
    Ok(page)
}

/// Marca los que ya están en la biblioteca de libros (por la URL de origen).
pub async fn mark_imported(s: &AppState, entries: &mut [Entry]) -> ApiResult<()> {
    let keys: Vec<String> = entries.iter().map(|e| e.key.clone()).filter(|k| !k.is_empty()).collect();
    if keys.is_empty() {
        return Ok(());
    }
    let found: Vec<(String, i64)> = s
        .db
        .call(move |c| {
            let mut st = c.prepare_cached("SELECT id FROM books WHERE source_url = ?1 LIMIT 1")?;
            let mut out = Vec::new();
            for k in keys {
                if let Some(id) = st.query_row([&k], |r| r.get::<_, i64>(0)).optional()? {
                    out.push((k, id));
                }
            }
            Ok(out)
        })
        .await?;
    for e in entries.iter_mut() {
        if let Some((_, id)) = found.iter().find(|(k, _)| *k == e.key) {
            e.imported = Some(format!("b{id}"));
        }
    }
    Ok(())
}

async fn browse(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>, Query(q): Query<BrowseQuery>) -> ApiResult<Json<Page>> {
    let cat = get_catalog(&s, &id).await?;
    Ok(Json(page_of(&s, &cat, q.q.as_deref(), q.path.as_deref(), spanish_only(&q.lang)).await?))
}

/// Detalle de un libro (sinopsis, título real) cuando la lista no lo trae entero.
async fn entry(State(s): State<AppState>, _u: AuthUser, Path((id, entry)): Path<(String, String)>) -> ApiResult<Json<Entry>> {
    let cat = get_catalog(&s, &id).await?;
    let mut e = match cat.k {
        Kind::Archive => archive::entry(&s, &cat, &entry).await?,
        Kind::Elejandria => elejandria::entry(&s, &cat, &entry).await?,
        Kind::Opds => return Err(ApiError::not_found("This catalog has no detail page")),
    };
    mark_imported(&s, std::slice::from_mut(&mut e)).await?;
    Ok(Json(e))
}

#[derive(Serialize)]
struct SearchLine<'a> {
    catalog: &'a Catalog,
    entries: Vec<Entry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Busca en todos los catálogos prendidos a la vez; una línea por catálogo apenas contesta.
async fn search(State(s): State<AppState>, _u: AuthUser, Query(q): Query<BrowseQuery>) -> ApiResult<Response> {
    let text = q.q.as_deref().unwrap_or("").trim().to_string();
    if text.chars().count() < 2 {
        return Err(ApiError::bad_request("q is required"));
    }
    let es = spanish_only(&q.lang);
    let cats = all_catalogs(&s, true).await?;
    let lines = stream::iter(cats)
        .map(move |cat| {
            let (s, text) = (s.clone(), text.clone());
            async move {
                let r = tokio::time::timeout(SEARCH_TIMEOUT, page_of(&s, &cat, Some(&text), None, es)).await;
                let (entries, error) = match r {
                    Ok(Ok(p)) => (p.entries, None),
                    Ok(Err(e)) => (Vec::new(), Some(e.message)),
                    Err(_) => (Vec::new(), Some("The catalog took too long to answer".to_string())),
                };
                let mut line = serde_json::to_vec(&SearchLine { catalog: &cat, entries, error }).unwrap_or_default();
                line.push(b'\n');
                Ok::<Bytes, Infallible>(Bytes::from(line))
            }
        })
        .buffer_unordered(SEARCH_PARALLEL);
    let mut res = Response::new(Body::from_stream(lines));
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-ndjson"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(res)
}

/// Lo que la UI ya sabe del libro (de la lista): se usa si el archivo no trae esos datos.
#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub summary: Option<String>,
    pub year: Option<i64>,
}

#[derive(Deserialize)]
struct ImportBody {
    entry: String,
    #[serde(default)]
    hint: Option<Hint>,
}

/// Qué bajar para un libro: la URL del archivo, su formato y la clave de origen (para no
/// importarlo dos veces).
pub struct Download {
    pub url: String,
    pub format: &'static str,
    pub ext: &'static str,
    pub key: String,
    pub hint: Hint,
    pub cover: Option<String>,
}

async fn import(State(s): State<AppState>, u: AuthUser, Path(id): Path<String>, Json(b): Json<ImportBody>) -> ApiResult<Response> {
    let cat = get_catalog(&s, &id).await?;
    let mut dl = match cat.k {
        Kind::Opds => opds::download(&s, &cat, &b.entry)?,
        Kind::Archive => archive::download(&s, &cat, &b.entry).await?,
        Kind::Elejandria => elejandria::download(&s, &cat, &b.entry).await?,
    };
    // Lo de la lista completa lo que el adaptador no sabe.
    if let Some(h) = b.hint {
        let d = &mut dl.hint;
        d.title = d.title.take().or(h.title);
        d.author = d.author.take().or(h.author);
        d.language = d.language.take().or(h.language);
        d.summary = d.summary.take().or(h.summary);
        d.year = d.year.or(h.year);
    }
    let key = dl.key.clone();
    let existing: Option<i64> = s
        .db
        .call(move |c| Ok(c.query_row("SELECT id FROM books WHERE source_url = ?1 LIMIT 1", [key], |r| r.get(0)).optional()?))
        .await?;
    if let Some(bid) = existing {
        return Ok((StatusCode::OK, Json(books::get_book(&s, bid).await?)).into_response());
    }
    let book = fetch_into_store(&s, &cat, dl, &u).await?;
    tracing::info!(user = %u.user.name, catalog = %cat.id, id = %book.card.id, "libro importado de un catálogo");
    Ok((StatusCode::CREATED, Json(book)).into_response())
}

/// Baja el archivo en streaming a `books/` y lo registra.
async fn fetch_into_store(s: &AppState, cat: &Catalog, dl: Download, u: &AuthUser) -> ApiResult<Book> {
    let res = s.catalogs.http.get(&dl.url).send().await.map_err(|e| ApiError::source(format!("{} didn't answer: {e}", cat.name)))?;
    if !res.status().is_success() {
        return Err(ApiError::source(format!("{} answered {}", cat.name, res.status())));
    }
    let max = s.cfg.max_upload_bytes;
    if res.content_length().is_some_and(|n| n > max) {
        return Err(ApiError::too_large(format!("The book is larger than {} MB", max / (1024 * 1024))));
    }
    let dir = books::books_dir(s);
    tokio::fs::create_dir_all(&dir).await?;
    let tmp = dir.join(format!("import-{}.part", crate::auth::new_token()));
    let mut guard = books::TmpGuard(Some(tmp.clone()));
    let mut f = tokio::io::BufWriter::with_capacity(256 * 1024, tokio::fs::File::create(&tmp).await?);
    let mut size: u64 = 0;
    let mut head: Vec<u8> = Vec::new();
    let mut body = res.bytes_stream();
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| ApiError::source(format!("The download from {} was cut: {e}", cat.name)))?;
        size += chunk.len() as u64;
        if size > max {
            return Err(ApiError::too_large(format!("The book is larger than {} MB", max / (1024 * 1024))));
        }
        if head.len() < 1024 {
            head.extend_from_slice(&chunk[..chunk.len().min(1024 - head.len())]);
        }
        f.write_all(&chunk).await?;
    }
    f.flush().await?;
    drop(f);
    if size == 0 || !crate::bookmeta::magic_ok(dl.format, &head) {
        return Err(ApiError::source(format!("{} didn't send a valid {} file", cat.name, dl.format.to_uppercase())));
    }
    let name = format!("{}.{}", dl.hint.title.clone().unwrap_or_else(|| "book".into()), dl.ext);
    let from = ImportFrom {
        origin: Some(cat.name.clone()),
        source_url: Some(dl.key.clone()),
        uploader_id: Some(u.user.id),
        uploader: u.user.name.clone(),
        hint: dl.hint.clone(),
        cover_url: dl.cover.clone(),
    };
    let book = books::import_file(s, &tmp, &name, dl.format, dl.ext, size, from).await?;
    guard.0 = None;
    Ok(book)
}

#[derive(Deserialize)]
struct CoverQuery {
    t: String,
}

/// Portadas de los catálogos por el server (la CSP no deja pedir imágenes afuera). Solo URLs que
/// firmó el server.
async fn cover_proxy(State(s): State<AppState>, _u: AuthUser, Query(q): Query<CoverQuery>) -> ApiResult<Response> {
    let url = s.catalogs.verify(&q.t).ok_or_else(|| ApiError::not_found("Not found"))?;
    proxy_image(&s, &url).await
}

pub async fn proxy_image(s: &AppState, url: &str) -> ApiResult<Response> {
    let up = s.catalogs.http.get(url).send().await.map_err(|e| ApiError::source(format!("The image didn't load: {e}")))?;
    if !up.status().is_success() {
        return Err(ApiError::not_found("Image not found"));
    }
    let ct = up
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|c| c.starts_with("image/"))
        .map(str::to_string)
        .ok_or_else(|| ApiError::not_found("Not an image"))?;
    let len = up.content_length();
    let mut res = Body::from_stream(up.bytes_stream()).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(&ct).unwrap_or(HeaderValue::from_static("image/jpeg")));
    if let Some(n) = len {
        h.insert(header::CONTENT_LENGTH, HeaderValue::from(n));
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=604800"));
    Ok(res)
}

/// Portada que hay que buscar en la página del libro (Elejandría no la tiene en la lista).
async fn lazy_cover(State(s): State<AppState>, _u: AuthUser, Path((id, entry)): Path<(String, String)>) -> ApiResult<Response> {
    let cat = get_catalog(&s, &id).await?;
    let url = match cat.k {
        Kind::Elejandria => elejandria::cover_of(&s, &cat, &entry).await?,
        _ => None,
    };
    match url {
        Some(u) => proxy_image(&s, &u).await,
        None => Err(ApiError::not_found("No cover")),
    }
}

// ---------- admin ----------

async fn admin_list(State(s): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<Catalog>>> {
    let mut v = all_catalogs(&s, false).await?;
    for c in v.iter_mut() {
        c.url = Some(c.base.clone());
    }
    Ok(Json(v))
}

#[derive(Deserialize)]
struct AddBody {
    url: String,
    name: Option<String>,
}

/// Agrega un catálogo OPDS por URL: se prueba antes (tiene que contestar un feed OPDS válido).
async fn admin_add(State(s): State<AppState>, _a: AdminUser, Json(b): Json<AddBody>) -> ApiResult<Json<Catalog>> {
    let url = b.url.trim().to_string();
    let parsed = url::Url::parse(&url).map_err(|_| ApiError::bad_request("That isn't a valid URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ApiError::bad_request("The URL must be http or https"));
    }
    let feed = opds::fetch_feed(&s, &url).await.map_err(|e| ApiError::bad_request(format!("That URL isn't an OPDS catalog: {}", e.message)))?;
    let name = b
        .name
        .map(|n| n.trim().chars().take(80).collect::<String>())
        .filter(|n| !n.is_empty())
        .or(feed.title.clone())
        .unwrap_or_else(|| parsed.host_str().unwrap_or("OPDS").to_string());
    let (u2, n2) = (url.clone(), name.clone());
    let id: String = s
        .db
        .call(move |c| {
            let exists: Option<String> = c.query_row("SELECT id FROM catalogs WHERE url = ?1", [&u2], |r| r.get(0)).optional()?;
            if exists.is_some() {
                return Err(ApiError::conflict("That catalog is already there"));
            }
            let n: i64 = c.query_row("SELECT COUNT(*) FROM catalogs", [], |r| r.get(0))?;
            let mut id = format!("o{}", n + 1);
            while c.query_row("SELECT 1 FROM catalogs WHERE id = ?1", [&id], |_| Ok(())).optional()?.is_some() {
                id.push('x');
            }
            c.execute(
                "INSERT INTO catalogs (id, name, kind, url, enabled, preset, added_at) VALUES (?1, ?2, 'opds', ?3, 1, 0, ?4)",
                params![id, n2, u2, now_ms()],
            )?;
            Ok(id)
        })
        .await?;
    let mut c = all_catalogs(&s, false).await?.into_iter().find(|c| c.id == id).ok_or_else(|| ApiError::internal("catalog vanished"))?;
    c.url = Some(c.base.clone());
    Ok(Json(c))
}

#[derive(Deserialize)]
struct EditBody {
    enabled: Option<bool>,
    name: Option<String>,
}

async fn admin_edit(State(s): State<AppState>, _a: AdminUser, Path(id): Path<String>, Json(b): Json<EditBody>) -> ApiResult<StatusCode> {
    let name = b.name.map(|n| n.trim().chars().take(80).collect::<String>()).filter(|n| !n.is_empty());
    let n = s
        .db
        .call(move |c| {
            let mut n = 0;
            if let Some(e) = b.enabled {
                n += c.execute("UPDATE catalogs SET enabled = ?2 WHERE id = ?1", params![id, e])?;
            }
            if let Some(name) = name {
                n += c.execute("UPDATE catalogs SET name = ?2 WHERE id = ?1 AND preset = 0", params![id, name])?;
            }
            Ok(n)
        })
        .await?;
    if n == 0 {
        return Err(ApiError::not_found("Catalog not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_remove(State(s): State<AppState>, _a: AdminUser, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let n = s.db.call(move |c| Ok(c.execute("DELETE FROM catalogs WHERE id = ?1 AND preset = 0", [id])?)).await?;
    if n == 0 {
        return Err(ApiError::conflict("Built-in catalogs can only be turned off"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Título "lindo" a partir de un slug (`orgullo-y-prejuicio` → `Orgullo y prejuicio`).
pub fn deslug(slug: &str, every_word: bool) -> String {
    let words: Vec<String> = slug
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .enumerate()
        .map(|(i, w)| {
            if i == 0 || every_word {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            } else {
                w.to_string()
            }
        })
        .collect();
    words.join(" ")
}

/// Elige el primero sin repetir (por clave).
pub fn dedupe(entries: &mut Vec<Entry>) {
    let mut seen = HashSet::new();
    entries.retain(|e| seen.insert(if e.key.is_empty() { e.id.clone() } else { e.key.clone() }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firma_y_verifica() {
        let c = Catalogs::new().unwrap();
        let t = c.sign("https://example.org/a.epub");
        assert_eq!(c.verify(&t).as_deref(), Some("https://example.org/a.epub"));
        let (b, _) = t.rsplit_once('.').unwrap();
        assert!(c.verify(&format!("{b}.AAAAAAAAAAAAAAAAAAAAAA")).is_none());
        let other = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("https://evil.example/");
        assert!(c.verify(&format!("{other}.{}", t.rsplit_once('.').unwrap().1)).is_none());
        // Otra clave (otro arranque) no la acepta.
        assert!(Catalogs::new().unwrap().verify(&t).is_none());
    }

    #[test]
    fn slugs() {
        assert_eq!(deslug("orgullo-y-prejuicio", false), "Orgullo y prejuicio");
        assert_eq!(deslug("jane-austen", true), "Jane Austen");
    }
}
