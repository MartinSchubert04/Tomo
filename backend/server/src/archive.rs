//! Internet Archive como catálogo de libros: su búsqueda (`advancedsearch.php`) filtrada a textos
//! de dominio público (colección de Gutenberg o marcados por IA como fuera de copyright) que se
//! pueden bajar libremente (nunca los de préstamo: `access-restricted-item`) con EPUB o PDF, en
//! español por defecto. El archivo se elige con `metadata/<id>` (EPUB primero, después PDF con
//! texto) y se baja de `download/<id>/<archivo>`. Portadas: `services/img/<id>`.
use serde::Deserialize;
use serde_json::Value;

use crate::{
    bookmeta::{normalize_lang, plain_text, year_of},
    catalogs::{Catalog, Download, Entry, Hint, Page},
    error::{ApiError, ApiResult},
    state::AppState,
};

const ROWS: usize = 40;

/// Lo que la búsqueda de Lucene de IA interpreta como operador: afuera.
fn clean_terms(q: &str) -> String {
    q.chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|w| !matches!(w.to_ascii_uppercase().as_str(), "AND" | "OR" | "NOT" | "TO"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn query(q: Option<&str>, es: bool) -> String {
    let mut parts = vec![
        "mediatype:texts".to_string(),
        "-access-restricted-item:true".to_string(),
        "(format:EPUB OR format:\"Text PDF\")".to_string(),
        // Solo dominio público verificado: la colección de Project Gutenberg o lo que el propio
        // Internet Archive marcó como "fuera de copyright" (escaneos de bibliotecas). Lo subido por
        // usuarios sin esa marca (aunque diga "Creative Commons") queda afuera: ahí aparecen copias
        // de libros con copyright.
        "(collection:gutenberg OR possible-copyright-status:NOT_IN_COPYRIGHT)".to_string(),
    ];
    if es {
        parts.push("language:(spa OR Spanish OR español OR es)".into());
    }
    if let Some(t) = q.map(clean_terms).filter(|t| !t.is_empty()) {
        parts.insert(0, format!("(title:({t}) OR creator:({t}))"));
    }
    parts.join(" AND ")
}

fn first(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.iter().find_map(|x| x.as_str().map(str::to_string)),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn all_strings(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 200 && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

pub fn key_for(cat: &Catalog, id: &str) -> String {
    format!("{}/details/{id}", cat.base.trim_end_matches('/'))
}

fn to_entry(s: &AppState, cat: &Catalog, d: &Value) -> Option<Entry> {
    let id = d.get("identifier")?.as_str()?.to_string();
    if !valid_id(&id) {
        return None;
    }
    let formats = d.get("format").map(all_strings).unwrap_or_default();
    let mut f = Vec::new();
    if formats.iter().any(|x| x == "EPUB") {
        f.push("epub".to_string());
    }
    if formats.iter().any(|x| x.contains("PDF")) {
        f.push("pdf".to_string());
    }
    let base = cat.base.trim_end_matches('/');
    Some(Entry {
        title: d.get("title").and_then(first).map(|t| t.trim().trim_end_matches([';', '/', ':', ',']).trim().to_string()).unwrap_or_else(|| id.clone()),
        author: d.get("creator").and_then(first),
        cover: Some(s.catalogs.cover_url(&format!("{base}/services/img/{id}"))),
        language: d.get("language").and_then(first).and_then(|l| normalize_lang(&l)),
        year: d.get("year").and_then(first).or_else(|| d.get("date").and_then(first)).and_then(|y| year_of(&y)),
        summary: d.get("description").and_then(first).map(|t| plain_text(&t, 1200)).filter(|t| !t.is_empty()),
        formats: f,
        imported: None,
        url: Some(format!("{base}/details/{id}")),
        key: key_for(cat, &id),
        id,
    })
}

pub async fn browse(s: &AppState, cat: &Catalog, q: Option<&str>, path: Option<&str>, es: bool) -> ApiResult<Page> {
    let page: usize = path.and_then(|p| p.strip_prefix('p')).and_then(|n| n.parse().ok()).filter(|n| *n >= 1).unwrap_or(1);
    let base = cat.base.trim_end_matches('/');
    let mut u = url::Url::parse(&format!("{base}/advancedsearch.php")).map_err(|_| ApiError::internal("bad Internet Archive URL"))?;
    {
        let mut qp = u.query_pairs_mut();
        qp.append_pair("q", &query(q, es)).append_pair("rows", &ROWS.to_string()).append_pair("page", &page.to_string()).append_pair("output", "json");
        for f in ["identifier", "title", "creator", "language", "year", "date", "description", "format"] {
            qp.append_pair("fl[]", f);
        }
        // Sin búsqueda: los más bajados.
        if q.is_none() {
            qp.append_pair("sort[]", "downloads desc");
        }
    }
    let req = s.catalogs.http.get(u.as_str());
    let v: Value = req
        .send()
        .await
        .map_err(|e| ApiError::source(format!("Internet Archive didn't answer: {e}")))?
        .error_for_status()
        .map_err(|e| ApiError::source(format!("Internet Archive: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::source(format!("Internet Archive answered something unexpected: {e}")))?;
    let resp = v.get("response").ok_or_else(|| ApiError::source("Internet Archive answered something unexpected"))?;
    let found = resp.get("numFound").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut entries: Vec<Entry> = resp.get("docs").and_then(Value::as_array).map(|a| a.iter().filter_map(|d| to_entry(s, cat, d)).collect()).unwrap_or_default();
    crate::catalogs::dedupe(&mut entries);
    Ok(Page { title: None, entries, navigation: Vec::new(), next: (page * ROWS < found).then(|| format!("p{}", page + 1)) })
}

#[derive(Deserialize)]
struct Meta {
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    files: Vec<FileInfo>,
    #[serde(default)]
    is_dark: Option<bool>,
}

#[derive(Deserialize)]
struct FileInfo {
    name: String,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    private: Option<Value>,
}

async fn metadata(s: &AppState, cat: &Catalog, id: &str) -> ApiResult<Meta> {
    if !valid_id(id) {
        return Err(ApiError::not_found("Book not found"));
    }
    let base = cat.base.trim_end_matches('/');
    s.catalogs
        .http
        .get(format!("{base}/metadata/{id}"))
        .send()
        .await
        .map_err(|e| ApiError::source(format!("Internet Archive didn't answer: {e}")))?
        .error_for_status()
        .map_err(|e| ApiError::source(format!("Internet Archive: {e}")))?
        .json()
        .await
        .map_err(|_| ApiError::not_found("Book not found"))
}

fn restricted(m: &Meta) -> bool {
    m.is_dark == Some(true)
        || m.metadata.get("access-restricted-item").and_then(first).is_some_and(|v| v == "true")
        || m.metadata.get("collection").map(all_strings).unwrap_or_default().iter().any(|c| c == "inlibrary" || c == "printdisabled" || c == "lendinglibrary")
}

pub async fn entry(s: &AppState, cat: &Catalog, id: &str) -> ApiResult<Entry> {
    let m = metadata(s, cat, id).await?;
    let mut d = m.metadata.clone();
    if let Value::Object(o) = &mut d {
        o.insert("identifier".into(), Value::String(id.to_string()));
        let formats: Vec<Value> = m.files.iter().filter_map(|f| f.format.clone()).map(Value::String).collect();
        o.insert("format".into(), Value::Array(formats));
    }
    let mut e = to_entry(s, cat, &d).ok_or_else(|| ApiError::not_found("Book not found"))?;
    if restricted(&m) {
        e.formats.clear();
    }
    Ok(e)
}

pub async fn download(s: &AppState, cat: &Catalog, id: &str) -> ApiResult<Download> {
    let m = metadata(s, cat, id).await?;
    if restricted(&m) {
        return Err(ApiError::forbidden("That book is only for lending on Internet Archive, it can't be downloaded"));
    }
    let open = |f: &&FileInfo| f.private.as_ref().is_none_or(|p| !matches!(p, Value::Bool(true)) && p.as_str() != Some("true"));
    let pick = m
        .files
        .iter()
        .filter(open)
        .find(|f| f.format.as_deref() == Some("EPUB") || f.name.to_lowercase().ends_with(".epub"))
        .map(|f| (f, "epub"))
        .or_else(|| m.files.iter().filter(open).find(|f| f.format.as_deref() == Some("Text PDF")).map(|f| (f, "pdf")))
        .or_else(|| m.files.iter().filter(open).find(|f| f.name.to_lowercase().ends_with(".pdf")).map(|f| (f, "pdf")));
    let (file, format) = pick.ok_or_else(|| ApiError::not_found("That book has no EPUB or PDF to download"))?;
    let base = cat.base.trim_end_matches('/');
    let enc: String = file.name.split('/').map(|seg| url::form_urlencoded::byte_serialize(seg.as_bytes()).collect::<String>().replace('+', "%20")).collect::<Vec<_>>().join("/");
    let md = &m.metadata;
    Ok(Download {
        url: format!("{base}/download/{id}/{enc}"),
        format,
        ext: format,
        key: key_for(cat, id),
        hint: Hint {
            title: md.get("title").and_then(first),
            author: md.get("creator").and_then(first),
            language: md.get("language").and_then(first).and_then(|l| normalize_lang(&l)),
            summary: md.get("description").and_then(first).map(|t| plain_text(&t, 4000)).filter(|t| !t.is_empty()),
            year: md.get("year").and_then(first).or_else(|| md.get("date").and_then(first)).and_then(|y| year_of(&y)),
        },
        cover: Some(format!("{base}/services/img/{id}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arma_la_consulta() {
        let q = query(Some("Don Quijote (1605)"), true);
        assert!(q.starts_with("(title:(Don Quijote 1605) OR creator:(Don Quijote 1605))"), "{q}");
        assert!(q.contains("-access-restricted-item:true"));
        assert!(q.contains("possible-copyright-status:NOT_IN_COPYRIGHT"));
        assert!(q.contains("language:(spa"));
        assert!(!query(None, false).contains("language"));
        assert_eq!(clean_terms("a OR b: c*"), "a b c");
    }
}
