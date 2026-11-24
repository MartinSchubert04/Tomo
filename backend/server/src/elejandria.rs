//! Elejandría (elejandria.com): libros gratis y legales en español (dominio público y licencias
//! libres). No tiene API ni feed, y su `robots.txt` no deja usar el buscador (`/buscar`), así que:
//! - el índice sale del `sitemap.xml` (todas las fichas `/libro/<titulo>/<autor>/<id>`) y de las
//!   listas por letra (`/libros/a`...: título con acentos, autor y portada), se baja una vez por
//!   día y se busca en memoria;
//! - sin búsqueda, la lista es la de "más descargados" (`/libros-mas-descargados`);
//! - el detalle (título real con acentos, autor, sinopsis, portada) y el enlace de descarga salen de
//!   la ficha del libro, que se pide recién cuando hace falta y queda en caché.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use tokio::sync::Semaphore;

use crate::{
    anilist::normalize,
    bookmeta::plain_text,
    catalogs::{deslug, Catalog, Download, Entry, Hint, Page, Timed},
    error::{ApiError, ApiResult},
    state::AppState,
};

const INDEX_TTL: Duration = Duration::from_secs(24 * 3600);
const TOP_TTL: Duration = Duration::from_secs(6 * 3600);
const PAGE_SIZE: usize = 40;
const DETAIL_CACHE: usize = 3000;

#[derive(Clone, Debug)]
struct Item {
    id: u64,
    title_slug: String,
    author_slug: String,
    norm: String,
    /// De las listas por letra (si no, del slug).
    title: Option<String>,
    author: Option<String>,
    cover: Option<String>,
}

/// Un libro de una lista del sitio (por letra o "más descargados").
#[derive(Debug, PartialEq)]
struct Listed {
    id: u64,
    title: Option<String>,
    author: Option<String>,
    cover: Option<String>,
}

fn unescape(t: &str) -> String {
    plain_text(t, 300)
}

