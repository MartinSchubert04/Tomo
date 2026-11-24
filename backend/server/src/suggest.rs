//! Sugerencias mientras se escribe en la búsqueda (`GET /api/suggest?q=`) y el proxy de portadas
//! de AniList (`/api/img/al/...`).
//!
//! Junta dos cosas, rápido:
//! 1. Lo conocido: las tarjetas guardadas (`works`: todo manga que alguien abrió, agregó o leyó) y
//!    los libros subidos, primero lo que está en la biblioteca o el historial del usuario.
//! 2. AniList: títulos en otros idiomas (`anilist.rs`). Si uno de los títulos de una obra de
//!    AniList coincide con una tarjeta conocida, se sugiere esa tarjeta (`kind: "work"`, abre la
//!    ficha) con los otros títulos; si no, una sugerencia `kind: "title"`, que la UI usa para la
//!    búsqueda global con todas sus variantes (`/api/search?alt=`).
use std::time::Duration;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{
    anilist::{allowed_cover_path, normalize, proxied_cover, Media},
    auth::AuthUser,
    error::{ApiError, ApiResult, Json, Path, Query},
    library,
    state::AppState,
};

const LOCAL_MAX: usize = 6;
const REMOTE_MAX: usize = 6;
const ANILIST_TIMEOUT: Duration = Duration::from_millis(2500);

