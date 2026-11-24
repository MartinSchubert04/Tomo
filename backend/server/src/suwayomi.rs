//! Puente a Suwayomi-Server (el contenedor que corre las extensiones de Mihon/Tachiyomi). Le habla
//! por GraphQL (`POST /api/graphql`) y le pide imágenes por `GET /api/v1/...`. Acá se traducen sus
//! tipos a los de la API (`docs/API.md`): la UI nunca ve a Suwayomi.
//!
//! Errores: si no se puede conectar → 503 `unavailable`; si GraphQL devuelve `errors` → 502
//! `source_error` con la primera línea del mensaje (Suwayomi manda el stack trace de Java entero).
use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::ApiError;

#[derive(Clone)]
pub struct Suwayomi {
    base: String,
    http: reqwest::Client,
}

// ---------- tipos de la API (lo que ve la UI) ----------

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub lang: String,
    pub icon: String,
    pub supports_latest: bool,
    pub nsfw: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkCard {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub cover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub id: String,
    pub name: String,
    pub number: f64,
    pub scanlator: Option<String>,
    /// Idioma del capítulo si la fuente lo dice (MangaDex y otras multi-idioma lo ponen como
    /// `[es-la] Grupo` en el scanlator): `es-419`, `en`... `None` = el idioma de la fuente.
    pub lang: Option<String>,
    pub date: i64,
    /// sourceOrder (para guardar; la UI no lo usa).
    #[serde(skip)]
    pub ord: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaDetail {
    #[serde(flatten)]
    pub card: WorkCard,
    pub source_id: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub status: &'static str,
    pub url: Option<String>,
    pub chapters: Vec<Chapter>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    pub pkg: String,
    pub name: String,
    pub lang: String,
    pub version: String,
    pub icon: String,
    pub installed: bool,
    pub has_update: bool,
    pub obsolete: bool,
    pub nsfw: &'static str,
}

// ---------- tipos de Suwayomi ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SSource {
    id: String,
    display_name: String,
    lang: String,
    icon_url: String,
    supports_latest: bool,
    content_warning: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SManga {
    id: i64,
    title: String,
    thumbnail_url: Option<String>,
    #[serde(default)]
    source: Option<SSourceName>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SSourceName {
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SMangaFull {
    id: i64,
    title: String,
    thumbnail_url: Option<String>,
    source_id: String,
    source: Option<SSourceName>,
    author: Option<String>,
    artist: Option<String>,
    description: Option<String>,
    genre: Vec<String>,
    status: String,
    real_url: Option<String>,
    initialized: bool,
    chapters_last_fetched_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SChapter {
    id: i64,
    name: String,
    chapter_number: f64,
    scanlator: Option<String>,
    upload_date: String,
    source_order: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SExtension {
    pkg_name: String,
    name: String,
    lang: String,
    version_name: String,
    icon_url: String,
    is_installed: bool,
    has_update: bool,
    is_obsolete: bool,
    content_warning: String,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
}

const MANGA_FIELDS: &str = "id title thumbnailUrl sourceId source { displayName } author artist description genre status realUrl initialized chaptersLastFetchedAt";
const CHAPTER_FIELDS: &str = "id name chapterNumber scanlator uploadDate sourceOrder";
const EXT_FIELDS: &str = "pkgName name lang versionName iconUrl isInstalled hasUpdate isObsolete contentWarning";

/// Los capítulos se piden de nuevo a la fuente si los que tiene Suwayomi son más viejos que esto.
pub const CHAPTERS_FRESH_S: i64 = 6 * 3600;

pub fn nsfw(cw: &str) -> &'static str {
    match cw {
        "NSFW" => "nsfw",
        "MIXED" => "mixed",
        _ => "safe",
    }
}

fn status(s: &str) -> &'static str {
    match s {
        "ONGOING" => "ongoing",
        "COMPLETED" | "PUBLISHING_FINISHED" => "completed",
        "ON_HIATUS" => "hiatus",
        "CANCELLED" => "cancelled",
        _ => "unknown",
    }
}

/// `/api/v1/manga/1/thumbnail` → `/api/img/s/manga/1/thumbnail`. Lo que no sea de Suwayomi → `None`.
pub fn img_url(path: &str) -> Option<String> {
    path.strip_prefix("/api/v1/").map(|p| format!("/api/img/s/{p}"))
}

/// `s123` → 123.
pub fn manga_id(work: &str) -> Option<i64> {
    work.strip_prefix('s')?.parse().ok().filter(|n: &i64| *n > 0)
}

pub fn work_id(id: i64) -> String {
    format!("s{id}")
}

fn card(m: &SManga) -> WorkCard {
    WorkCard {
        id: work_id(m.id),
        kind: "manga".into(),
        title: m.title.clone(),
        cover: m.thumbnail_url.as_deref().and_then(img_url),
        source: m.source.as_ref().map(|s| s.display_name.clone()),
        format: None,
    }
}

fn chapter(c: SChapter) -> Chapter {
    let (lang, scanlator) = split_lang(c.scanlator.as_deref().unwrap_or(""));
    Chapter {
        id: c.id.to_string(),
        name: c.name,
        number: c.chapter_number,
        scanlator,
        lang,
        date: c.upload_date.parse().unwrap_or(0),
        ord: c.source_order,
    }
}

/// Un código de idioma: 2 o 3 letras y, opcional, `-` y una región (`es`, `es-la`, `pt-br`, `zh-hk`).
fn is_lang_code(code: &str) -> bool {
    let mut parts = code.split(['-', '_']);
    let base = parts.next().unwrap_or("");
    let region = parts.next();
    parts.next().is_none()
        && (2..=3).contains(&base.len())
        && base.bytes().all(|b| b.is_ascii_alphabetic())
        && region.is_none_or(|r| (2..=4).contains(&r.len()) && r.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// `"[es-la] Por amor al arte"` → (`es-419`, `Por amor al arte`). Sin prefijo, sin idioma.
/// es-la y es-419 son lo mismo (español latino): queda `es-419`.
pub fn split_lang(raw: &str) -> (Option<String>, Option<String>) {
    let raw = raw.trim();
    let (lang, rest) = match raw.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((code, rest))
            if is_lang_code(code) =>
        {
            let code = code.to_ascii_lowercase().replace('_', "-");
            let code = if code == "es-la" || code == "es-mx" { "es-419".to_string() } else { code };
            (Some(code), rest.trim())
        }
        _ => (None, raw),
    };
    (lang, (!rest.is_empty()).then(|| rest.to_string()))
}

/// Más nuevo primero.
fn sort_chapters(mut v: Vec<Chapter>) -> Vec<Chapter> {
    v.sort_by_key(|c| std::cmp::Reverse(c.ord));
    v
}

fn extension(e: SExtension) -> Extension {
    Extension {
        icon: img_url(&e.icon_url).unwrap_or_default(),
        pkg: e.pkg_name,
        name: e.name,
        lang: e.lang,
        version: e.version_name,
        installed: e.is_installed,
        has_update: e.has_update,
        obsolete: e.is_obsolete,
        nsfw: nsfw(&e.content_warning),
    }
}

/// Primera línea de un error de GraphQL, sin el prefijo de Suwayomi ni el stack trace.
fn first_line(msg: &str) -> String {
    let line = msg.lines().next().unwrap_or(msg).trim();
    let line = line.split_once(" : ").map(|(_, r)| r).unwrap_or(line);
    line.chars().take(300).collect()
}

impl Suwayomi {
    pub fn new(base: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self { base: base.trim_end_matches('/').to_string(), http })
    }

    /// Una consulta GraphQL. `timeout`: las que van a la fuente pueden tardar.
    async fn gql<T: DeserializeOwned>(&self, query: &str, vars: Value, timeout: Duration) -> Result<T, ApiError> {
        let res = self
            .http
            .post(format!("{}/api/graphql", self.base))
            .timeout(timeout)
            .json(&json!({ "query": query, "variables": vars }))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ApiError::source("The source took too long to answer")
                } else {
                    tracing::warn!("suwayomi no responde: {e}");
                    ApiError::unavailable("The manga server isn't responding")
                }
            })?;
        if !res.status().is_success() {
            return Err(ApiError::source(format!("Manga server error ({})", res.status())));
        }
        #[derive(Deserialize)]
        struct Resp {
            data: Option<Value>,
            errors: Option<Vec<GqlError>>,
        }
        #[derive(Deserialize)]
        struct GqlError {
            message: String,
        }
        let body: Resp = res.json().await.map_err(|e| ApiError::source(format!("Bad answer from the manga server: {e}")))?;
        if let Some(e) = body.errors.as_ref().and_then(|e| e.first()) {
            return Err(ApiError::source(first_line(&e.message)));
        }
        let data = body.data.ok_or_else(|| ApiError::source("Empty answer from the manga server"))?;
        serde_json::from_value(data).map_err(|e| ApiError::source(format!("Unexpected answer from the manga server: {e}")))
    }

    const QUICK: Duration = Duration::from_secs(10);
    const SLOW: Duration = Duration::from_secs(40);

    /// ¿Responde? (para `/api/health`).
    pub async fn ping(&self) -> bool {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct D {
            #[allow(dead_code)]
            about_server: Value,
        }
        self.gql::<D>("{ aboutServer { version } }", json!({}), Duration::from_secs(2)).await.is_ok()
    }

    pub async fn sources(&self) -> Result<Vec<Source>, ApiError> {
        #[derive(Deserialize)]
        struct D {
            sources: Nodes<SSource>,
        }
        let d: D = self
            .gql("{ sources { nodes { id displayName lang iconUrl supportsLatest contentWarning } } }", json!({}), Self::QUICK)
            .await?;
        let mut v: Vec<Source> = d
            .sources
            .nodes
            .into_iter()
            .filter(|s| s.id != "0")
            .map(|s| Source {
                icon: img_url(&s.icon_url).unwrap_or_default(),
                id: s.id,
                name: s.display_name,
                lang: s.lang,
                supports_latest: s.supports_latest,
                nsfw: nsfw(&s.content_warning),
            })
            .collect();
        v.sort_by_cached_key(|s| s.name.to_lowercase());
        Ok(v)
    }

    /// Populares, últimos o búsqueda en una fuente. `kind`: POPULAR, LATEST o SEARCH.
    pub async fn browse(&self, source: &str, kind: &str, q: Option<&str>, page: u32) -> Result<(Vec<WorkCard>, bool), ApiError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct D {
            fetch_source_manga: P,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct P {
            has_next_page: bool,
            mangas: Vec<SManga>,
        }
        let query = "mutation($source: LongString!, $type: FetchSourceMangaType!, $page: Int!, $query: String) {
            fetchSourceManga(input: { source: $source, type: $type, page: $page, query: $query }) {
                hasNextPage mangas { id title thumbnailUrl source { displayName } } } }";
        let d: D = self
            .gql(query, json!({ "source": source, "type": kind, "page": page.max(1), "query": q }), Self::SLOW)
            .await?;
        Ok((d.fetch_source_manga.mangas.iter().map(card).collect(), d.fetch_source_manga.has_next_page))
    }

    /// Ficha y capítulos. Usa lo que Suwayomi ya tiene si es reciente; si no (o `refresh`), le
    /// pide a la fuente.
    pub async fn manga(&self, id: i64, refresh: bool) -> Result<MangaDetail, ApiError> {
        #[derive(Deserialize)]
        struct Cached {
            manga: CachedManga,
        }
        #[derive(Deserialize)]
        struct CachedManga {
            #[serde(flatten)]
            m: SMangaFull,
            chapters: Nodes<SChapter>,
        }
        if !refresh {
            let q = format!("query($id: Int!) {{ manga(id: $id) {{ {MANGA_FIELDS} chapters {{ nodes {{ {CHAPTER_FIELDS} }} }} }} }}");
            let d: Cached = self.gql(&q, json!({ "id": id }), Self::QUICK).await.map_err(not_found_if_missing)?;
            let fetched: i64 = d.manga.m.chapters_last_fetched_at.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);
            let fresh = crate::db::now_unix() - fetched < CHAPTERS_FRESH_S;
            if d.manga.m.initialized && fetched > 0 && fresh {
                return Ok(detail(d.manga.m, d.manga.chapters.nodes));
            }
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Fetched {
            fetch_manga: FM,
            fetch_chapters: FC,
        }
        #[derive(Deserialize)]
        struct FM {
            manga: SMangaFull,
        }
        #[derive(Deserialize)]
        struct FC {
            chapters: Vec<SChapter>,
        }
        let q = format!(
            "mutation($id: Int!) {{ fetchManga(input: {{ id: $id }}) {{ manga {{ {MANGA_FIELDS} }} }}
               fetchChapters(input: {{ mangaId: $id }}) {{ chapters {{ {CHAPTER_FIELDS} }} }} }}"
        );
        let d: Fetched = self.gql(&q, json!({ "id": id }), Self::SLOW).await.map_err(not_found_if_missing)?;
        Ok(detail(d.fetch_manga.manga, d.fetch_chapters.chapters))
    }

    /// Solo los capítulos, pidiéndolos a la fuente (búsqueda de novedades).
    pub async fn fetch_chapters(&self, id: i64) -> Result<Vec<Chapter>, ApiError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct D {
            fetch_chapters: FC,
        }
        #[derive(Deserialize)]
        struct FC {
            chapters: Vec<SChapter>,
        }
        let q = format!("mutation($id: Int!) {{ fetchChapters(input: {{ mangaId: $id }}) {{ chapters {{ {CHAPTER_FIELDS} }} }} }}");
        let d: D = self.gql(&q, json!({ "id": id }), Self::SLOW).await?;
        Ok(sort_chapters(d.fetch_chapters.chapters.into_iter().map(chapter).collect()))
    }

    /// Tarjeta de un manga desde lo que Suwayomi ya tiene (sin ir a la fuente).
    pub async fn card(&self, id: i64) -> Result<WorkCard, ApiError> {
        #[derive(Deserialize)]
        struct D {
            manga: SManga,
        }
        let d: D = self
            .gql("query($id: Int!) { manga(id: $id) { id title thumbnailUrl source { displayName } } }", json!({ "id": id }), Self::QUICK)
            .await
            .map_err(not_found_if_missing)?;
        Ok(card(&d.manga))
    }

    /// URLs (`/api/img/s/...`) de las páginas de un capítulo, en orden.
    pub async fn pages(&self, chapter_id: i64) -> Result<Vec<String>, ApiError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct D {
            fetch_chapter_pages: P,
        }
        #[derive(Deserialize)]
        struct P {
            pages: Vec<String>,
        }
        let d: D = self
            .gql(
                "mutation($id: Int!) { fetchChapterPages(input: { chapterId: $id }) { pages } }",
                json!({ "id": chapter_id }),
                Self::SLOW,
            )
            .await
            .map_err(not_found_if_missing)?;
        Ok(d.fetch_chapter_pages.pages.iter().filter_map(|p| img_url(p)).collect())
    }

    /// Todas las extensiones que Suwayomi conoce (las del repo y las instaladas).
    pub async fn extensions(&self) -> Result<Vec<Extension>, ApiError> {
        #[derive(Deserialize)]
        struct D {
            extensions: Nodes<SExtension>,
        }
        let d: D = self.gql(&format!("{{ extensions {{ nodes {{ {EXT_FIELDS} }} }} }}"), json!({}), Self::QUICK).await?;
        Ok(d.extensions.nodes.into_iter().map(extension).collect())
    }

    /// Vuelve a leer el índice de los repos.
    pub async fn refresh_extensions(&self) -> Result<Vec<Extension>, ApiError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct D {
            fetch_extensions: E,
        }
        #[derive(Deserialize)]
        struct E {
            extensions: Vec<SExtension>,
        }
        let d: D = self
            .gql(&format!("mutation {{ fetchExtensions(input: {{}}) {{ extensions {{ {EXT_FIELDS} }} }} }}"), json!({}), Self::SLOW)
            .await?;
        Ok(d.fetch_extensions.extensions.into_iter().map(extension).collect())
    }

    /// `action`: install, uninstall o update.
    pub async fn update_extension(&self, pkg: &str, action: &str) -> Result<Extension, ApiError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct D {
            update_extension: U,
        }
        #[derive(Deserialize)]
        struct U {
            extension: Option<SExtension>,
        }
        let patch = match action {
            "install" => json!({ "install": true }),
            "uninstall" => json!({ "uninstall": true }),
            "update" => json!({ "update": true }),
            _ => return Err(ApiError::bad_request("action must be install, uninstall or update")),
        };
        let q = format!(
            "mutation($id: String!, $patch: UpdateExtensionPatchInput!) {{
               updateExtension(input: {{ id: $id, patch: $patch }}) {{ extension {{ {EXT_FIELDS} }} }} }}"
        );
        let d: D = self.gql(&q, json!({ "id": pkg, "patch": patch }), Duration::from_secs(120)).await?;
        d.update_extension.extension.map(extension).ok_or_else(|| ApiError::not_found("Unknown extension"))
    }

    pub async fn repos(&self) -> Result<Vec<String>, ApiError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct D {
            extension_stores: Nodes<R>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct R {
            index_url: String,
        }
        let d: D = self.gql("{ extensionStores { nodes { indexUrl } } }", json!({}), Self::QUICK).await?;
        Ok(d.extension_stores.nodes.into_iter().map(|r| r.index_url).collect())
    }

    pub async fn add_repo(&self, url: &str) -> Result<(), ApiError> {
        let _: Value = self
            .gql(
                "mutation($u: String!) { addExtensionStore(input: { indexUrl: $u }) { clientMutationId } }",
                json!({ "u": url }),
                Self::SLOW,
            )
            .await?;
        Ok(())
    }

    pub async fn remove_repo(&self, url: &str) -> Result<(), ApiError> {
        let _: Value = self
            .gql(
                "mutation($u: String!) { removeExtensionStore(input: { indexUrl: $u }) { clientMutationId } }",
                json!({ "u": url }),
                Self::QUICK,
            )
            .await?;
        Ok(())
    }

    /// Al arrancar: si Suwayomi no tiene ningún repo de extensiones, agrega `url`. Reintenta
    /// mientras Suwayomi arranca (la JVM tarda).
    pub async fn ensure_repo(&self, url: String) {
        if url.is_empty() {
            return;
        }
        for attempt in 0..30 {
            match self.repos().await {
                Ok(r) if !r.is_empty() => return,
                Ok(_) => match self.add_repo(&url).await {
                    Ok(()) => {
                        tracing::info!("repo de extensiones agregado a Suwayomi: {url}");
                        let _ = self.refresh_extensions().await;
                        return;
                    }
                    Err(e) => tracing::warn!("no se pudo agregar el repo de extensiones: {}", e.message),
                },
                Err(e) if attempt == 0 => tracing::info!("esperando a Suwayomi ({})", e.message),
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    /// GET a `/api/v1/{path}` de Suwayomi (imágenes). El que llama copia el cuerpo en streaming.
    pub async fn get_raw(&self, path: &str) -> Result<reqwest::Response, ApiError> {
        self.http
            .get(format!("{}/api/v1/{path}", self.base))
            .timeout(Self::SLOW)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ApiError::source("The source took too long to answer")
                } else {
                    ApiError::unavailable("The manga server isn't responding")
                }
            })
    }
}

