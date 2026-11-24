//! Rutas de manga: fuentes instaladas, explorar una fuente, búsqueda global (NDJSON en streaming,
//! una línea por fuente a medida que contestan), ficha con capítulos, páginas de un capítulo y el
//! proxy de imágenes a Suwayomi (`/api/img/s/...`, solo portadas, páginas e íconos).
use std::{collections::HashSet, convert::Infallible, time::Duration};

use axum::{
    body::Body,
    extract::{RawQuery, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use bytes::Bytes;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};

use crate::{
    auth::AuthUser,
    error::{ApiError, ApiResult, Json, Path, Query},
    anilist::normalize,
    library,
    state::AppState,
    suggest,
    suwayomi::{manga_id, MangaDetail, Source, WorkCard},
};

/// Búsqueda global: fuentes en paralelo, tiempo por fuente y tope de fuentes sin elegir.
const SEARCH_PARALLEL: usize = 6;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const SEARCH_DEFAULT_MAX: usize = 12;
/// Búsquedas por fuente como mucho (lo escrito y otros títulos de la misma obra).
const SEARCH_VARIANTS: usize = 3;
/// Cuánto se espera a AniList al apretar Enter sin elegir una sugerencia.
const RESOLVE_TIMEOUT: Duration = Duration::from_millis(1000);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sources", get(sources))
        .route("/sources/{id}/browse", get(browse))
        .route("/search", get(search))
        .route("/manga/{id}", get(manga))
        .route("/manga/{id}/chapters/{chapter}/pages", get(pages))
        .route("/img/s/{*path}", get(image))
}

async fn sources(State(s): State<AppState>, _u: AuthUser) -> ApiResult<Json<Vec<Source>>> {
    Ok(Json(s.suwa.sources().await?))
}

#[derive(Deserialize)]
struct BrowseQuery {
    #[serde(rename = "type")]
    kind: Option<String>,
    q: Option<String>,
    page: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    items: Vec<WorkCard>,
    has_next: bool,
}

fn valid_source_id(id: &str) -> ApiResult<()> {
    if !id.is_empty() && id.len() <= 24 && id.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        Ok(())
    } else {
        Err(ApiError::bad_request("Invalid source id"))
    }
}

async fn browse(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>, Query(q): Query<BrowseQuery>) -> ApiResult<Json<Page>> {
    valid_source_id(&id)?;
    let text = q.q.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let kind = match q.kind.as_deref().unwrap_or("popular") {
        "popular" => "POPULAR",
        "latest" => "LATEST",
        "search" => "SEARCH",
        _ => return Err(ApiError::bad_request("type must be popular, latest or search")),
    };
    if kind == "SEARCH" && text.is_none() {
        return Err(ApiError::bad_request("q is required to search"));
    }
    let (items, has_next) = s.suwa.browse(&id, kind, text, q.page.unwrap_or(1)).await?;
    Ok(Json(Page { items, has_next }))
}

/// `/api/search`: `q`, `sources=id,id`, `alt=` repetible (otros títulos de la misma obra, de
/// `/api/suggest`) y `resolve=1` (sin `alt`, preguntarle a AniList si `q` es el nombre de una obra
/// en otro idioma; ver `suggest::resolve`). `alt` se repite, así que no sirve `Query` (serde_urlencoded
/// no arma listas).
#[derive(Default)]
struct SearchQuery {
    q: String,
    sources: Option<String>,
    alts: Vec<String>,
    resolve: bool,
}

fn parse_search(raw: Option<&str>) -> SearchQuery {
    let mut out = SearchQuery::default();
    for (k, v) in url::form_urlencoded::parse(raw.unwrap_or("").as_bytes()) {
        match k.as_ref() {
            "q" => out.q = v.into_owned(),
            "sources" => out.sources = Some(v.into_owned()),
            "alt" if out.alts.len() < 8 && !v.trim().is_empty() && v.len() <= 200 => out.alts.push(v.trim().to_string()),
            "resolve" => out.resolve = v == "1" || v == "true",
            _ => {}
        }
    }
    out
}