pub fn routes() -> Router<AppState> {
    Router::new().route("/suggest", get(suggest)).route("/img/al/{*path}", get(cover))
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// "work": algo que ya existe acá (abre la ficha o el libro); "title": un título de AniList
    /// para buscar en las fuentes.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    pub title: String,
    pub alt_titles: Vec<String>,
    pub cover: Option<String>,
    /// manga | manhwa | manhua | novel (de AniList) o el formato del libro.
    pub format: Option<String>,
    pub year: Option<i64>,
    /// Fuente de la tarjeta ("InManga (ES)", "Upload").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Deserialize)]
struct SuggestQuery {
    q: String,
}

/// Una tarjeta conocida, con cuánto le importa al usuario (2 = en su biblioteca, 1 = en su
/// historial, 0 = solo vista por alguien).
struct Known {
    id: String,
    title: String,
    norm: String,
    cover: Option<String>,
    source: Option<String>,
    format: Option<String>,
    mine: i64,
}

async fn known_cards(s: &AppState, uid: i64) -> ApiResult<Vec<Known>> {
    s.db.call(move |c| {
        let mut out = Vec::new();
        let mut st = c.prepare_cached(
            "SELECT w.id, w.title, w.cover, w.source,
                    (CASE WHEN l.work_id IS NOT NULL THEN 2 WHEN lr.work_id IS NOT NULL THEN 1 ELSE 0 END)
             FROM works w
             LEFT JOIN library l ON l.user_id = ?1 AND l.work_id = w.id
             LEFT JOIN last_read lr ON lr.user_id = ?1 AND lr.work_id = w.id",
        )?;
        let rows = st.query_map(params![uid], |r| {
            let title: String = r.get(1)?;
            Ok(Known { id: r.get(0)?, norm: normalize(&title), title, cover: r.get(2)?, source: r.get(3)?, format: None, mine: r.get(4)? })
        })?;
        for k in rows {
            out.push(k?);
        }
        let mut st = c.prepare_cached(
            "SELECT b.id, b.title, b.format, b.cover_ext IS NOT NULL,
                    (CASE WHEN l.work_id IS NOT NULL THEN 2 WHEN lr.work_id IS NOT NULL THEN 1 ELSE 0 END)
             FROM books b
             LEFT JOIN library l ON l.user_id = ?1 AND l.work_id = 'b' || b.id
             LEFT JOIN last_read lr ON lr.user_id = ?1 AND lr.work_id = 'b' || b.id",
        )?;
        let rows = st.query_map(params![uid], |r| {
            let id: i64 = r.get(0)?;
            let title: String = r.get(1)?;
            let has_cover: bool = r.get(3)?;
            Ok(Known {
                id: format!("b{id}"),
                norm: normalize(&title),
                title,
                cover: has_cover.then(|| format!("/api/books/{id}/cover")),
                source: Some("Upload".into()),
                format: r.get(2)?,
                mine: r.get(4)?,
            })
        })?;
        for k in rows {
            out.push(k?);
        }
        Ok(out)
    })
    .await
}

/// Qué tan bien coincide un título normalizado con lo buscado: 0 igual, 1 empieza igual, 2 una
/// palabra empieza igual, 3 lo contiene. `None` = no coincide.
fn rank(title: &str, q: &str) -> Option<u8> {
    if title == q {
        Some(0)
    } else if title.starts_with(q) {
        Some(1)
    } else if title.split(' ').any(|w| w.starts_with(q)) || title.contains(&format!(" {q}")) {
        Some(2)
    } else if title.contains(q) {
        Some(3)
    } else {
        None
    }
}

fn work_suggestion(k: &Known, alts: Vec<String>) -> Suggestion {
    Suggestion {
        kind: "work",
        work_id: Some(k.id.clone()),
        title: k.title.clone(),
        alt_titles: alts,
        cover: k.cover.clone(),
        format: k.format.clone(),
        year: None,
        source: k.source.clone(),
    }
}

/// Arma la lista final (separado del endpoint para poder probarlo sin red).
fn merge(q: &str, known: &[Known], remote: &[Media], nsfw: bool) -> Vec<Suggestion> {
    let nq = normalize(q);
    // 1. Lo conocido que coincide: lo del usuario primero, después lo que mejor coincide.
    let mut local: Vec<(&Known, u8)> = known.iter().filter_map(|k| rank(&k.norm, &nq).map(|r| (k, r))).collect();
    local.sort_by(|a, b| b.0.mine.cmp(&a.0.mine).then(a.1.cmp(&b.1)).then(a.0.title.len().cmp(&b.0.title.len())));
    let mut out: Vec<Suggestion> = local.iter().take(LOCAL_MAX).map(|(k, _)| work_suggestion(k, Vec::new())).collect();

    // 2. AniList: si un título coincide con una tarjeta conocida, esa tarjeta (con los otros
    //    nombres); si no, el título para buscar.
    let mut remote_n = 0;
    for m in remote {
        if remote_n >= REMOTE_MAX {
            break;
        }
        if m.adult && !nsfw {
            continue;
        }
        let variants = m.variants();
        let norms: Vec<String> = variants.iter().map(|v| normalize(v)).collect();
        let hit = known.iter().filter(|k| norms.contains(&k.norm)).max_by_key(|k| k.mine);
        if let Some(k) = hit {
            let alts: Vec<String> = variants.iter().filter(|v| normalize(v) != k.norm).cloned().collect();
            if let Some(existing) = out.iter_mut().find(|s| s.work_id.as_deref() == Some(k.id.as_str())) {
                if existing.alt_titles.is_empty() {
                    existing.alt_titles = alts;
                }
            } else {
                out.push(work_suggestion(k, alts));
                remote_n += 1;
            }
            continue;
        }
        let title = m.title();
        if title.is_empty() || out.iter().any(|s| s.kind == "title" && normalize(&s.title) == normalize(&title)) {
            continue;
        }
        out.push(Suggestion {
            kind: "title",
            work_id: None,
            alt_titles: variants.into_iter().filter(|v| *v != title).collect(),
            title,
            cover: m.cover.as_deref().and_then(proxied_cover),
            format: m.format.map(str::to_string),
            year: m.year,
            source: None,
        });
        remote_n += 1;
    }
    out
}

async fn suggest(State(s): State<AppState>, u: AuthUser, Query(q): Query<SuggestQuery>) -> ApiResult<Json<Vec<Suggestion>>> {
    let text = q.q.trim().to_string();
    if text.chars().count() < 2 || text.len() > 200 {
        return Ok(Json(Vec::new()));
    }
    let uid = u.user.id;
    let (known, remote, nsfw) = tokio::join!(known_cards(&s, uid), s.anilist.search(&text, ANILIST_TIMEOUT), library::wants_nsfw(&s.db, uid));
    Ok(Json(merge(&text, &known?, &remote, nsfw.unwrap_or(false))))
}

/// Las otras formas de buscar `q` (para `/api/search?resolve=1`): si AniList conoce una obra con
/// ese nombre en otro idioma, sus títulos (romaji primero). Vacío si `q` ya es el principal.
pub async fn resolve(s: &AppState, q: &str, timeout: Duration) -> Vec<String> {
    let nq = normalize(q);
    let found = s.anilist.search(q, timeout).await;
    let Some(m) = found.iter().find(|m| m.variants().iter().any(|v| rank(&normalize(v), &nq).is_some_and(|r| r <= 1))) else {
        return Vec::new();
    };
    if normalize(&m.title()) == nq {
        return Vec::new();
    }
    // El nativo (japonés, coreano) no sirve en las fuentes en español.
    let native = m.native.as_deref().map(normalize);
    m.variants().into_iter().filter(|v| Some(normalize(v)) != native).collect()
}

/// Portadas de AniList por el server (la CSP no deja pedir imágenes a otros dominios).
async fn cover(State(s): State<AppState>, _u: AuthUser, Path(path): Path<String>) -> ApiResult<Response> {
    if !allowed_cover_path(&path) {
        return Err(ApiError::not_found("Not found"));
    }
    let up = s
        .anilist_img
        .get(format!("https://s4.anilist.co/{path}"))
        .send()
        .await
        .map_err(|e| ApiError::source(format!("AniList didn't answer: {e}")))?;
    if !up.status().is_success() {
        return Err(ApiError::not_found("Image not found"));
    }
    let ct = up
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|c| c.starts_with("image/"))
        .unwrap_or("image/jpeg")
        .to_string();
    let len = up.content_length();
    let mut res = Body::from_stream(up.bytes_stream()).into_response();
    *res.status_mut() = StatusCode::OK;
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(&ct).unwrap_or(HeaderValue::from_static("image/jpeg")));
    if let Some(n) = len {
        h.insert(header::CONTENT_LENGTH, HeaderValue::from(n));
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(id: &str, title: &str, mine: i64) -> Known {
        Known { id: id.into(), title: title.into(), norm: normalize(title), cover: None, source: Some("X (ES)".into()), format: None, mine }
    }

    fn media(romaji: &str, english: &str) -> Media {
        Media {
            id: 1,
            romaji: Some(romaji.into()),
            english: Some(english.into()),
            native: Some("進撃の巨人".into()),
            synonyms: vec![],
            cover: Some("https://s4.anilist.co/file/anilistcdn/media/manga/cover/small/x.jpg".into()),
            format: Some("manga"),
            year: Some(2009),
            adult: false,
        }
    }

    #[test]
    fn local_first_then_cross_language() {
        let k = vec![known("s1", "Shingeki no Kyojin", 2), known("s2", "Attack Stories", 0)];
        let r = merge("attack", &k, &[media("Shingeki no Kyojin", "Attack on Titan")], false);
        // "Attack Stories" coincide por texto; "Shingeki no Kyojin" llega por AniList con sus
        // otros títulos y abre la tarjeta que ya existe.
        assert_eq!(r[0].work_id.as_deref(), Some("s2"));
        assert_eq!(r[1].work_id.as_deref(), Some("s1"));
        assert_eq!(r[1].alt_titles, ["Attack on Titan", "進撃の巨人"]);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn unknown_titles_and_adult() {
        let mut adult = media("Ero", "Ero");
        adult.adult = true;
        let r = merge("titan", &[], &[media("Shingeki no Kyojin", "Attack on Titan"), adult.clone()], false);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, "title");
        assert_eq!(r[0].title, "Shingeki no Kyojin");
        assert_eq!(r[0].cover.as_deref(), Some("/api/img/al/file/anilistcdn/media/manga/cover/small/x.jpg"));
        assert_eq!(merge("titan", &[], &[adult], true).len(), 1);
    }

    #[test]
    fn ranks() {
        assert_eq!(rank("one piece", "one piece"), Some(0));
        assert_eq!(rank("one piece", "one"), Some(1));
        assert_eq!(rank("the one piece", "pie"), Some(2));
        assert_eq!(rank("onepiece", "piece"), Some(3));
        assert_eq!(rank("berserk", "naruto"), None);
    }
}
