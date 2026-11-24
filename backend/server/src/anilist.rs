//! Títulos de manga en otros idiomas, desde AniList (GraphQL público, sin cuenta). Sirve para que
//! buscar "attack on titan" encuentre "Shingeki no Kyojin" en las fuentes en español: AniList
//! devuelve el romaji, el inglés, el nativo y los sinónimos de cada obra.
//!
//! Cuidados (AniList anda degradado a 30 pedidos por minuto):
//! - caché en memoria por búsqueda normalizada, 24 h, ~500 entradas (se tiran las más viejas);
//! - tope propio de 25 pedidos por minuto para todo el server: pasado eso no se pregunta (las
//!   sugerencias salen solo con lo local) en vez de comerse un 429;
//! - tiempo máximo por pedido (2.5 s en las sugerencias, 1 s al resolver un Enter);
//! - si AniList falla, nunca falla el endpoint: se devuelve vacío.
//!
//! El navegador nunca le habla a AniList: las portadas pasan por `/api/img/al/...`.
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::json;

const CACHE_TTL: Duration = Duration::from_secs(24 * 3600);
const CACHE_MAX: usize = 500;
const PER_MINUTE: usize = 25;

/// Una obra de AniList, ya en nuestros términos.
#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub id: i64,
    pub romaji: Option<String>,
    pub english: Option<String>,
    pub native: Option<String>,
    pub synonyms: Vec<String>,
    /// URL de AniList (`https://s4.anilist.co/file/...`); la API la pasa por el proxy.
    pub cover: Option<String>,
    /// manga | manhwa | manhua | novel
    pub format: Option<&'static str>,
    pub year: Option<i64>,
    pub adult: bool,
}

impl Media {
    /// El título principal (romaji, que es como aparecen en casi todas las fuentes de scans).
    pub fn title(&self) -> String {
        self.romaji.clone().or_else(|| self.english.clone()).or_else(|| self.native.clone()).unwrap_or_default()
    }

    /// Todos los títulos sin repetir (normalizados), el principal primero: romaji, inglés,
    /// sinónimos y nativo al final (en las fuentes en español casi nunca sirve).
    pub fn variants(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let all = self.romaji.iter().chain(self.english.iter()).chain(self.synonyms.iter()).chain(self.native.iter());
        for t in all {
            let t = t.trim();
            if !t.is_empty() && seen.insert(normalize(t)) {
                out.push(t.to_string());
            }
        }
        out
    }
}

#[derive(Clone)]
pub struct AniList {
    url: String,
    http: reqwest::Client,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    cache: HashMap<String, (Instant, Arc<Vec<Media>>)>,
    order: VecDeque<String>,
    sent: VecDeque<Instant>,
}

impl AniList {
    /// `url` vacía = desactivado (siempre devuelve nada).
    pub fn new(url: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .user_agent(concat!("library/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { url: url.trim_end_matches('/').to_string(), http, inner: Arc::new(Mutex::new(Inner::default())) })
    }

    pub fn enabled(&self) -> bool {
        !self.url.is_empty()
    }

    /// Obras que coinciden con `q`. Nunca falla: sin respuesta a tiempo, vacío.
    pub async fn search(&self, q: &str, timeout: Duration) -> Arc<Vec<Media>> {
        let key = normalize(q);
        if !self.enabled() || key.chars().count() < 2 {
            return Arc::default();
        }
        // Caché y tope por minuto (sin await con el lock tomado).
        {
            let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((at, v)) = inner.cache.get(&key) {
                if at.elapsed() < CACHE_TTL {
                    return v.clone();
                }
            }
            let now = Instant::now();
            while inner.sent.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60)) {
                inner.sent.pop_front();
            }
            if inner.sent.len() >= PER_MINUTE {
                tracing::debug!("anilist: tope por minuto, sin preguntar por {q:?}");
                return Arc::default();
            }
            inner.sent.push_back(now);
        }
        let res = match tokio::time::timeout(timeout, self.fetch(q)).await {
            Ok(Ok(v)) => Arc::new(v),
            Ok(Err(e)) => {
                tracing::debug!("anilist: {q:?}: {e:#}");
                return Arc::default();
            }
            Err(_) => {
                tracing::debug!("anilist: {q:?}: tardó más de {timeout:?}");
                return Arc::default();
            }
        };
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if inner.cache.insert(key.clone(), (Instant::now(), res.clone())).is_none() {
            inner.order.push_back(key);
        }
        while inner.order.len() > CACHE_MAX {
            if let Some(old) = inner.order.pop_front() {
                inner.cache.remove(&old);
            }
        }
        res
    }