/// Las búsquedas que se le hacen a cada fuente: los otros títulos primero (romaji, inglés) y lo
/// escrito, sin repetir, como mucho `SEARCH_VARIANTS`.
fn search_terms(q: &str, alts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for t in alts.iter().map(String::as_str).chain(std::iter::once(q)) {
        if seen.insert(normalize(t)) {
            out.push(t.to_string());
        }
        if out.len() == SEARCH_VARIANTS {
            break;
        }
    }
    out
}

/// Primera línea del NDJSON cuando se buscó con otros títulos: `{"alts":[...]}`.
#[derive(Serialize)]
struct AltsLine<'a> {
    alts: &'a [String],
}

#[derive(Serialize)]
struct SearchLine<'a> {
    source: &'a Source,
    items: Vec<WorkCard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Qué fuentes usar sin `sources`: las fijadas por el usuario (`prefs.sources`) o las de español
/// y "all", hasta 12.
fn default_sources(all: &[Source], pinned: &[String]) -> Vec<Source> {
    if !pinned.is_empty() {
        let set: HashSet<&str> = pinned.iter().map(String::as_str).collect();
        let v: Vec<Source> = all.iter().filter(|s| set.contains(s.id.as_str())).cloned().collect();
        if !v.is_empty() {
            return v;
        }
    }
    all.iter()
        .filter(|s| matches!(s.lang.as_str(), "es" | "es-419" | "all"))
        .take(SEARCH_DEFAULT_MAX)
        .cloned()
        .collect()
}

async fn search(State(s): State<AppState>, u: AuthUser, RawQuery(raw): RawQuery) -> ApiResult<Response> {
    let q = parse_search(raw.as_deref());
    let text = q.q.trim().to_string();
    if text.is_empty() {
        return Err(ApiError::bad_request("q is required"));
    }
    // Las fuentes y AniList a la vez (AniList tiene 1 s; con caché, nada).
    let (all, resolved) = tokio::join!(s.suwa.sources(), async {
        if q.alts.is_empty() && q.resolve {
            suggest::resolve(&s, &text, RESOLVE_TIMEOUT).await
        } else {
            Vec::new()
        }
    });
    let all = all?;
    let alts: Vec<String> = if q.alts.is_empty() { resolved } else { q.alts.clone() };
    let terms = search_terms(&text, &alts);
    let chosen: Vec<Source> = match q.sources.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        Some(list) => {
            let ids: HashSet<&str> = list.split(',').map(str::trim).collect();
            all.iter().filter(|s| ids.contains(s.id.as_str())).cloned().collect()
        }
        None => {
            let pinned = library::pinned_sources(&s.db, u.user.id).await?;
            default_sources(&all, &pinned)
        }
    };
    let suwa = s.suwa.clone();
    // Si se buscó con otros títulos, la UI lo muestra ("Buscado también como ...").
    let head = (terms.len() > 1).then(|| {
        let shown: Vec<String> = terms.iter().filter(|t| normalize(t) != normalize(&text)).cloned().collect();
        let mut line = serde_json::to_vec(&AltsLine { alts: &shown }).unwrap_or_default();
        line.push(b'\n');
        Ok::<Bytes, Infallible>(Bytes::from(line))
    });
    let lines = stream::iter(chosen)
        .map(move |src| {
            let (suwa, terms) = (suwa.clone(), terms.clone());
            async move {
                // Cada título en paralelo; los resultados se juntan sin repetir (por id), en el
                // orden de los títulos. Error solo si fallaron todos.
                let each = futures_util::future::join_all(terms.iter().map(|t| suwa.browse(&src.id, "SEARCH", Some(t), 1)));
                let res = tokio::time::timeout(SEARCH_TIMEOUT, each).await;
                let (items, error) = match res {
                    Ok(all) => {
                        let mut items: Vec<WorkCard> = Vec::new();
                        let mut seen = HashSet::new();
                        let mut err = None;
                        let mut any_ok = false;
                        for r in all {
                            match r {
                                Ok((found, _)) => {
                                    any_ok = true;
                                    for w in found {
                                        if seen.insert(w.id.clone()) {
                                            items.push(w);
                                        }
                                    }
                                }
                                Err(e) => err = err.or(Some(e.message)),
                            }
                        }
                        (items, if any_ok { None } else { err })
                    }
                    Err(_) => (Vec::new(), Some("The source took too long to answer".to_string())),
                };
                let mut line = serde_json::to_vec(&SearchLine { source: &src, items, error }).unwrap_or_default();
                line.push(b'\n');
                Ok::<Bytes, Infallible>(Bytes::from(line))
            }
        })
        .buffer_unordered(SEARCH_PARALLEL);
    let lines = stream::iter(head).chain(lines);
    let mut res = Response::new(Body::from_stream(lines));
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-ndjson"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    // Que ningún proxy (ni la compresión) junte las líneas antes de mandarlas.
    h.insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(res)
}

