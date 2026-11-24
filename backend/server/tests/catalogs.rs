//! Catálogos de libros, carpeta de entrada y Open Library de punta a punta, sin red: un server
//! falso (axum en un puerto libre) hace de Internet Archive, Elejandría, un catálogo OPDS y Open
//! Library a la vez.
use std::io::Write;

use axum::{
    body::Body,
    extract::{Path, Query},
    http::{header, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use http_body_util::BodyExt;
use library::{app, auth::create_user, build_state, db::Db, Config};
use serde_json::{json, Value};
use tower::ServiceExt;

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut out);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (n, d) in files {
            z.start_file(*n, opt).unwrap();
            z.write_all(d).unwrap();
        }
        z.finish().unwrap();
    }
    out.into_inner()
}

/// EPUB con saga (calibre y EPUB 3), idioma, sinopsis y año.
fn epub(title: &str, series: bool) -> Vec<u8> {
    let meta = if series {
        r#"<meta name="calibre:series" content="La rueda del tiempo"/><meta name="calibre:series_index" content="3"/>"#
    } else {
        r##"<meta property="belongs-to-collection" id="c1">Episodios nacionales</meta><meta refines="#c1" property="group-position">2</meta>"##
    };
    let opf = format!(
        r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:title>{title}</dc:title><dc:language>es-AR</dc:language><dc:description>&lt;p&gt;Una &lt;b&gt;historia&lt;/b&gt;.&lt;/p&gt;</dc:description>
        <dc:date>1990-01-01</dc:date>{meta}</metadata><manifest></manifest></package>"#
    );
    zip_of(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#),
        ("content.opf", opf.as_bytes()),
    ])
}

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 0];

fn bytes(ct: &'static str, b: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, ct)], b).into_response()
}