    async fn fetch(&self, q: &str) -> anyhow::Result<Vec<Media>> {
        const QUERY: &str = "query($q:String){Page(perPage:8){media(search:$q,type:MANGA,sort:SEARCH_MATCH){id title{romaji english native} synonyms coverImage{medium} format countryOfOrigin startDate{year} isAdult}}}";
        let res = self
            .http
            .post(&self.url)
            .json(&json!({ "query": QUERY, "variables": { "q": q } }))
            .send()
            .await?
            .error_for_status()?;
        let body: Resp = res.json().await?;
        Ok(body.data.page.media.into_iter().map(Into::into).collect())
    }
}

#[derive(Deserialize)]
struct Resp {
    data: RespData,
}
#[derive(Deserialize)]
struct RespData {
    #[serde(rename = "Page")]
    page: RespPage,
}
#[derive(Deserialize)]
struct RespPage {
    media: Vec<RawMedia>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMedia {
    id: i64,
    title: RawTitle,
    #[serde(default)]
    synonyms: Vec<String>,
    cover_image: Option<RawCover>,
    format: Option<String>,
    country_of_origin: Option<String>,
    start_date: Option<RawDate>,
    #[serde(default)]
    is_adult: bool,
}
#[derive(Deserialize)]
struct RawTitle {
    romaji: Option<String>,
    english: Option<String>,
    native: Option<String>,
}
#[derive(Deserialize)]
struct RawCover {
    medium: Option<String>,
}
#[derive(Deserialize)]
struct RawDate {
    year: Option<i64>,
}

impl From<RawMedia> for Media {
    fn from(m: RawMedia) -> Self {
        let format = match (m.format.as_deref(), m.country_of_origin.as_deref()) {
            (Some("NOVEL"), _) => Some("novel"),
            (_, Some("KR")) => Some("manhwa"),
            (_, Some("CN" | "TW")) => Some("manhua"),
            (Some(_), _) => Some("manga"),
            _ => None,
        };
        let clean = |s: Option<String>| s.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        Media {
            id: m.id,
            romaji: clean(m.title.romaji),
            english: clean(m.title.english),
            native: clean(m.title.native),
            synonyms: m.synonyms.into_iter().filter(|s| !s.trim().is_empty()).take(6).collect(),
            cover: m.cover_image.and_then(|c| c.medium),
            format,
            year: m.start_date.and_then(|d| d.year),
            adult: m.is_adult,
        }
    }
}

/// La portada de AniList por nuestro proxy: `https://s4.anilist.co/file/x.jpg` → `/api/img/al/file/x.jpg`.
pub fn proxied_cover(url: &str) -> Option<String> {
    let path = url.strip_prefix("https://s4.anilist.co/")?;
    allowed_cover_path(path).then(|| format!("/api/img/al/{path}"))
}

/// Solo archivos de la CDN de AniList, sin `..` ni caracteres raros.
pub fn allowed_cover_path(p: &str) -> bool {
    p.starts_with("file/anilistcdn/")
        && !p.split('/').any(|seg| seg.is_empty() || seg == "." || seg == "..")
        && p.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
}

/// Para comparar títulos: minúsculas, sin tildes ni signos, espacios simples. "¡Shingeki no
/// Kyojin!" y "shingeki  no kyojin" dan lo mismo.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = true;
    for ch in s.chars().flat_map(char::to_lowercase) {
        let ch = fold(ch);
        if ch.is_alphanumeric() {
            out.push(ch);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Letras latinas con tilde → sin tilde (las de español, portugués, francés y el romaji con
/// macrón: "Shōnen" → "shonen").
fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'ē' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ō' | 'ø' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ū' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize("¡Shingeki  no Kyojin!"), "shingeki no kyojin");
        assert_eq!(normalize("Shōnen Jump: Edición"), "shonen jump edicion");
        assert_eq!(normalize("  "), "");
    }

    #[test]
    fn covers() {
        assert_eq!(
            proxied_cover("https://s4.anilist.co/file/anilistcdn/media/manga/cover/small/bx53390-1RsuABC34P9D.jpg").as_deref(),
            Some("/api/img/al/file/anilistcdn/media/manga/cover/small/bx53390-1RsuABC34P9D.jpg")
        );
        assert_eq!(proxied_cover("https://evil.example/file/anilistcdn/x.jpg"), None);
        assert!(!allowed_cover_path("file/anilistcdn/../x"));
    }

    #[test]
    fn variants_order() {
        let m = Media {
            id: 1,
            romaji: Some("Shingeki no Kyojin".into()),
            english: Some("Attack on Titan".into()),
            native: Some("進撃の巨人".into()),
            synonyms: vec!["AoT".into(), "shingeki no kyojin".into()],
            cover: None,
            format: Some("manga"),
            year: Some(2009),
            adult: false,
        };
        assert_eq!(m.variants(), ["Shingeki no Kyojin", "Attack on Titan", "AoT", "進撃の巨人"]);
    }
}