#[derive(Deserialize)]
struct MangaQuery {
    refresh: Option<String>,
}

fn parse_manga(id: &str) -> ApiResult<i64> {
    manga_id(id).ok_or_else(|| ApiError::not_found("Not found"))
}

async fn manga(State(s): State<AppState>, _u: AuthUser, Path(id): Path<String>, Query(q): Query<MangaQuery>) -> ApiResult<Json<MangaDetail>> {
    let mid = parse_manga(&id)?;
    let refresh = q.refresh.as_deref().is_some_and(|v| v == "1" || v == "true");
    let d = s.suwa.manga(mid, refresh).await?;
    library::save_manga(&s.db, &d).await?;
    Ok(Json(d))
}

#[derive(Serialize)]
struct Pages {
    pages: Vec<String>,
}

async fn pages(State(s): State<AppState>, _u: AuthUser, Path((id, chapter)): Path<(String, String)>) -> ApiResult<Json<Pages>> {
    parse_manga(&id)?;
    let cid: i64 = chapter.parse().map_err(|_| ApiError::not_found("Not found"))?;
    Ok(Json(Pages { pages: s.suwa.pages(cid).await? }))
}

/// Solo portadas, páginas e íconos de extensiones; nada de `..` ni caracteres raros.
fn allowed_path(p: &str) -> bool {
    (p.starts_with("manga/") || p.starts_with("extension/icon/"))
        && !p.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..")
        && p.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
}

async fn image(State(s): State<AppState>, _u: AuthUser, Path(path): Path<String>) -> ApiResult<Response> {
    if !allowed_path(&path) {
        return Err(ApiError::not_found("Not found"));
    }
    let up = s.suwa.get_raw(&path).await?;
    let status = up.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ApiError::not_found("Image not found"));
    }
    if !status.is_success() {
        return Err(ApiError::source(format!("The source couldn't give the image ({status})")));
    }
    let ctype = up.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_string);
    let len = up.content_length();
    let cache = if path.ends_with("/thumbnail") { "private, max-age=86400" } else { "private, max-age=31536000, immutable" };
    let mut res = Body::from_stream(up.bytes_stream()).into_response();
    *res.status_mut() = StatusCode::OK;
    let h = res.headers_mut();
    let ct = ctype.as_deref().filter(|c| c.starts_with("image/")).unwrap_or("application/octet-stream");
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(ct).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    if let Some(n) = len {
        h.insert(header::CONTENT_LENGTH, HeaderValue::from(n));
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_paths() {
        assert!(allowed_path("manga/1/thumbnail"));
        assert!(allowed_path("manga/1/chapter/1201/page/0"));
        assert!(allowed_path("extension/icon/eu.kanade.tachiyomi.extension.es.inmanga"));
        assert!(!allowed_path("manga/../settings"));
        assert!(!allowed_path("graphql"));
        assert!(!allowed_path("manga//x"));
        assert!(!allowed_path("manga/1?x=y"));
    }

    #[test]
    fn default_search_sources() {
        let mk = |id: &str, lang: &str| Source {
            id: id.into(),
            name: id.into(),
            lang: lang.into(),
            icon: String::new(),
            supports_latest: true,
            nsfw: "safe",
        };
        let all = vec![mk("1", "en"), mk("2", "es"), mk("3", "es-419"), mk("4", "all")];
        let ids: Vec<_> = default_sources(&all, &[]).into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["2", "3", "4"]);
        let ids: Vec<_> = default_sources(&all, &["1".into()]).into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["1"]);
    }
}