async fn fake_remote() -> String {
    let big_png = || {
        let mut v = PNG.to_vec();
        v.resize(4096, 0);
        v
    };
    let app = Router::new()
        // ---- Internet Archive ----
        .route(
            "/advancedsearch.php",
            get(|Query(q): Query<Vec<(String, String)>>| async move {
                let query = q.iter().find(|(k, _)| k == "q").map(|(_, v)| v.clone()).unwrap_or_default();
                assert!(query.contains("-access-restricted-item:true") && query.contains("NOT_IN_COPYRIGHT"), "{query}");
                axum::Json(json!({ "response": { "numFound": 41, "docs": [
                    { "identifier": "marianela01", "title": "Marianela;", "creator": ["Pérez Galdós, Benito"], "language": ["spa"], "year": "1878", "format": ["EPUB", "Text PDF"] },
                    { "identifier": "bad id!", "title": "x" }
                ]}}))
            }),
        )
        .route(
            "/metadata/{id}",
            get(|Path(id): Path<String>| async move {
                match id.as_str() {
                    "marianela01" => axum::Json(json!({ "metadata": { "title": "Marianela", "creator": "Pérez Galdós, Benito", "language": "spa", "year": "1878",
                        "description": "Novela de 1878." }, "files": [ { "name": "secreto.epub", "format": "EPUB", "private": "true" },
                        { "name": "Marianela libro.epub", "format": "EPUB" }, { "name": "m.pdf", "format": "Text PDF" } ] })),
                    _ => axum::Json(json!({ "metadata": { "title": "Prestado", "access-restricted-item": "true" }, "files": [ { "name": "p.epub", "format": "EPUB" } ] })),
                }
            }),
        )
        .route("/download/marianela01/{file}", get(|Path(f): Path<String>| async move {
            assert_eq!(f, "Marianela libro.epub");
            bytes("application/epub+zip", epub("", false))
        }))
        .route("/services/img/{id}", get(move || async move { bytes("image/png", big_png()) }))
        // ---- Elejandría ----
        .route("/sitemap.xml", get(|| async {
            "<urlset><url><loc>http://h/libro/orgullo-y-prejuicio/jane-austen/20</loc></url><url><loc>http://h/libro/la-isla-del-tesoro/stevenson-robert/7</loc></url></urlset>"
        }))
        .route("/libros/{l}", get(|Path(l): Path<String>| async move {
            if l == "o" {
                r#"<div class="col-lg-2 col-6"><a href="http://h/libro/orgullo-y-prejuicio/jane-austen/20"><img data-src="http://h/covers/o.png"/></a>
                   <a href="http://h/libro/orgullo-y-prejuicio/jane-austen/20" title="Libro Orgullo y prejuicio">X</a><a title="Libros de Jane Austen">Y</a></div>"#.to_string()
            } else {
                String::new()
            }
        }))
        .route("/libros-mas-descargados", get(|| async {
            r#"<div class="col-lg-2"><a href="http://h/libro/la-isla-del-tesoro/stevenson-robert/7">x</a></div>"#
        }))
        .route("/libro/{t}/{a}/{id}", get(|Path((_t, _a, id)): Path<(String, String, u64)>| async move {
            assert_eq!(id, 20);
            r#"<meta property="og:image" content="BASE/covers/o-lg.png" />
            <h1 class="mb-2">Libro Orgullo y prejuicio<span> en PDF</span></h1>
            <h2><a href="BASE/autor/jane-austen/20">de Jane Austen</a></h2>
            <div class="book-abstract mt-3"><p>La novela.</p></div>
            <a href="BASE/libro/descargar/orgullo-y-prejuicio/jane-austen/20/1016"><img alt="ePub"/> Descargar en ePub</a>"#
                .to_string()
        }))
        .route("/libro/link_descarga_libro/{id}/{f}", get(|Path((id, f)): Path<(u64, u64)>| async move {
            assert_eq!((id, f), (20, 1016));
            bytes("application/octet-stream", epub("Orgullo y prejuicio", true))
        }))
        // ---- OPDS ----
        .route("/opds", get(|| async {
            bytes("application/atom+xml", br#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Textos</title>
              <link rel="search" type="application/opensearchdescription+xml" href="/osd.xml"/>
              <entry><title>Populares</title><link rel="subsection" type="application/atom+xml;profile=opds-catalog" href="/opds/top"/></entry></feed>"#.to_vec())
        }))
        .route("/osd.xml", get(|| async {
            r#"<OpenSearchDescription><Url type="application/atom+xml" template="/opds/search?q={searchTerms}&amp;p={startPage?}"/></OpenSearchDescription>"#
        }))
        .route("/opds/{which}", get(|Path(w): Path<String>, Query(q): Query<Vec<(String, String)>>| async move {
            if w == "search" {
                assert_eq!(q.iter().find(|(k, _)| k == "q").map(|(_, v)| v.as_str()), Some("la celestina"));
            }
            bytes("application/atom+xml", br#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/"><title>Lista</title>
              <entry><title>La Celestina</title><author><name>Fernando de Rojas</name></author><dc:language>es</dc:language>
                <link rel="http://opds-spec.org/image" href="/img/c.png"/>
                <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/files/c.epub"/></entry>
              <entry><title>Hamlet</title><dc:language>en</dc:language><link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/files/h.epub"/></entry>
              </feed>"#.to_vec())
        }))
        .route("/files/c.epub", get(|| async { bytes("application/epub+zip", epub("La Celestina", false)) }))
        .route("/img/c.png", get(|| async { bytes("image/png", PNG.to_vec()) }))
        // ---- Open Library ----
        .route("/search.json", get(|| async {
            axum::Json(json!({ "docs": [ { "key": "/works/OL1W", "title": "Libro suelto", "author_name": ["Autora Real"], "first_publish_year": 1901, "language": ["spa"], "cover_i": 9 } ] }))
        }))
        .route("/works/OL1W.json", get(|| async { axum::Json(json!({ "description": { "value": "Sinopsis de Open Library." } })) }))
        .route("/b/id/{f}", get(move || async move {
            let mut v = PNG.to_vec();
            v.resize(4096, 0);
            bytes("image/jpeg", v)
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

struct T {
    app: Router,
    state: library::state::AppState,
    dir: std::path::PathBuf,
}

impl Drop for T {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn setup(openlib: bool) -> (T, String) {
    let remote = fake_remote().await;
    let dir = std::env::temp_dir().join(format!("library-cat-{}-{}", std::process::id(), library::auth::new_token()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::new(&dir);
    cfg.web_dir = None;
    cfg.suwayomi_url = "http://127.0.0.1:9".into();
    cfg.anilist_url.clear();
    cfg.archive_url = remote.clone();
    cfg.elejandria_url = remote.clone();
    cfg.textos_url = format!("{remote}/opds");
    cfg.gutenberg_url.clear();
    cfg.openlibrary_url = if openlib { remote.clone() } else { String::new() };
    cfg.inbox_every = None;
    cfg.inbox_stable = std::time::Duration::ZERO;
    cfg.max_upload_bytes = 5 * 1024 * 1024;
    let db = Db::open_in_memory().unwrap();
    db.call(|c| {
        create_user(c, "admin", "secreto1", true)?;
        create_user(c, "ana", "secreto2", false)?;
        Ok(())
    })
    .await
    .unwrap();
    let state = build_state(cfg, db).unwrap();
    library::catalogs::ensure_presets(&state).await.unwrap();
    (T { app: app(state.clone()), state, dir }, remote)
}

impl T {
    async fn req(&self, method: Method, uri: &str, cookie: Option<&str>, body: Option<Value>) -> Response {
        let mut b = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        let req = match body {
            Some(v) => b.header(header::CONTENT_TYPE, "application/json").body(Body::from(v.to_string())).unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        self.app.clone().oneshot(req).await.unwrap()
    }

    async fn json(&self, method: Method, uri: &str, cookie: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
        let res = self.req(method, uri, cookie, body).await;
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn login(&self, name: &str, pw: &str) -> String {
        let res = self.req(Method::POST, "/api/auth/login", None, Some(json!({ "name": name, "password": pw }))).await;
        assert_eq!(res.status(), StatusCode::OK);
        res.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().split(';').next().unwrap().to_string()
    }

    async fn ndjson(&self, uri: &str, c: &str) -> Vec<Value> {
        let res = self.req(Method::GET, uri, Some(c), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "application/x-ndjson");
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap().lines().filter(|l| !l.is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect()
    }
}

#[tokio::test]
async fn catalogs_browse_search_and_import() {
    let (t, _) = setup(false).await;
    let c = t.login("ana", "secreto2").await;
    let (s, v) = t.json(Method::GET, "/api/catalogs", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK);
    let ids: Vec<&str> = v.as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["elejandria", "textos", "archive"], "los de español primero; Gutenberg apagado (y acá sin URL)");
    assert!(v[0].get("url").is_none(), "la URL es solo para el admin");

    // Internet Archive: lista con portada firmada; los ids raros se descartan.
    let (s, p) = t.json(Method::GET, "/api/catalogs/archive/browse", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["entries"].as_array().unwrap().len(), 1);
    let e = &p["entries"][0];
    assert_eq!(e["title"], "Marianela");
    assert_eq!(e["language"], "es");
    assert_eq!(e["year"], 1878);
    assert_eq!(e["formats"], json!(["epub", "pdf"]));
    assert_eq!(p["next"], "p2");
    let cover = e["cover"].as_str().unwrap().to_string();
    assert!(cover.starts_with("/api/img/cat?t="));
    let res = t.req(Method::GET, &cover, Some(&c), None).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "image/png");
    // Una portada con la firma tocada no pasa (no es un proxy abierto).
    let res = t.req(Method::GET, &format!("{}x", cover), Some(&c), None).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Importar: elige el EPUB que no es privado, lo baja y no lo repite.
    let (s, b) = t.json(Method::POST, "/api/catalogs/archive/import", Some(&c), Some(json!({ "entry": "marianela01" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["origin"], "Internet Archive");
    assert_eq!(b["format"], "epub");
    // El EPUB no traía título: se usa el del catálogo. El año del catálogo le gana al del OPF.
    assert_eq!(b["title"], "Marianela");
    assert_eq!(b["author"], "Pérez Galdós, Benito");
    assert_eq!(b["year"], 1878);
    assert_eq!(b["series"], "Episodios nacionales");
    assert_eq!(b["seriesIndex"], 2.0);
    assert_eq!(b["cover"], format!("/api/books/{}/cover", b["id"].as_str().unwrap().trim_start_matches('b')));
    let (s, again) = t.json(Method::POST, "/api/catalogs/archive/import", Some(&c), Some(json!({ "entry": "marianela01" }))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(again["id"], b["id"]);
    let (_, p) = t.json(Method::GET, "/api/catalogs/archive/browse", Some(&c), None).await;
    assert_eq!(p["entries"][0]["imported"], b["id"]);
    // Los de préstamo no se bajan.
    let (s, _) = t.json(Method::POST, "/api/catalogs/archive/import", Some(&c), Some(json!({ "entry": "prestado" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Elejandría: índice del sitemap + listas por letra; sin búsqueda, los más bajados primero.
    let (s, p) = t.json(Method::GET, "/api/catalogs/elejandria/browse", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    let titles: Vec<&str> = p["entries"].as_array().unwrap().iter().map(|e| e["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["La isla del tesoro", "Orgullo y prejuicio"]);
    let (_, p) = t.json(Method::GET, "/api/catalogs/elejandria/browse?q=prejuicio%20austen", Some(&c), None).await;
    assert_eq!(p["entries"][0]["id"], "20");
    assert_eq!(p["entries"][0]["author"], "Jane Austen");
    let (s, d) = t.json(Method::GET, "/api/catalogs/elejandria/entry/20", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["summary"], "La novela.");
    let (s, b) = t.json(Method::POST, "/api/catalogs/elejandria/import", Some(&c), Some(json!({ "entry": "20" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["series"], "La rueda del tiempo");
    assert_eq!(b["seriesIndex"], 3.0);
    assert_eq!(b["language"], "es");
    assert_eq!(b["description"], "Una historia.", "la sinopsis más larga (la del EPUB)");

    // OPDS: navegación, búsqueda por OpenSearch, filtro de español e importación firmada.
    let (s, p) = t.json(Method::GET, "/api/catalogs/textos/browse", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["navigation"][0]["title"], "Populares");
    assert_eq!(p["entries"].as_array().unwrap().len(), 1, "la raíz solo tiene secciones: se abre la primera");
    let path = p["navigation"][0]["path"].as_str().unwrap().to_string();
    let (_, p) = t.json(Method::GET, &format!("/api/catalogs/textos/browse?path={path}"), Some(&c), None).await;
    assert_eq!(p["entries"].as_array().unwrap().len(), 1, "Hamlet (en inglés) no aparece: {p}");
    let (_, p) = t.json(Method::GET, &format!("/api/catalogs/textos/browse?path={path}&lang=all"), Some(&c), None).await;
    assert_eq!(p["entries"].as_array().unwrap().len(), 2);
    let (s, _) = t.json(Method::GET, "/api/catalogs/textos/browse?path=aHR0cDovL2V2aWw.AAAA", Some(&c), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "una página sin firma no se pide");

    let lines = t.ndjson("/api/catalogs/search?q=la%20celestina", &c).await;
    assert_eq!(lines.len(), 3);
    let textos = lines.iter().find(|l| l["catalog"]["id"] == "textos").unwrap();
    assert_eq!(textos["entries"][0]["title"], "La Celestina");
    let entry = textos["entries"][0]["id"].as_str().unwrap().to_string();
    let (s, b) = t.json(Method::POST, "/api/catalogs/textos/import", Some(&c), Some(json!({ "entry": entry }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["origin"], "textos.info");
    let (s, _) = t.json(Method::POST, "/api/catalogs/textos/import", Some(&c), Some(json!({ "entry": "aHR0cDovL2V2aWw.AAAA" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Editar saga y número; "" la borra.
    let id = b["id"].as_str().unwrap().to_string();
    let (s, e) = t.json(Method::PATCH, &format!("/api/books/{id}"), Some(&c), Some(json!({ "series": "Clásicos", "seriesIndex": 4, "year": 1499, "language": "Español" }))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!((e["series"].as_str(), e["seriesIndex"].as_f64(), e["year"].as_i64(), e["language"].as_str()), (Some("Clásicos"), Some(4.0), Some(1499), Some("es")));
    let (_, e) = t.json(Method::PATCH, &format!("/api/books/{id}"), Some(&c), Some(json!({ "series": "" }))).await;
    assert!(e["series"].is_null() && e["seriesIndex"].is_null());
}

#[tokio::test]
async fn admin_catalogs() {
    let (t, remote) = setup(false).await;
    let ana = t.login("ana", "secreto2").await;
    let adm = t.login("admin", "secreto1").await;
    let (s, _) = t.json(Method::GET, "/api/admin/catalogs", Some(&ana), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = t.json(Method::GET, "/api/admin/catalogs", Some(&adm), None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v[0]["url"].is_string());

    // Agregar: se prueba antes (tiene que ser OPDS).
    let (s, _) = t.json(Method::POST, "/api/admin/catalogs", Some(&adm), Some(json!({ "url": format!("{remote}/sitemap.xml") }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, cat) = t.json(Method::POST, "/api/admin/catalogs", Some(&adm), Some(json!({ "url": format!("{remote}/opds/top") }))).await;
    assert_eq!(s, StatusCode::OK, "{cat}");
    assert_eq!(cat["name"], "Lista");
    assert_eq!(cat["preset"], false);
    let (s, _) = t.json(Method::POST, "/api/admin/catalogs", Some(&adm), Some(json!({ "url": format!("{remote}/opds/top") }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let id = cat["id"].as_str().unwrap();

    // Apagar uno de fábrica: deja de verse; borrarlo no se puede.
    let (s, _) = t.json(Method::PATCH, "/api/admin/catalogs/archive", Some(&adm), Some(json!({ "enabled": false }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, v) = t.json(Method::GET, "/api/catalogs", Some(&ana), None).await;
    assert!(!v.as_array().unwrap().iter().any(|c| c["id"] == "archive"));
    let (s, _) = t.json(Method::GET, "/api/catalogs/archive/browse", Some(&ana), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = t.json(Method::DELETE, "/api/admin/catalogs/archive", Some(&adm), None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = t.json(Method::DELETE, &format!("/api/admin/catalogs/{id}"), Some(&adm), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn inbox_and_open_library() {
    let (t, _) = setup(true).await;
    let adm = t.login("admin", "secreto1").await;
    let inbox = t.dir.join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    // Un EPUB sin autor ni portada (Open Library los completa), uno que no es libro y uno a medio
    // bajar (se ignora).
    std::fs::write(inbox.join("Libro suelto.epub"), epub("Libro suelto", true)).unwrap();
    std::fs::write(inbox.join("notas.docx"), b"hola").unwrap();
    std::fs::write(inbox.join("bajando.epub.part"), b"PK").unwrap();
    // Primera pasada: anota tamaños; segunda: importa lo que no cambió.
    assert_eq!(library::inbox::scan(&t.state).await, 0);
    assert_eq!(library::inbox::scan(&t.state).await, 1);
    assert!(!inbox.join("Libro suelto.epub").exists());
    assert!(inbox.join("failed/notas.docx").exists());
    assert!(std::fs::read_to_string(inbox.join("failed/notas.docx.error.txt")).unwrap().contains("Unsupported"));
    let (s, st) = t.json(Method::GET, "/api/admin/inbox", Some(&adm), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(st["pending"], json!([]));
    assert_eq!(st["failed"][0]["name"], "notas.docx");

    let (_, books) = t.json(Method::GET, "/api/books", Some(&adm), None).await;
    let b = &books[0];
    assert_eq!(b["origin"], "inbox");
    assert_eq!(b["uploader"], "inbox");
    assert_eq!(b["series"], "La rueda del tiempo");
    assert_eq!(b["language"], "es");
    assert_eq!(b["description"], "Una historia.");
    // Open Library corre aparte: esperarlo un poco.
    let id = b["id"].as_str().unwrap().to_string();
    let mut done = Value::Null;
    for _ in 0..50 {
        let (_, v) = t.json(Method::GET, &format!("/api/books/{id}"), Some(&adm), None).await;
        if v["author"].is_string() && v["cover"].is_string() {
            done = v;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(done["author"], "Autora Real", "{done}");
    assert_eq!(done["year"], 1990, "el año del archivo no se pisa");
    assert_eq!(done["description"], "Una historia.", "la sinopsis del archivo no se pisa");
}
