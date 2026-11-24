//! Lectura de libros subidos, sin la UI: qué formato es (por extensión y bytes mágicos), título,
//! autor y portada de un EPUB (container.xml → OPF), y las páginas de un CBZ (imágenes en orden
//! natural: "2.jpg" antes que "10.jpg"). Todo bloqueante: se llama desde `spawn_blocking`.
use std::{cmp::Ordering, fs::File, io::Read, path::Path};

use quick_xml::{events::Event, Reader};

/// Tope de una portada o de una página leída de un zip.
const MAX_IMAGE: u64 = 25 * 1024 * 1024;

/// Formato por extensión del nombre: (formato de la API, extensión en disco).
pub fn format_for(name: &str) -> Option<(&'static str, &'static str)> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    Some(match ext.as_str() {
        "epub" => ("epub", "epub"),
        "pdf" => ("pdf", "pdf"),
        "cbz" | "zip" => ("cbz", "cbz"),
        "mobi" | "prc" => ("mobi", "mobi"),
        "azw3" | "azw" => ("azw3", "azw3"),
        "fb2" => ("fb2", "fb2"),
        "txt" => ("txt", "txt"),
        _ => return None,
    })
}

/// ¿Los primeros bytes coinciden con el formato? (zip para epub/cbz, `%PDF` para pdf; el resto no
/// se verifica).
pub fn magic_ok(format: &str, head: &[u8]) -> bool {
    match format {
        "epub" | "cbz" => head.starts_with(b"PK\x03\x04"),
        "pdf" => head.windows(5).take(1024).any(|w| w == b"%PDF-"),
        _ => true,
    }
}

pub fn content_type(format: &str) -> &'static str {
    match format {
        "epub" => "application/epub+zip",
        "pdf" => "application/pdf",
        "cbz" => "application/vnd.comicbook+zip",
        "mobi" => "application/x-mobipocket-ebook",
        "azw3" => "application/vnd.amazon.ebook",
        "fb2" => "application/x-fictionbook+xml",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub fn image_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    Some(match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        _ => return None,
    })
}

pub fn ext_for_mime(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/avif" => "avif",
        _ => return None,
    })
}

/// Título "humano" a partir del nombre del archivo.
pub fn title_from_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
    let t: String = stem.replace(['_', '.'], " ").split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        "Untitled".into()
    } else {
        t.chars().take(300).collect()
    }
}

