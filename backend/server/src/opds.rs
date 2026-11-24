//! Cliente OPDS genérico (1.x en Atom y 2.0 en JSON): Project Gutenberg y cualquier catálogo que
//! agregue el admin (Calibre, Kavita, bibliotecas públicas...). Entiende navegación (subcatálogos),
//! entradas con enlaces de adquisición (EPUB primero, después PDF; nunca los de compra, préstamo o
//! suscripción), paginado (`rel="next"`) y búsqueda (plantilla OpenSearch, o `{searchTerms}` en el
//! enlace `search`). Las URLs que vuelven a la UI (páginas y descargas) van firmadas.
use std::time::{Duration, Instant};

use quick_xml::{events::Event, Reader};
use serde_json::Value;

use crate::{
    bookmeta::{normalize_lang, plain_text, year_of},
    catalogs::{Catalog, Download, Entry, Hint, Nav, Page},
    error::{ApiError, ApiResult},
    state::AppState,
};

/// Tope de un feed (los de Gutenberg rondan los 100 KB).
const MAX_FEED: usize = 8 * 1024 * 1024;
const OSD_TTL: Duration = Duration::from_secs(6 * 3600);

#[derive(Debug, Default, Clone)]
pub struct Link {
    rel: String,
    href: String,
    kind: String,
    title: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct RawEntry {
    title: String,
    author: Option<String>,
    language: Option<String>,
    summary: Option<String>,
    year: Option<i64>,
    links: Vec<Link>,
}

#[derive(Debug, Default)]
pub struct Feed {
    pub title: Option<String>,
    links: Vec<Link>,
    entries: Vec<RawEntry>,
    /// OPDS 2: navegación explícita.
    nav: Vec<Link>,
}

fn resolve(base: &url::Url, href: &str) -> Option<String> {
    let u = base.join(href.trim()).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| u.to_string())
}

pub async fn fetch_feed(s: &AppState, url: &str) -> ApiResult<Feed> {
    let base = url::Url::parse(url).map_err(|_| ApiError::bad_request("Invalid catalog URL"))?;
    let res = s
        .catalogs
        .http
        .get(url)
        .header(reqwest::header::ACCEPT, "application/atom+xml, application/opds+json, application/json;q=0.9, */*;q=0.5")
        .send()
        .await
        .map_err(|e| ApiError::source(format!("The catalog didn't answer: {e}")))?;
    if !res.status().is_success() {
        return Err(ApiError::source(format!("The catalog answered {}", res.status())));
    }
    if res.content_length().is_some_and(|n| n as usize > MAX_FEED) {
        return Err(ApiError::source("The catalog page is too big"));
    }
    let bytes = res.bytes().await.map_err(|e| ApiError::source(format!("The catalog: {e}")))?;
    if bytes.len() > MAX_FEED {
        return Err(ApiError::source("The catalog page is too big"));
    }
    let text = String::from_utf8_lossy(&bytes);
    let trimmed = text.trim_start();
    let mut feed = if trimmed.starts_with('{') {
        parse_json(trimmed)?
    } else if trimmed.starts_with('<') {
        parse_atom(trimmed)?
    } else {
        return Err(ApiError::source("The catalog didn't send an OPDS feed"));
    };
    if feed.entries.is_empty() && feed.links.is_empty() && feed.nav.is_empty() {
        return Err(ApiError::source("The catalog didn't send an OPDS feed"));
    }
    // Todas las URLs absolutas.
    let fix = |l: &mut Link| {
        if let Some(u) = resolve(&base, &l.href) {
            l.href = u;
        } else {
            l.href.clear();
        }
    };
    feed.links.iter_mut().for_each(fix);
    feed.nav.iter_mut().for_each(fix);
    for e in feed.entries.iter_mut() {
        e.links.iter_mut().for_each(fix);
        e.links.retain(|l| !l.href.is_empty());
    }
    Ok(feed)
}

fn local(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

fn attr(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| local(a.key.as_ref()) == key)
        .and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()))
}

fn link_of(e: &quick_xml::events::BytesStart) -> Link {
    Link { rel: attr(e, "rel").unwrap_or_default(), href: attr(e, "href").unwrap_or_default(), kind: attr(e, "type").unwrap_or_default(), title: attr(e, "title") }
}

