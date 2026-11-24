//! Completa los datos de un libro recién importado con Open Library (autor, año, idioma,
//! sinopsis y portada) cuando el archivo y el catálogo no los traían. Es "mejor esfuerzo": corre
//! aparte después de importar (nunca la demora), de a una consulta por vez con una pausa entre
//! cada una, y recuerda lo que no encontró para no volver a preguntar.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::{
    anilist::normalize,
    bookmeta::{normalize_lang, plain_text},
    books,
    state::AppState,
};

/// Pausa entre consultas (Open Library pide no pasar de ~1 por segundo sin identificarse).
const GAP: Duration = Duration::from_millis(1100);
const MISS_TTL: Duration = Duration::from_secs(24 * 3600);

#[derive(Clone)]
pub struct OpenLibrary {
    base: String,
    covers: String,
    gate: Arc<Semaphore>,
    misses: Arc<Mutex<HashMap<String, Instant>>>,
}

impl OpenLibrary {
    pub fn new(base: &str) -> Self {
        let base = base.trim_end_matches('/').to_string();
        // Las portadas viven en otro dominio (covers.openlibrary.org); con un server falso
        // (tests) salen del mismo.
        let covers = if base == "https://openlibrary.org" { "https://covers.openlibrary.org".to_string() } else { base.clone() };
        Self { base, covers, gate: Arc::new(Semaphore::new(1)), misses: Default::default() }
    }

    pub fn enabled(&self) -> bool {
        !self.base.is_empty()
    }
}

struct Missing {
    title: String,
    author: Option<String>,
    need_author: bool,
    need_year: bool,
    need_lang: bool,
    need_desc: bool,
    need_cover: bool,
}

/// Lanza el completado en segundo plano.
pub fn spawn_enrich(s: &AppState, id: i64) {
    if !s.openlib.enabled() {
        return;
    }
    let s = s.clone();
    tokio::spawn(async move {
        if let Err(e) = enrich(&s, id).await {
            tracing::debug!(%id, "Open Library: {e:#}");
        }
    });
}

pub async fn enrich(s: &AppState, id: i64) -> anyhow::Result<()> {
    let m = s
        .db
        .call(move |c| {
            Ok(c.query_row(
                "SELECT title, author, year IS NULL, language IS NULL, description IS NULL, cover_ext IS NULL FROM books WHERE id = ?1",
                [id],
                |r| {
                    let author: Option<String> = r.get(1)?;
                    Ok(Missing {
                        title: r.get(0)?,
                        need_author: author.is_none(),
                        author,
                        need_year: r.get(2)?,
                        need_lang: r.get(3)?,
                        need_desc: r.get(4)?,
                        need_cover: r.get(5)?,
                    })
                },
            )
            .optional()?)
        })
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    let Some(m) = m else { return Ok(()) };
    if !(m.need_author || m.need_year || m.need_lang || m.need_desc || m.need_cover) {
        return Ok(());
    }
    let key = normalize(&format!("{} {}", m.title, m.author.as_deref().unwrap_or("")));
    if s.openlib.misses.lock().unwrap().get(&key).is_some_and(|t| t.elapsed() < MISS_TTL) {
        return Ok(());
    }
    let _permit = s.openlib.gate.acquire().await?;
    let http = &s.catalogs.http;
    let q = format!("{} {}", m.title, m.author.as_deref().unwrap_or(""));
    let mut u = url::Url::parse(&format!("{}/search.json", s.openlib.base))?;
    u.query_pairs_mut().append_pair("q", q.trim()).append_pair("limit", "5").append_pair("fields", "key,title,author_name,first_publish_year,cover_i,language");
    let v: Value = http
        .get(u.as_str())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    tokio::time::sleep(GAP).await;
    let nt = normalize(&m.title);
    let docs = v["docs"].as_array().cloned().unwrap_or_default();
    // El primero cuyo título coincida; si hay uno en español, ese.
    let matching: Vec<&Value> = docs.iter().filter(|d| d["title"].as_str().map(normalize).is_some_and(|t| t == nt || t.starts_with(&nt) || nt.starts_with(&t))).collect();
    let doc = matching
        .iter()
        .find(|d| d["language"].as_array().is_some_and(|l| l.iter().any(|x| x == "spa")))
        .or_else(|| matching.first())
        .copied();
    let Some(doc) = doc else {
        s.openlib.misses.lock().unwrap().insert(key, Instant::now());
        return Ok(());
    };
    let author = if m.need_author { doc["author_name"].as_array().and_then(|a| a.first()).and_then(Value::as_str).map(str::to_string) } else { None };
    let year = if m.need_year { doc["first_publish_year"].as_i64() } else { None };
    let lang = if m.need_lang {
        doc["language"].as_array().and_then(|l| if l.iter().any(|x| x == "spa") { Some("es".to_string()) } else { l.first().and_then(Value::as_str).and_then(normalize_lang) })
    } else {
        None
    };
    let mut desc = None;
    if m.need_desc {
        if let Some(k) = doc["key"].as_str().filter(|k| k.starts_with("/works/")) {
            if let Ok(r) = http.get(format!("{}{k}.json", s.openlib.base)).send().await {
                if let Ok(w) = r.json::<Value>().await {
                    desc = match &w["description"] {
                        Value::String(t) => Some(t.clone()),
                        Value::Object(o) => o.get("value").and_then(Value::as_str).map(str::to_string),
                        _ => None,
                    }
                    .map(|t| plain_text(&t, 4000))
                    .filter(|t| !t.is_empty());
                }
            }
            tokio::time::sleep(GAP).await;
        }
    }
    s.db
        .call(move |c| {
            c.execute(
                "UPDATE books SET author = COALESCE(author, ?2), year = COALESCE(year, ?3), language = COALESCE(language, ?4),
                        description = COALESCE(description, ?5) WHERE id = ?1",
                params![id, author, year, lang, desc],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    if m.need_cover {
        if let Some(cid) = doc["cover_i"].as_i64() {
            let url = format!("{}/b/id/{cid}-L.jpg?default=false", s.openlib.covers);
            let _ = books::save_cover_from_url(s, id, &url).await;
        }
    }
    Ok(())
}