#[derive(Debug, Default)]
pub struct Meta {
    pub title: Option<String>,
    pub author: Option<String>,
    /// (bytes, extensión)
    pub cover: Option<(Vec<u8>, &'static str)>,
    pub pages: Option<i64>,
    /// Saga y número dentro de ella (`calibre:series` o `belongs-to-collection` de EPUB 3).
    pub series: Option<String>,
    pub series_index: Option<f64>,
    /// Idioma normalizado (`es`, `en`...; ver `normalize_lang`).
    pub language: Option<String>,
    pub description: Option<String>,
    pub year: Option<i64>,
}

/// `es-AR`, `spa`, `Spanish`, `español` → `es`; lo que no conoce lo deja en minúsculas (2-3 letras).
pub fn normalize_lang(raw: &str) -> Option<String> {
    let l = raw.trim().to_lowercase();
    let base = l.split(['-', '_']).next().unwrap_or("").trim();
    let v = match base {
        "" => return None,
        "spa" | "spanish" | "español" | "espanol" | "castellano" | "esp" => "es",
        "eng" | "english" | "inglés" | "ingles" => "en",
        "por" | "portuguese" | "português" | "portugues" => "pt",
        "fre" | "fra" | "french" | "francés" | "frances" => "fr",
        "ger" | "deu" | "german" | "alemán" | "aleman" => "de",
        "ita" | "italian" | "italiano" => "it",
        "lat" | "latin" | "latín" => "la",
        "cat" | "catalan" | "català" | "catalán" => "ca",
        x if x.len() <= 3 && x.chars().all(|c| c.is_ascii_alphabetic()) => x,
        _ => return None,
    };
    Some(v.to_string())
}

/// Texto plano a partir de HTML simple (sinopsis de OPF, OPDS u Open Library): saca etiquetas,
/// resuelve las entidades comunes, junta espacios y corta en `max` caracteres.
pub fn plain_text(html: &str, max: usize) -> String {
    let mut out = String::with_capacity(html.len().min(max * 2));
    let mut tag: Option<String> = None;
    for ch in html.chars() {
        match (&mut tag, ch) {
            (None, '<') => tag = Some(String::new()),
            (Some(name), '>') => {
                // Las etiquetas de bloque separan palabras; las de línea (<b>, <em>) no.
                let n = name.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
                if matches!(n.as_str(), "p" | "br" | "div" | "li" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "tr" | "td" | "blockquote" | "section") {
                    out.push(' ');
                }
                tag = None;
            }
            (Some(name), c) => {
                if name.len() < 16 {
                    name.push(c);
                }
            }
            (None, c) => out.push(c),
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let v: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if v.chars().count() > max {
        let mut t: String = v.chars().take(max).collect();
        t.push('…');
        t
    } else {
        v
    }
}

/// Año de una fecha cualquiera ("1605", "1605-01-01", "c. 1890"): los primeros 4 dígitos seguidos.
pub fn year_of(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(3))
        .find(|&i| b[i..i + 4].iter().all(u8::is_ascii_digit) && (i + 4 == b.len() || !b[i + 4].is_ascii_digit()) && (i == 0 || !b[i - 1].is_ascii_digit()))
        .and_then(|i| s[i..i + 4].parse().ok())
        .filter(|y| (1000..=2100).contains(y))
}

/// Lo que se pueda sacar del archivo según su formato. Nunca falla: sin datos → `Meta` vacío.
pub fn read_meta(path: &Path, format: &str) -> Meta {
    let r = match format {
        "epub" => epub_meta(path),
        "cbz" => cbz_meta(path),
        _ => Ok(Meta::default()),
    };
    r.unwrap_or_else(|e| {
        tracing::debug!("no se pudieron leer los datos de {}: {e:#}", path.display());
        Meta::default()
    })
}

fn read_entry(zip: &mut zip::ZipArchive<File>, name: &str, max: u64) -> anyhow::Result<Vec<u8>> {
    let f = zip.by_name(name)?;
    if f.size() > max {
        anyhow::bail!("{name} es demasiado grande");
    }
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.take(max).read_to_end(&mut buf)?;
    Ok(buf)
}

fn local_name(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

fn attr(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find(|a| local_name(a.key.as_ref()) == key).and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Une `href` (relativo al OPF) con la carpeta del OPF, resolviendo `..`.
fn join_href(base_dir: &str, href: &str) -> String {
    let href = percent_decode(href.split('#').next().unwrap_or(href));
    let mut parts: Vec<&str> = if href.starts_with('/') { Vec::new() } else { base_dir.split('/').filter(|p| !p.is_empty()).collect() };
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn epub_meta(path: &Path) -> anyhow::Result<Meta> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let container = read_entry(&mut zip, "META-INF/container.xml", 1024 * 1024)?;
    let mut opf_path = None;
    let mut rd = Reader::from_reader(container.as_slice());
    let mut buf = Vec::new();
    loop {
        match rd.read_event_into(&mut buf)? {
            Event::Start(e) | Event::Empty(e) if local_name(e.name().as_ref()) == "rootfile" => {
                opf_path = attr(&e, "full-path");
                break;
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    let opf_path = opf_path.ok_or_else(|| anyhow::anyhow!("container.xml sin rootfile"))?;
    let opf = read_entry(&mut zip, &opf_path, 4 * 1024 * 1024)?;
    let base_dir = opf_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");

    let mut meta = Meta::default();
    let mut cover_id: Option<String> = None;
    // EPUB 3: <meta property="belongs-to-collection" id="c1">Saga</meta> y
    // <meta refines="#c1" property="group-position">3</meta>.
    let mut collections: Vec<(String, String)> = Vec::new();
    let mut positions: Vec<(String, String)> = Vec::new();
    let mut meta_id = String::new();
    // id → (href, media-type, properties)
    let mut items: Vec<(String, String, String, String)> = Vec::new();
    let mut rd = Reader::from_reader(opf.as_slice());
    // El texto de un elemento puede venir partido en varios eventos (`El Aleph &amp; otros` →
    // texto, referencia, texto): se junta hasta el cierre.
    let mut current: Option<&'static str> = None;
    let mut acc = String::new();
    loop {
        match rd.read_event_into(&mut buf)? {
            Event::Start(e) => match local_name(e.name().as_ref()).as_str() {
                "title" if meta.title.is_none() => {
                    current = Some("title");
                    acc.clear();
                }
                "creator" if meta.author.is_none() => {
                    current = Some("creator");
                    acc.clear();
                }
                "language" if meta.language.is_none() => {
                    current = Some("language");
                    acc.clear();
                }
                "description" if meta.description.is_none() => {
                    current = Some("description");
                    acc.clear();
                }
                "date" if meta.year.is_none() => {
                    current = Some("date");
                    acc.clear();
                }
                "meta" => match attr(&e, "property").as_deref() {
                    Some("belongs-to-collection") => {
                        current = Some("collection");
                        meta_id = attr(&e, "id").unwrap_or_default();
                        acc.clear();
                    }
                    Some("group-position") => {
                        current = Some("position");
                        meta_id = attr(&e, "refines").unwrap_or_default().trim_start_matches('#').to_string();
                        acc.clear();
                    }
                    _ => {}
                },
                _ => {}
            },
            Event::Empty(e) => match local_name(e.name().as_ref()).as_str() {
                "meta" if attr(&e, "name").as_deref() == Some("cover") => cover_id = attr(&e, "content"),
                "meta" if attr(&e, "name").as_deref() == Some("calibre:series") => {
                    meta.series = attr(&e, "content").map(|v| v.trim().chars().take(300).collect::<String>()).filter(|v| !v.is_empty());
                }
                "meta" if attr(&e, "name").as_deref() == Some("calibre:series_index") => {
                    meta.series_index = attr(&e, "content").and_then(|v| v.trim().parse::<f64>().ok()).filter(|n| n.is_finite() && *n >= 0.0);
                }
                "item" => items.push((
                    attr(&e, "id").unwrap_or_default(),
                    attr(&e, "href").unwrap_or_default(),
                    attr(&e, "media-type").unwrap_or_default(),
                    attr(&e, "properties").unwrap_or_default(),
                )),
                _ => {}
            },
            Event::Text(t) if current.is_some() => acc.push_str(&t.decode().unwrap_or_default()),
            Event::CData(t) if current.is_some() => acc.push_str(&t.decode().unwrap_or_default()),
            Event::GeneralRef(r) if current.is_some() => {
                if let Ok(Some(ch)) = r.resolve_char_ref() {
                    acc.push(ch);
                } else if let Some(v) = r.decode().ok().and_then(|n| quick_xml::escape::resolve_predefined_entity(&n)) {
                    acc.push_str(v);
                }
            }
            Event::End(_) => {
                if let Some(which) = current.take() {
                    if which == "description" {
                        let v = plain_text(&acc, 4000);
                        if !v.is_empty() {
                            meta.description = Some(v);
                        }
                    } else {
                        let v: String = acc.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect();
                        if !v.is_empty() {
                            match which {
                                "title" => meta.title = Some(v),
                                "creator" => meta.author = Some(v),
                                "language" => meta.language = normalize_lang(&v),
                                "date" => meta.year = year_of(&v),
                                "collection" => collections.push((std::mem::take(&mut meta_id), v)),
                                "position" => positions.push((std::mem::take(&mut meta_id), v)),
                                _ => {}
                            }
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if meta.series.is_none() {
        if let Some((id, name)) = collections.into_iter().next() {
            meta.series = Some(name);
            meta.series_index = positions.iter().find(|(r, _)| *r == id).and_then(|(_, p)| p.trim().parse::<f64>().ok());
        }
    }
    let is_image = |m: &str| m.starts_with("image/");
    let cover = items
        .iter()
        .find(|i| i.3.split_whitespace().any(|p| p == "cover-image") && is_image(&i.2))
        .or_else(|| cover_id.as_ref().and_then(|id| items.iter().find(|i| &i.0 == id && is_image(&i.2))))
        .or_else(|| items.iter().find(|i| is_image(&i.2) && (i.0.to_lowercase().contains("cover") || i.1.to_lowercase().contains("cover"))));
    if let Some((_, href, mime, _)) = cover {
        if let Some(ext) = ext_for_mime(mime).or_else(|| image_type(href).and_then(ext_for_mime)) {
            if let Ok(bytes) = read_entry(&mut zip, &join_href(base_dir, href), MAX_IMAGE) {
                meta.cover = Some((bytes, ext));
            }
        }
    }
    Ok(meta)
}

/// Orden natural: los números se comparan como números ("2" < "10"), el resto sin mayúsculas.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Las imágenes de un CBZ, en orden de lectura.
pub fn cbz_pages(path: &Path) -> anyhow::Result<Vec<String>> {
    let zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut v: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|c| c.into_owned()))
        .filter(|n| !n.ends_with('/') && !n.starts_with("__MACOSX/") && !n.rsplit('/').next().unwrap_or("").starts_with('.'))
        .filter(|n| image_type(n).is_some())
        .collect();
    v.sort_by(|a, b| natural_cmp(a, b));
    Ok(v)
}

/// Una imagen de adentro de un CBZ.
pub fn cbz_page(path: &Path, name: &str) -> anyhow::Result<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    read_entry(&mut zip, name, MAX_IMAGE)
}

fn cbz_meta(path: &Path) -> anyhow::Result<Meta> {
    let pages = cbz_pages(path)?;
    let mut meta = Meta { pages: Some(pages.len() as i64), ..Default::default() };
    if let Some(first) = pages.first() {
        if let Some(ext) = image_type(first).and_then(ext_for_mime) {
            meta.cover = Some((cbz_page(path, first)?, ext));
        }
    }
    Ok(meta)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;

    /// Un EPUB mínimo (para tests): container, OPF con título, autor y portada.
    pub fn tiny_epub() -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("mimetype", opt).unwrap();
            z.write_all(b"application/epub+zip").unwrap();
            z.start_file("META-INF/container.xml", opt).unwrap();
            z.write_all(br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
            z.start_file("OEBPS/content.opf", opt).unwrap();
            z.write_all(br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>El Aleph &amp; otros</dc:title><dc:creator>Jorge Luis Borges</dc:creator><meta name="cover" content="img1"/></metadata><manifest><item id="img1" href="images/my%20cover.png" media-type="image/png"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#).unwrap();
            z.start_file("OEBPS/images/my cover.png", opt).unwrap();
            z.write_all(b"\x89PNGfake").unwrap();
            z.start_file("OEBPS/c1.xhtml", opt).unwrap();
            z.write_all(b"<html><body><p>Hola</p></body></html>").unwrap();
            z.finish().unwrap();
        }
        out.into_inner()
    }

    /// Un CBZ mínimo con páginas desordenadas.
    pub fn tiny_cbz() -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let opt = zip::write::SimpleFileOptions::default();
            for (n, data) in [("10.jpg", &b"ten"[..]), ("2.jpg", b"two"), ("1.jpg", b"one"), ("notes.txt", b"x"), ("__MACOSX/._1.jpg", b"junk")] {
                z.start_file(n, opt).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["p10.jpg", "p2.jpg", "P1.jpg", "p02b.jpg"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["P1.jpg", "p2.jpg", "p02b.jpg", "p10.jpg"]);
    }

    #[test]
    fn reads_epub_and_cbz() {
        let dir = std::env::temp_dir().join(format!("library-meta-{}", crate::db::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let e = dir.join("a.epub");
        std::fs::write(&e, tiny_epub()).unwrap();
        let m = read_meta(&e, "epub");
        assert_eq!(m.title.as_deref(), Some("El Aleph & otros"));
        assert_eq!(m.author.as_deref(), Some("Jorge Luis Borges"));
        assert_eq!(m.cover.as_ref().map(|c| c.1), Some("png"));
        let c = dir.join("a.cbz");
        std::fs::write(&c, tiny_cbz()).unwrap();
        assert_eq!(cbz_pages(&c).unwrap(), ["1.jpg", "2.jpg", "10.jpg"]);
        let m = read_meta(&c, "cbz");
        assert_eq!(m.pages, Some(3));
        assert_eq!(m.cover.unwrap().0, b"one");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn names_and_formats() {
        assert_eq!(format_for("Libro.EPUB"), Some(("epub", "epub")));
        assert_eq!(format_for("x.zip"), Some(("cbz", "cbz")));
        assert_eq!(format_for("x.exe"), None);
        assert_eq!(title_from_name("One_Piece.v01.cbz"), "One Piece v01");
        assert!(magic_ok("pdf", b"%PDF-1.7"));
        assert!(!magic_ok("epub", b"%PDF-1.7"));
        assert_eq!(join_href("OEBPS", "../img/a%20b.jpg"), "img/a b.jpg");
    }
}