/// Las tarjetas de una lista: `<a href=".../libro/t/a/ID"><img ... data-src="portada">` y después
/// `title="Libro TÍTULO"` y `title="Libros de AUTOR"`.
fn parse_listing(html: &str) -> Vec<Listed> {
    let mut out: Vec<Listed> = Vec::new();
    for block in html.split("<div class=\"col-lg-2").skip(1) {
        let Some(href) = between(block, "href=\"", "\"") else { continue };
        let Some((_, _, id)) = parse_book_path(href) else { continue };
        if out.iter().any(|l| l.id == id) {
            continue;
        }
        out.push(Listed {
            id,
            title: between(block, "title=\"Libro ", "\"").map(unescape).filter(|t| !t.is_empty()),
            author: between(block, "title=\"Libros de ", "\"").map(unescape).filter(|t| !t.is_empty()),
            cover: between(block, "data-src=\"", "\"").or_else(|| between(block, " src=\"", "\"")).filter(|u| u.contains("/covers/")).map(str::to_string),
        });
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct Detail {
    title: Option<String>,
    author: Option<String>,
    summary: Option<String>,
    cover: Option<String>,
    epub: Option<String>,
    pdf: Option<String>,
    page: String,
}

/// Un valor compartido que vence (`None` = todavía no se pidió).
type Slot<T> = Arc<Mutex<Option<Timed<Arc<T>>>>>;

/// Caché compartida: el índice del sitemap, los "más descargados" y las fichas ya leídas.
#[derive(Clone)]
pub struct Index {
    items: Slot<Vec<Item>>,
    top: Slot<Vec<u64>>,
    details: Arc<Mutex<HashMap<u64, Arc<Detail>>>>,
    /// Pocas fichas a la vez: es un sitio chico.
    gate: Arc<Semaphore>,
}

impl Default for Index {
    fn default() -> Self {
        Self { items: Default::default(), top: Default::default(), details: Default::default(), gate: Arc::new(Semaphore::new(3)) }
    }
}

fn base(cat: &Catalog) -> &str {
    cat.base.trim_end_matches('/')
}

async fn get_text(s: &AppState, url: &str) -> ApiResult<String> {
    s.catalogs
        .http
        .get(url)
        .send()
        .await
        .map_err(|e| ApiError::source(format!("Elejandría didn't answer: {e}")))?
        .error_for_status()
        .map_err(|e| ApiError::source(format!("Elejandría: {e}")))?
        .text()
        .await
        .map_err(|e| ApiError::source(format!("Elejandría: {e}")))
}

/// `/libro/<titulo>/<autor>/<id>` (sin nada después) → (titulo, autor, id).
fn parse_book_path(path: &str) -> Option<(String, String, u64)> {
    let rest = path.split("/libro/").nth(1)?;
    let mut it = rest.trim_end_matches('/').split('/');
    let (t, a, id) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() || t.is_empty() || a.is_empty() || matches!(t, "descargar" | "link_descarga_libro" | "enviar-por-email") {
        return None;
    }
    Some((t.to_string(), a.to_string(), id.parse().ok()?))
}

fn parse_sitemap(xml: &str) -> Vec<Item> {
    let mut out = Vec::new();
    for part in xml.split("<loc>").skip(1) {
        let loc = part.split("</loc>").next().unwrap_or("");
        if let Some((t, a, id)) = parse_book_path(loc) {
            out.push(Item { id, norm: normalize(&format!("{} {}", t.replace('-', " "), a.replace('-', " "))), title_slug: t, author_slug: a, title: None, author: None, cover: None });
        }
    }
    out
}

async fn index(s: &AppState, cat: &Catalog) -> ApiResult<Arc<Vec<Item>>> {
    if let Some((at, v)) = s.catalogs.eleja.items.lock().unwrap().as_ref() {
        if at.elapsed() < INDEX_TTL {
            return Ok(v.clone());
        }
    }
    let xml = get_text(s, &format!("{}/sitemap.xml", base(cat))).await?;
    let mut items = parse_sitemap(&xml);
    if items.is_empty() {
        return Err(ApiError::source("Elejandría's book list came empty"));
    }
    // Títulos con acentos, autores y portadas de las listas por letra (de a 3 páginas por vez).
    let letters = "abcdefghijklmnopqrstuvwxyz";
    let pages: Vec<Vec<Listed>> = futures_util::stream::iter(letters.chars())
        .map(|l| {
            let url = format!("{}/libros/{l}", base(cat));
            async move { get_text(s, &url).await.map(|h| parse_listing(&h)).unwrap_or_default() }
        })
        .buffer_unordered(3)
        .collect()
        .await;
    let listed: HashMap<u64, Listed> = pages.into_iter().flatten().map(|l| (l.id, l)).collect();
    for it in items.iter_mut() {
        if let Some(l) = listed.get(&it.id) {
            it.title = l.title.clone();
            it.author = l.author.clone();
            it.cover = l.cover.clone();
            it.norm = normalize(&format!("{} {} {}", l.title.as_deref().unwrap_or(""), l.author.as_deref().unwrap_or(""), it.norm));
        }
    }
    let v = Arc::new(items);
    *s.catalogs.eleja.items.lock().unwrap() = Some((Instant::now(), v.clone()));
    Ok(v)
}

async fn top(s: &AppState, cat: &Catalog) -> ApiResult<Arc<Vec<u64>>> {
    if let Some((at, v)) = s.catalogs.eleja.top.lock().unwrap().as_ref() {
        if at.elapsed() < TOP_TTL {
            return Ok(v.clone());
        }
    }
    let html = get_text(s, &format!("{}/libros-mas-descargados", base(cat))).await?;
    let ids: Vec<u64> = parse_listing(&html).into_iter().map(|l| l.id).collect();
    let v = Arc::new(ids);
    *s.catalogs.eleja.top.lock().unwrap() = Some((Instant::now(), v.clone()));
    Ok(v)
}

fn to_entry(s: &AppState, cat: &Catalog, it: &Item, d: Option<&Detail>) -> Entry {
    let id = it.id.to_string();
    Entry {
        title: d.and_then(|d| d.title.clone()).or_else(|| it.title.clone()).unwrap_or_else(|| deslug(&it.title_slug, false)),
        author: d.and_then(|d| d.author.clone()).or_else(|| it.author.clone()).or_else(|| Some(deslug(&it.author_slug, true))),
        // Con la portada de la lista, directo; si no, la busca la ficha cuando se pide.
        cover: Some(match &it.cover {
            Some(u) => s.catalogs.cover_url(u),
            None => format!("/api/catalogs/{}/cover/{id}", cat.id),
        }),
        language: Some("es".into()),
        year: None,
        summary: d.and_then(|d| d.summary.clone()),
        formats: vec!["epub".into(), "pdf".into()],
        imported: None,
        url: Some(format!("{}/libro/{}/{}/{id}", base(cat), it.title_slug, it.author_slug)),
        key: key_for(cat, it.id),
        id,
    }
}

/// Bajar el índice ya (al arrancar), así la primera búsqueda no espera.
pub async fn warm(s: &AppState, cat: &Catalog) -> ApiResult<()> {
    index(s, cat).await.map(|_| ())
}

pub fn key_for(cat: &Catalog, id: u64) -> String {
    format!("{}/libro/{id}", base(cat))
}

/// Qué tan bien coincide (más bajo = mejor) o `None`.
fn score(it: &Item, words: &[String], q: &str) -> Option<u8> {
    if !words.iter().all(|w| it.norm.contains(w.as_str())) {
        return None;
    }
    let title = it.title.as_deref().map(normalize).unwrap_or_else(|| it.title_slug.replace('-', " "));
    Some(if title == q {
        0
    } else if title.starts_with(q) {
        1
    } else if title.contains(q) {
        2
    } else {
        3
    })
}

pub async fn browse(s: &AppState, cat: &Catalog, q: Option<&str>, path: Option<&str>) -> ApiResult<Page> {
    let page: usize = path.and_then(|p| p.strip_prefix('p')).and_then(|n| n.parse().ok()).filter(|n| *n >= 1).unwrap_or(1);
    let items = index(s, cat).await?;
    let chosen: Vec<&Item> = match q {
        Some(q) => {
            let nq = normalize(q);
            let words: Vec<String> = nq.split_whitespace().map(str::to_string).collect();
            if words.is_empty() {
                return Ok(Page::default());
            }
            let mut v: Vec<(u8, &Item)> = items.iter().filter_map(|it| score(it, &words, &nq).map(|r| (r, it))).collect();
            v.sort_by_key(|(r, _)| *r);
            v.into_iter().map(|(_, it)| it).collect()
        }
        None => {
            let ids = top(s, cat).await.unwrap_or_default();
            let mut v: Vec<&Item> = ids.iter().filter_map(|id| items.iter().find(|it| it.id == *id)).collect();
            // Después de los más bajados, el resto del catálogo (los más nuevos primero).
            let mut rest: Vec<&Item> = items.iter().filter(|it| !ids.contains(&it.id)).collect();
            rest.sort_by_key(|a| std::cmp::Reverse(a.id));
            v.extend(rest);
            v
        }
    };
    let total = chosen.len();
    let details = s.catalogs.eleja.details.lock().unwrap().clone();
    let entries: Vec<Entry> = chosen
        .into_iter()
        .skip((page - 1) * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|it| to_entry(s, cat, it, details.get(&it.id).map(|d| d.as_ref())))
        .collect();
    Ok(Page { title: None, entries, navigation: Vec::new(), next: (page * PAGE_SIZE < total).then(|| format!("p{}", page + 1)) })
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)? + i;
    Some(&s[i..j])
}

fn attr_content(html: &str, prop: &str) -> Option<String> {
    let i = html.find(prop)?;
    let rest = &html[i..];
    let v = between(rest, "content=\"", "\"")?;
    Some(v.trim().to_string())
}

/// Lo que importa de la ficha de un libro.
fn parse_detail(html: &str, base: &str, id: u64) -> Detail {
    let title = between(html, "<h1", "</h1>")
        .and_then(|h| h.split_once('>').map(|(_, t)| t))
        .map(|t| t.split("<span").next().unwrap_or(t))
        .map(|t| plain_text(t, 300))
        .map(|t| t.strip_prefix("Libro ").map(str::to_string).unwrap_or(t))
        .filter(|t| !t.is_empty());
    let author = html
        .find("<h2")
        .and_then(|i| between(&html[i..], ">de ", "</a>").or_else(|| between(&html[i..], ">", "</h2>")))
        .map(|t| plain_text(t, 200))
        .map(|t| t.strip_prefix("de ").map(str::to_string).unwrap_or(t))
        .filter(|t| !t.is_empty() && t.len() < 200);
    let summary = between(html, "class=\"book-abstract", "</div>")
        .and_then(|t| t.split_once('>').map(|(_, t)| t))
        .map(|t| plain_text(t, 2000))
        .filter(|t| !t.is_empty());
    let cover = attr_content(html, "property=\"og:image\"").filter(|u| u.starts_with(base));
    // Los botones de descarga: `/libro/descargar/<t>/<a>/<id>/<formato>` con el texto "en ePub".
    let mut epub = None;
    let mut pdf = None;
    for part in html.split("href=\"").skip(1) {
        let href = part.split('"').next().unwrap_or("");
        let Some(rest) = href.split("/libro/descargar/").nth(1) else { continue };
        let fmt_id = rest.rsplit('/').next().and_then(|f| f.parse::<u64>().ok());
        let label = part.split("</a>").next().unwrap_or("");
        if let Some(f) = fmt_id {
            let link = format!("{base}/libro/link_descarga_libro/{id}/{f}");
            if label.contains("ePub") && epub.is_none() {
                epub = Some(link);
            } else if label.contains("PDF") && pdf.is_none() {
                pdf = Some(link);
            }
        }
    }
    Detail { title, author, summary, cover, epub, pdf, page: String::new() }
}

async fn detail(s: &AppState, cat: &Catalog, id: u64) -> ApiResult<Arc<Detail>> {
    if let Some(d) = s.catalogs.eleja.details.lock().unwrap().get(&id) {
        return Ok(d.clone());
    }
    let items = index(s, cat).await?;
    let it = items.iter().find(|it| it.id == id).ok_or_else(|| ApiError::not_found("Book not found"))?;
    let page = format!("{}/libro/{}/{}/{id}", base(cat), it.title_slug, it.author_slug);
    let _permit = s.catalogs.eleja.gate.acquire().await.map_err(ApiError::internal)?;
    let html = get_text(s, &page).await?;
    let mut d = parse_detail(&html, base(cat), id);
    d.page = page;
    let d = Arc::new(d);
    let mut cache = s.catalogs.eleja.details.lock().unwrap();
    if cache.len() >= DETAIL_CACHE {
        cache.clear();
    }
    cache.insert(id, d.clone());
    Ok(d)
}

fn parse_id(entry: &str) -> ApiResult<u64> {
    entry.parse().map_err(|_| ApiError::not_found("Book not found"))
}

pub async fn entry(s: &AppState, cat: &Catalog, entry: &str) -> ApiResult<Entry> {
    let id = parse_id(entry)?;
    let d = detail(s, cat, id).await?;
    let items = index(s, cat).await?;
    let it = items.iter().find(|it| it.id == id).ok_or_else(|| ApiError::not_found("Book not found"))?;
    let mut e = to_entry(s, cat, it, Some(&d));
    e.formats = [d.epub.as_ref().map(|_| "epub"), d.pdf.as_ref().map(|_| "pdf")].into_iter().flatten().map(str::to_string).collect();
    Ok(e)
}

pub async fn cover_of(s: &AppState, cat: &Catalog, entry: &str) -> ApiResult<Option<String>> {
    let id = parse_id(entry)?;
    Ok(detail(s, cat, id).await?.cover.clone())
}

pub async fn download(s: &AppState, cat: &Catalog, entry: &str) -> ApiResult<Download> {
    let id = parse_id(entry)?;
    let d = detail(s, cat, id).await?;
    let (url, format) = match (&d.epub, &d.pdf) {
        (Some(u), _) => (u.clone(), "epub"),
        (None, Some(u)) => (u.clone(), "pdf"),
        _ => return Err(ApiError::not_found("That book has no EPUB or PDF to download")),
    };
    Ok(Download {
        url,
        format,
        ext: format,
        key: key_for(cat, id),
        hint: Hint { title: d.title.clone(), author: d.author.clone(), language: Some("es".into()), summary: d.summary.clone(), year: None },
        cover: d.cover.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitemap_y_ficha() {
        let xml = "<urlset><url><loc>https://x.org</loc></url><url><loc>https://x.org/libro/orgullo-y-prejuicio/jane-austen/20</loc></url>\
                   <url><loc>https://x.org/autor/jane-austen/20</loc></url><url><loc>https://x.org/libro/descargar/a/b/1/2</loc></url></urlset>";
        let v = parse_sitemap(xml);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, 20);
        assert_eq!(v[0].norm, "orgullo y prejuicio jane austen");

        let html = r#"<meta property="og:image" content="https://x.org/covers/Orgullo-lg.png" />
            <h1 class="mb-2" >Libro Orgullo y prejuicio<span class="text-muted"> en PDF</span> </h1>
            <h2 class="mb-2"><a href="https://x.org/autor/jane-austen/20" class="s">de Jane Austen</a></h2>
            <div class="book-abstract mt-3"><p><em>Orgullo y prejuicio</em> es la novela.</p></div>
            <a href="https://x.org/libro/descargar/orgullo-y-prejuicio/jane-austen/20/2799"><img alt="Descargar en PDF"/> Descargar en PDF</a>
            <a href="https://x.org/libro/descargar/orgullo-y-prejuicio/jane-austen/20/1016"><img alt="Descargar en ePub"/> Descargar en ePub</a>"#;
        let d = parse_detail(html, "https://x.org", 20);
        assert_eq!(d.title.as_deref(), Some("Orgullo y prejuicio"));
        assert_eq!(d.author.as_deref(), Some("Jane Austen"));
        assert_eq!(d.summary.as_deref(), Some("Orgullo y prejuicio es la novela."));
        assert_eq!(d.cover.as_deref(), Some("https://x.org/covers/Orgullo-lg.png"));
        assert_eq!(d.epub.as_deref(), Some("https://x.org/libro/link_descarga_libro/20/1016"));
        assert_eq!(d.pdf.as_deref(), Some("https://x.org/libro/link_descarga_libro/20/2799"));
    }

    #[test]
    fn listas_por_letra() {
        let html = r#"<div class="col-lg-2 col-6 text-center" >
    <a href="https://x.org/libro/a-buen-juez-mejor-testigo/zorrilla-jose/213"><img loading="lazy" alt="Descargar A buen juez" data-src="https://x.org/covers/A_buen_juez-md.png" class="lazyload"/></a>
    <p class="mb-1"><a href="https://x.org/libro/a-buen-juez-mejor-testigo/zorrilla-jose/213" title="Libro A buen juez, mejor testigo">A BUEN JUEZ</a></p>
    <p><a href="https://x.org/autor/zorrilla-jose/112" title="Libros de José Zorrilla">JOSÉ ZORRILLA</a></p></div>
    <div class="col-lg-2 col-6 text-center" ><a href="https://x.org/autor/otro/1">x</a></div>"#;
        let v = parse_listing(html);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, 213);
        assert_eq!(v[0].title.as_deref(), Some("A buen juez, mejor testigo"));
        assert_eq!(v[0].cover.as_deref(), Some("https://x.org/covers/A_buen_juez-md.png"));
    }

    #[test]
    fn busca_por_palabras() {
        let it = Item {
            id: 1,
            title_slug: "la-isla-del-tesoro".into(),
            author_slug: "stevenson-robert-louis".into(),
            norm: "la isla del tesoro stevenson robert louis".into(),
            title: None,
            author: None,
            cover: None,
        };
        let w = |q: &str| normalize(q).split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(score(&it, &w("isla tesoro"), "isla tesoro"), Some(3));
        assert_eq!(score(&it, &w("La isla"), "la isla"), Some(1));
        assert_eq!(score(&it, &w("Stevenson"), "stevenson"), Some(3));
        assert_eq!(score(&it, &w("quijote"), "quijote"), None);
    }
}