pub fn parse_atom(xml: &str) -> ApiResult<Feed> {
    let mut rd = Reader::from_str(xml);
    let mut feed = Feed::default();
    let mut cur: Option<RawEntry> = None;
    // Qué texto se está juntando: (elemento, ¿dentro de <author>?)
    let mut field: Option<String> = None;
    let mut in_author = false;
    let mut acc = String::new();
    let mut depth_feed_title = true;
    loop {
        let ev = rd.read_event().map_err(|e| ApiError::source(format!("Broken catalog feed: {e}")))?;
        match ev {
            Event::Start(e) => {
                let name = local(e.name().as_ref());
                match name.as_str() {
                    "entry" => cur = Some(RawEntry::default()),
                    "author" => in_author = true,
                    "link" => {
                        let l = link_of(&e);
                        match cur.as_mut() {
                            Some(c) => c.links.push(l),
                            None => feed.links.push(l),
                        }
                    }
                    "title" | "name" | "language" | "summary" | "content" | "issued" | "published" | "date" => {
                        field = Some(name);
                        acc.clear();
                    }
                    _ => {}
                }
            }
            Event::Empty(e) if local(e.name().as_ref()) == "link" => {
                let l = link_of(&e);
                match cur.as_mut() {
                    Some(c) => c.links.push(l),
                    None => feed.links.push(l),
                }
            }
            Event::Text(t) if field.is_some() => acc.push_str(&t.decode().unwrap_or_default()),
            Event::CData(t) if field.is_some() => acc.push_str(&t.decode().unwrap_or_default()),
            Event::GeneralRef(r) if field.is_some() => {
                if let Ok(Some(ch)) = r.resolve_char_ref() {
                    acc.push(ch);
                } else if let Some(v) = r.decode().ok().and_then(|n| quick_xml::escape::resolve_predefined_entity(&n)) {
                    acc.push_str(v);
                }
            }
            Event::End(e) => {
                let name = local(e.name().as_ref());
                if field.as_deref() == Some(name.as_str()) {
                    field = None;
                    let text = acc.split_whitespace().collect::<Vec<_>>().join(" ");
                    match (cur.as_mut(), name.as_str()) {
                        (Some(c), "title") if c.title.is_empty() => c.title = text.chars().take(300).collect(),
                        (Some(c), "name") if in_author && c.author.is_none() && !text.is_empty() => c.author = Some(text.chars().take(200).collect()),
                        (Some(c), "language") if c.language.is_none() => c.language = normalize_lang(&text),
                        (Some(c), "summary" | "content") if c.summary.is_none() => {
                            let t = plain_text(&acc, 2000);
                            if !t.is_empty() {
                                c.summary = Some(t);
                            }
                        }
                        (Some(c), "issued" | "published" | "date") if c.year.is_none() => c.year = year_of(&text),
                        (None, "title") if depth_feed_title && !text.is_empty() => {
                            feed.title = Some(text.chars().take(120).collect());
                            depth_feed_title = false;
                        }
                        _ => {}
                    }
                }
                match name.as_str() {
                    "author" => in_author = false,
                    "entry" => {
                        if let Some(c) = cur.take() {
                            feed.entries.push(c);
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(feed)
}

fn json_links(v: &Value) -> Vec<Link> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| {
                    Some(Link {
                        rel: match l.get("rel") {
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
                            _ => String::new(),
                        },
                        href: l.get("href")?.as_str()?.to_string(),
                        kind: l.get("type").and_then(Value::as_str).unwrap_or("").to_string(),
                        title: l.get("title").and_then(Value::as_str).map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn json_str(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.iter().find_map(|x| json_str(Some(x))),
        Value::Object(o) => o.get("name").and_then(|n| json_str(Some(n))).or_else(|| o.values().find_map(|x| x.as_str().map(str::to_string))),
        _ => None,
    }
}

pub fn parse_json(text: &str) -> ApiResult<Feed> {
    let v: Value = serde_json::from_str(text).map_err(|e| ApiError::source(format!("Broken catalog feed: {e}")))?;
    let mut feed = Feed { title: json_str(v.pointer("/metadata/title")), links: json_links(&v["links"]), ..Default::default() };
    feed.nav = json_links(&v["navigation"]);
    let pubs = v["publications"].as_array().cloned().unwrap_or_default();
    let groups = v["groups"].as_array().cloned().unwrap_or_default();
    let mut all = pubs;
    for g in &groups {
        all.extend(g["publications"].as_array().cloned().unwrap_or_default());
        feed.nav.extend(json_links(&g["navigation"]));
    }
    for p in all {
        let m = &p["metadata"];
        let mut links = json_links(&p["links"]);
        for img in json_links(&p["images"]) {
            links.push(Link { rel: "http://opds-spec.org/image".into(), ..img });
        }
        feed.entries.push(RawEntry {
            title: json_str(m.get("title")).unwrap_or_default().chars().take(300).collect(),
            author: json_str(m.get("author")),
            language: json_str(m.get("language")).and_then(|l| normalize_lang(&l)),
            summary: json_str(m.get("description")).map(|d| plain_text(&d, 2000)).filter(|d| !d.is_empty()),
            year: json_str(m.get("published")).and_then(|y| year_of(&y)),
            links,
        });
    }
    Ok(feed)
}

fn is_acquisition(l: &Link) -> bool {
    l.rel.split_whitespace().any(|r| {
        r.starts_with("http://opds-spec.org/acquisition")
            && !r.ends_with("/buy")
            && !r.ends_with("/borrow")
            && !r.ends_with("/subscribe")
            && !r.ends_with("/sample")
            && !r.ends_with("/preview")
    })
}

/// (formato, extensión) por el tipo MIME o la extensión del enlace.
fn format_of(l: &Link) -> Option<(&'static str, &'static str)> {
    let k = l.kind.to_ascii_lowercase();
    let h = l.href.to_ascii_lowercase();
    let h = h.split(['?', '#']).next().unwrap_or("");
    if k.contains("epub") || h.ends_with(".epub") || h.ends_with(".epub3.images") || h.ends_with(".epub.images") || h.ends_with(".epub.noimages") {
        Some(("epub", "epub"))
    } else if k.contains("pdf") || h.ends_with(".pdf") {
        Some(("pdf", "pdf"))
    } else if k.contains("comicbook") || h.ends_with(".cbz") {
        Some(("cbz", "cbz"))
    } else if k.contains("fictionbook") || h.ends_with(".fb2") {
        Some(("fb2", "fb2"))
    } else if k.contains("mobipocket") || h.ends_with(".mobi") {
        Some(("mobi", "mobi"))
    } else {
        None
    }
}

fn best_acquisition(links: &[Link]) -> Option<(&Link, &'static str, &'static str)> {
    let acq: Vec<(&Link, &'static str, &'static str)> = links.iter().filter(|l| is_acquisition(l)).filter_map(|l| format_of(l).map(|(f, e)| (l, f, e))).collect();
    let rank = |f: &str| match f {
        "epub" => 0,
        "pdf" => 1,
        "cbz" => 2,
        "fb2" => 3,
        _ => 4,
    };
    // Gutenberg da varios EPUB: el de imágenes (epub3) primero.
    acq.into_iter().min_by_key(|(l, f, _)| (rank(f), if l.href.contains("noimages") { 1 } else { 0 }))
}

fn is_feed_link(l: &Link) -> bool {
    let k = l.kind.to_ascii_lowercase();
    k.contains("profile=opds-catalog") || k.contains("application/atom+xml") || k.contains("opds+json")
}

fn to_page(s: &AppState, cat: &Catalog, feed: Feed, es: bool) -> Page {
    let mut entries = Vec::new();
    let mut navigation = Vec::new();
    for e in feed.entries {
        if let Some((l, f, _)) = best_acquisition(&e.links) {
            if es && e.language.as_deref().is_some_and(|x| x != "es") {
                continue;
            }
            let formats: Vec<String> = {
                let mut v: Vec<String> = e.links.iter().filter(|l| is_acquisition(l)).filter_map(format_of).map(|(f, _)| f.to_string()).collect();
                v.dedup();
                if v.is_empty() {
                    v.push(f.to_string());
                }
                v
            };
            let cover = e
                .links
                .iter()
                .find(|l| l.rel.contains("opds-spec.org/image/thumbnail"))
                .or_else(|| e.links.iter().find(|l| l.rel.contains("opds-spec.org/image") || l.rel.contains("opds-spec.org/cover")))
                .map(|l| s.catalogs.cover_url(&l.href));
            let page = e.links.iter().find(|l| l.rel == "alternate" && l.kind.contains("html")).map(|l| l.href.clone());
            entries.push(Entry {
                id: s.catalogs.sign(&format!("{f}|{}", l.href)),
                title: if e.title.is_empty() { "?".into() } else { e.title },
                author: e.author,
                cover,
                language: e.language,
                year: e.year,
                summary: e.summary,
                formats,
                imported: None,
                url: page,
                key: l.href.clone(),
            });
        } else if let Some(l) = e.links.iter().find(|l| is_feed_link(l) && !l.rel.contains("acquisition")) {
            if !e.title.is_empty() {
                navigation.push(Nav { title: e.title, path: s.catalogs.sign(&l.href) });
            }
        }
    }
    for l in feed.nav {
        navigation.push(Nav { title: l.title.unwrap_or_else(|| "?".into()), path: s.catalogs.sign(&l.href) });
    }
    let next = feed.links.iter().find(|l| l.rel.split_whitespace().any(|r| r == "next")).map(|l| s.catalogs.sign(&l.href));
    crate::catalogs::dedupe(&mut entries);
    let _ = cat;
    Page { title: feed.title, entries, navigation, next }
}

/// La plantilla de búsqueda del catálogo (con `{searchTerms}`), si tiene.
async fn search_template(s: &AppState, cat: &Catalog) -> ApiResult<Option<String>> {
    if let Some((at, t)) = s.catalogs.osd.lock().unwrap().get(&cat.id) {
        if at.elapsed() < OSD_TTL {
            return Ok(t.clone());
        }
    }
    let root = fetch_feed(s, &cat.base).await?;
    let mut tpl = None;
    for l in root.links.iter().filter(|l| l.rel.split_whitespace().any(|r| r == "search")) {
        // OPDS 2 usa plantillas RFC 6570 (`{?query}`); OPDS 1, `{searchTerms}`.
        let href = l.href.replace("%7B", "{").replace("%7D", "}").replace("{?query}", "?query={searchTerms}");
        if href.contains("{searchTerms}") {
            tpl = Some(href);
            break;
        }
        if l.kind.contains("opensearchdescription") {
            if let Ok(t) = osd_template(s, &l.href).await {
                tpl = t;
                break;
            }
        }
    }
    s.catalogs.osd.lock().unwrap().insert(cat.id.clone(), (Instant::now(), tpl.clone()));
    Ok(tpl)
}

async fn osd_template(s: &AppState, url: &str) -> ApiResult<Option<String>> {
    let base = url::Url::parse(url).map_err(|_| ApiError::source("bad OpenSearch URL"))?;
    let xml = s.catalogs.http.get(url).send().await.map_err(|e| ApiError::source(e.to_string()))?.text().await.map_err(|e| ApiError::source(e.to_string()))?;
    let mut rd = Reader::from_str(&xml);
    let mut best: Option<(u8, String)> = None;
    loop {
        match rd.read_event().map_err(|e| ApiError::source(e.to_string()))? {
            Event::Start(e) | Event::Empty(e) if local(e.name().as_ref()) == "Url" => {
                let kind = attr(&e, "type").unwrap_or_default();
                if let Some(t) = attr(&e, "template").and_then(|t| resolve(&base, &t.replace("{searchTerms}", "SEARCHTERMS"))) {
                    let rank = if kind.contains("atom") || kind.contains("opds") { 0 } else { 1 };
                    if best.as_ref().is_none_or(|(r, _)| rank < *r) {
                        best = Some((rank, t.replace("%7B", "{").replace("%7D", "}").replace("SEARCHTERMS", "{searchTerms}")));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(best.map(|(_, t)| t))
}

fn fill_template(tpl: &str, q: &str) -> String {
    let enc: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
    // Los parámetros opcionales de OpenSearch (`{startPage?}`) se van.
    let mut out = tpl.replace("{searchTerms}", &enc);
    while let Some(i) = out.find('{') {
        match out[i..].find('}') {
            Some(j) if out[i..i + j].ends_with('?') || out[i + 1..i + j].chars().all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '?') => {
                out.replace_range(i..i + j + 1, "");
            }
            _ => break,
        }
    }
    out
}

pub async fn browse(s: &AppState, cat: &Catalog, q: Option<&str>, path: Option<&str>, es: bool) -> ApiResult<Page> {
    let url = match (path, q) {
        (Some(p), _) => s.catalogs.verify(p).ok_or_else(|| ApiError::bad_request("Invalid catalog page"))?,
        (None, Some(q)) => {
            let tpl = search_template(s, cat).await?.ok_or_else(|| ApiError::source("This catalog has no search"))?;
            fill_template(&tpl, q)
        }
        (None, None) => cat.base.clone(),
    };
    let feed = fetch_feed(s, &url).await?;
    let mut page = to_page(s, cat, feed, es);
    // Muchos catálogos (textos.info, Calibre) tienen en la raíz solo secciones: para que haya libros
    // a la vista de entrada, se abre la primera ("Más populares", "Novedades"...) y se dejan las
    // secciones arriba para elegir otra.
    if path.is_none() && q.is_none() && page.entries.is_empty() {
        if let Some(first) = page.navigation.first().and_then(|n| s.catalogs.verify(&n.path)) {
            if let Ok(f) = fetch_feed(s, &first).await {
                let inner = to_page(s, cat, f, es);
                page.entries = inner.entries;
                page.next = inner.next;
            }
        }
    }
    Ok(page)
}

pub fn download(s: &AppState, _cat: &Catalog, entry: &str) -> ApiResult<Download> {
    let v = s.catalogs.verify(entry).ok_or_else(|| ApiError::bad_request("Invalid book"))?;
    let (f, url) = v.split_once('|').ok_or_else(|| ApiError::bad_request("Invalid book"))?;
    let (format, ext): (&'static str, &'static str) = match f {
        "epub" => ("epub", "epub"),
        "pdf" => ("pdf", "pdf"),
        "cbz" => ("cbz", "cbz"),
        "fb2" => ("fb2", "fb2"),
        "mobi" => ("mobi", "mobi"),
        _ => return Err(ApiError::bad_request("Invalid book")),
    };
    Ok(Download { url: url.to_string(), format, ext, key: url.to_string(), hint: Hint::default(), cover: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATOM: &str = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dcterms="http://purl.org/dc/terms/">
  <title>Catálogo &amp; más</title>
  <link rel="search" type="application/atom+xml" href="/search?q={searchTerms}"/>
  <link rel="next" type="application/atom+xml;profile=opds-catalog" href="?page=2"/>
  <entry>
    <title>Novedades</title>
    <link rel="subsection" type="application/atom+xml;profile=opds-catalog;kind=acquisition" href="/new"/>
  </entry>
  <entry>
    <title>Don Quijote</title>
    <author><name>Miguel de Cervantes</name></author>
    <dcterms:language>es</dcterms:language>
    <dcterms:issued>1605</dcterms:issued>
    <summary type="html">&lt;p&gt;El hidalgo.&lt;/p&gt;</summary>
    <link rel="http://opds-spec.org/image/thumbnail" type="image/jpeg" href="/c/1.jpg"/>
    <link rel="http://opds-spec.org/acquisition/buy" type="application/epub+zip" href="/buy/1.epub"/>
    <link rel="http://opds-spec.org/acquisition" type="application/pdf" href="/b/1.pdf"/>
    <link rel="http://opds-spec.org/acquisition/open-access" type="application/epub+zip" href="/b/1.epub"/>
  </entry>
  <entry>
    <title>Hamlet</title>
    <dcterms:language>en</dcterms:language>
    <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/b/2.epub"/>
  </entry>
</feed>"#;

    #[test]
    fn atom() {
        let f = parse_atom(ATOM).unwrap();
        assert_eq!(f.title.as_deref(), Some("Catálogo & más"));
        assert_eq!(f.entries.len(), 3);
        let q = &f.entries[1];
        assert_eq!(q.author.as_deref(), Some("Miguel de Cervantes"));
        assert_eq!(q.language.as_deref(), Some("es"));
        assert_eq!(q.year, Some(1605));
        assert_eq!(q.summary.as_deref(), Some("El hidalgo."));
        let (l, f2, _) = best_acquisition(&q.links).unwrap();
        assert_eq!((l.href.as_str(), f2), ("/b/1.epub", "epub"));
        assert!(best_acquisition(&f.entries[0].links).is_none());
    }

    #[test]
    fn json() {
        let j = r#"{"metadata":{"title":"Biblio"},"links":[{"rel":"search","href":"/s{?query}","templated":true}],
          "navigation":[{"href":"/new","title":"Nuevos","type":"application/opds+json"}],
          "publications":[{"metadata":{"title":"Rayuela","author":{"name":"Julio Cortázar"},"language":"es","published":"1963-06-28"},
            "links":[{"rel":"http://opds-spec.org/acquisition/open-access","href":"/r.epub","type":"application/epub+zip"}],
            "images":[{"href":"/r.jpg","type":"image/jpeg"}]}]}"#;
        let f = parse_json(j).unwrap();
        assert_eq!(f.title.as_deref(), Some("Biblio"));
        assert_eq!(f.nav.len(), 1);
        let e = &f.entries[0];
        assert_eq!(e.author.as_deref(), Some("Julio Cortázar"));
        assert_eq!(e.year, Some(1963));
        assert!(best_acquisition(&e.links).is_some());
    }

    #[test]
    fn plantillas() {
        assert_eq!(fill_template("https://x.org/s?q={searchTerms}&p={startPage?}", "don quijote"), "https://x.org/s?q=don+quijote&p=");
        assert_eq!(fill_template("https://x.org/s/{searchTerms}", "a&b"), "https://x.org/s/a%26b");
    }
}