/// Suwayomi responde a un id que no existe con errores como "Collection is empty",
/// "NoSuchElementException" o "wrongly returned a null value" (consulta `manga(id)`): eso es un 404.
fn not_found_if_missing(e: ApiError) -> ApiError {
    let m = e.message.to_ascii_lowercase();
    if e.code == "source_error" && (m.contains("collection is empty") || m.contains("nosuchelement") || m.contains("not found") || m.contains("returned a null value")) {
        ApiError::not_found("Not found")
    } else {
        e
    }
}

fn detail(m: SMangaFull, chapters: Vec<SChapter>) -> MangaDetail {
    MangaDetail {
        card: WorkCard {
            id: work_id(m.id),
            kind: "manga".into(),
            title: m.title,
            cover: m.thumbnail_url.as_deref().and_then(img_url),
            source: m.source.map(|s| s.display_name),
            format: None,
        },
        source_id: m.source_id,
        author: m.author.filter(|s| !s.trim().is_empty()),
        artist: m.artist.filter(|s| !s.trim().is_empty()),
        description: m.description.filter(|s| !s.trim().is_empty()),
        genres: m.genre,
        status: status(&m.status),
        url: m.real_url,
        chapters: sort_chapters(chapters.into_iter().map(chapter).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_langs() {
        assert_eq!(split_lang("[es-la] Por amor al arte"), (Some("es-419".into()), Some("Por amor al arte".into())));
        assert_eq!(split_lang("[EN] RESET SCANS"), (Some("en".into()), Some("RESET SCANS".into())));
        assert_eq!(split_lang("[es]"), (Some("es".into()), None));
        assert_eq!(split_lang("Scan X"), (None, Some("Scan X".into())));
        assert_eq!(split_lang("[Oficial] Algo largo de verdad"), (None, Some("[Oficial] Algo largo de verdad".into())));
        assert_eq!(split_lang(""), (None, None));
    }

    #[test]
    fn maps_paths_and_ids() {
        assert_eq!(img_url("/api/v1/manga/1/thumbnail").as_deref(), Some("/api/img/s/manga/1/thumbnail"));
        assert_eq!(img_url("https://x/y"), None);
        assert_eq!(manga_id("s12"), Some(12));
        assert_eq!(manga_id("b12"), None);
        assert_eq!(manga_id("s0"), None);
        assert_eq!(status("PUBLISHING_FINISHED"), "completed");
        assert_eq!(first_line("Exception while fetching data (/fetchManga) : Collection is empty.\r\n\tat x"), "Collection is empty.");
    }
}
