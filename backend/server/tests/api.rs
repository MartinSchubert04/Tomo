//! Tests de la API de punta a punta (sin red): la app de axum con una base en memoria y un
//! Suwayomi falso (otro axum en un puerto libre que contesta el GraphQL que usamos).
use std::io::Write;

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    response::Response,
    routing::{get, post},
    Router,
};
use http_body_util::BodyExt;
use library::{app, auth::create_user, build_state, db::Db, Config};
use serde_json::{json, Value};
use tower::ServiceExt;

struct T {
    app: Router,
    _dir: TempDir,
}

struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn setup(suwayomi: Option<String>) -> T {
    setup_with(suwayomi, None).await
}

/// Con un AniList falso (`anilist`); sin él, AniList apagado (los tests nunca salen a la red).
async fn setup_with(suwayomi: Option<String>, anilist: Option<String>) -> T {
    let dir = std::env::temp_dir().join(format!("library-test-{}-{}", std::process::id(), library::auth::new_token()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cfg = Config::new(&dir);
    cfg.web_dir = None;
    cfg.max_upload_bytes = 5 * 1024 * 1024;
    // Sin Suwayomi: un puerto donde no escucha nadie.
    cfg.suwayomi_url = suwayomi.unwrap_or_else(|| "http://127.0.0.1:9".into());
    cfg.anilist_url = anilist.unwrap_or_default();
    let db = Db::open_in_memory().unwrap();
    db.call(|c| {
        create_user(c, "admin", "secreto1", true)?;
        create_user(c, "ana", "secreto2", false)?;
        Ok(())
    })
    .await
    .unwrap();
    T { app: app(build_state(cfg, db).unwrap()), _dir: TempDir(dir) }
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

    /// Login y devuelve el header Cookie para los siguientes pedidos.
    async fn login(&self, name: &str, pw: &str) -> String {
        let res = self.req(Method::POST, "/api/auth/login", None, Some(json!({ "name": name, "password": pw }))).await;
        assert_eq!(res.status(), StatusCode::OK);
        let set = res.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
        assert!(set.contains("HttpOnly"));
        set.split(';').next().unwrap().to_string()
    }

    async fn upload(&self, cookie: &str, files: &[(&str, Vec<u8>)]) -> (StatusCode, Value) {
        let boundary = "XbOuNdArYx";
        let mut body = Vec::new();
        for (name, data) in files {
            write!(body, "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").unwrap();
            body.extend_from_slice(data);
            body.extend_from_slice(b"\r\n");
        }
        write!(body, "--{boundary}--\r\n").unwrap();
        let req = Request::post("/api/books")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(body))
            .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
}

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

fn tiny_epub() -> Vec<u8> {
    zip_of(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#),
        ("content.opf", br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Ficciones</dc:title><dc:creator>Borges</dc:creator></metadata><manifest><item id="cv" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/></manifest></package>"#),
        ("cover.jpg", b"JPEGDATA"),
    ])
}

#[tokio::test]
async fn auth_flow() {
    let t = setup(None).await;
    let (s, v) = t.json(Method::GET, "/api/me", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(v["error"], "unauthorized");
    let (s, _) = t.json(Method::POST, "/api/auth/login", None, Some(json!({ "name": "ana", "password": "mal" }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let c = t.login("ana", "secreto2").await;
    let (s, v) = t.json(Method::GET, "/api/me", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["user"]["name"], "ana");
    assert_eq!(v["user"]["admin"], false);
    assert_eq!(v["prefs"], json!({}));

    let res = t.req(Method::PUT, "/api/me/prefs", Some(&c), Some(json!({ "theme": "warp", "sources": ["1"] }))).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (_, v) = t.json(Method::GET, "/api/me", Some(&c), None).await;
    assert_eq!(v["prefs"]["theme"], "warp");

    // Admin solo para admins.
    let (s, v) = t.json(Method::GET, "/api/admin/users", Some(&c), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(v["error"], "forbidden");

    // Cambio de contraseña.
    let (s, _) = t.json(Method::POST, "/api/me/password", Some(&c), Some(json!({ "current": "x", "next": "nuevaclave" }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let res = t.req(Method::POST, "/api/me/password", Some(&c), Some(json!({ "current": "secreto2", "next": "nuevaclave" }))).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = t.req(Method::POST, "/api/auth/logout", Some(&c), None).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (s, _) = t.json(Method::GET, "/api/me", Some(&c), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    t.login("ana", "nuevaclave").await;

    // Rate limit: 8 fallos → 429 (aun con la contraseña correcta).
    for _ in 0..8 {
        t.json(Method::POST, "/api/auth/login", None, Some(json!({ "name": "ana", "password": "mal" }))).await;
    }
    let (s, v) = t.json(Method::POST, "/api/auth/login", None, Some(json!({ "name": "ana", "password": "nuevaclave" }))).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(v["error"], "rate_limited");
}

#[tokio::test]
async fn admin_users() {
    let t = setup(None).await;
    let c = t.login("admin", "secreto1").await;
    let (s, u) = t.json(Method::POST, "/api/admin/users", Some(&c), Some(json!({ "name": "beto", "password": "123456", "admin": false }))).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = t.json(Method::POST, "/api/admin/users", Some(&c), Some(json!({ "name": "BETO", "password": "123456" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, list) = t.json(Method::GET, "/api/admin/users", Some(&c), None).await;
    assert_eq!(list.as_array().unwrap().len(), 3);
    assert!(list[0]["createdAt"].as_i64().unwrap() > 0);
    // No se puede sacar el último admin ni borrarse a uno mismo.
    let (_, me) = t.json(Method::GET, "/api/me", Some(&c), None).await;
    let my_id = me["user"]["id"].as_i64().unwrap();
    let (s, _) = t.json(Method::PATCH, &format!("/api/admin/users/{my_id}"), Some(&c), Some(json!({ "admin": false }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = t.json(Method::DELETE, &format!("/api/admin/users/{my_id}"), Some(&c), None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let res = t.req(Method::DELETE, &format!("/api/admin/users/{}", u["id"]), Some(&c), None).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn books_library_and_progress() {
    let t = setup(None).await;
    let c = t.login("ana", "secreto2").await;

    let cbz = zip_of(&[("p10.jpg", b"ten"), ("p2.jpg", b"two"), ("p1.jpg", b"one"), ("info.txt", b"x")]);
    let (s, v) = t.upload(&c, &[("Ficciones.epub", tiny_epub()), ("My_Comic.cbz", cbz)]).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let epub = &v[0];
    assert_eq!(epub["title"], "Ficciones");
    assert_eq!(epub["author"], "Borges");
    assert_eq!(epub["kind"], "book");
    assert_eq!(epub["format"], "epub");
    assert_eq!(epub["source"], "Upload");
    assert_eq!(epub["uploader"], "ana");
    let eid = epub["id"].as_str().unwrap().to_string();
    let n = &eid[1..];
    assert_eq!(epub["cover"], format!("/api/books/{n}/cover"));
    let comic = &v[1];
    assert_eq!(comic["title"], "My Comic");
    assert_eq!(comic["pages"], 3);
    let cn = comic["id"].as_str().unwrap()[1..].to_string();

    // Formato no soportado y archivo que no es lo que dice.
    let (s, v) = t.upload(&c, &[("virus.exe", b"MZ".to_vec())]).await;
    assert_eq!((s, v["error"].as_str()), (StatusCode::BAD_REQUEST, Some("bad_request")));
    let (s, _) = t.upload(&c, &[("fake.pdf", b"not a pdf".to_vec())]).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, v) = t.upload(&c, &[("big.txt", vec![b'a'; 6 * 1024 * 1024])]).await;
    assert_eq!((s, v["error"].as_str()), (StatusCode::PAYLOAD_TOO_LARGE, Some("too_large")));

    // Portada, páginas del CBZ en orden natural, archivo con Range.
    let res = t.req(Method::GET, &format!("/api/books/{n}/cover"), Some(&c), None).await;
    assert_eq!(res.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(&res.into_body().collect().await.unwrap().to_bytes()[..], b"JPEGDATA");
    let (_, v) = t.json(Method::GET, &format!("/api/books/{cn}/pages"), Some(&c), None).await;
    assert_eq!(v["pages"][2], format!("/api/books/{cn}/pages/2"));
    let res = t.req(Method::GET, &format!("/api/books/{cn}/pages/1"), Some(&c), None).await;
    assert_eq!(&res.into_body().collect().await.unwrap().to_bytes()[..], b"two");
    let req = Request::get(format!("/api/books/{n}/file")).header(header::COOKIE, &c).header(header::RANGE, "bytes=0-3").body(Body::empty()).unwrap();
    let res = t.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "application/epub+zip");
    assert_eq!(&res.into_body().collect().await.unwrap().to_bytes()[..], b"PK\x03\x04");

    // Portada subida por la UI (PDF) y cantidad de páginas.
    let req = Request::put(format!("/api/books/{cn}/cover")).header(header::COOKIE, &c).header(header::CONTENT_TYPE, "image/webp").body(Body::from("WEBP")).unwrap();
    assert_eq!(t.app.clone().oneshot(req).await.unwrap().status(), StatusCode::NO_CONTENT);

    // Biblioteca.
    let (s, e) = t.json(Method::PUT, &format!("/api/library/{eid}"), Some(&c), Some(json!({ "favorite": true }))).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!(e["status"], "reading");
    assert_eq!(e["favorite"], true);
    assert_eq!(e["unread"], Value::Null);
    assert_eq!(e["work"]["title"], "Ficciones");
    let (s, _) = t.json(Method::PUT, &format!("/api/library/{eid}"), Some(&c), Some(json!({ "status": "nope" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = t.json(Method::PUT, "/api/library/b999", Some(&c), Some(json!({}))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Progreso e historial.
    let res = t.req(Method::PUT, &format!("/api/progress/{eid}"), Some(&c), Some(json!({ "chapter": "", "position": "epubcfi(/6/4)", "fraction": 0.42, "done": false }))).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (_, p) = t.json(Method::GET, &format!("/api/progress/{eid}"), Some(&c), None).await;
    assert_eq!(p["last"]["position"], "epubcfi(/6/4)");
    assert!((p["last"]["fraction"].as_f64().unwrap() - 0.42).abs() < 1e-9);
    assert!(p["last"]["updatedAt"].as_i64().unwrap() > 0);
    let (_, h) = t.json(Method::GET, "/api/history", Some(&c), None).await;
    assert_eq!(h.as_array().unwrap().len(), 1);
    assert_eq!(h[0]["work"]["id"], eid);
    let (_, lib) = t.json(Method::GET, "/api/library", Some(&c), None).await;
    assert!(lib[0]["lastReadAt"].as_i64().is_some());
    let res = t.req(Method::DELETE, &format!("/api/history/{eid}"), Some(&c), None).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (_, h) = t.json(Method::GET, "/api/history", Some(&c), None).await;
    assert!(h.as_array().unwrap().is_empty());

    // Otro usuario no ve la biblioteca de ana, pero sí los libros; no puede borrarlos.
    let c2 = t.login("admin", "secreto1").await;
    let (_, lib) = t.json(Method::GET, "/api/library", Some(&c2), None).await;
    assert!(lib.as_array().unwrap().is_empty());
    let (_, books) = t.json(Method::GET, "/api/books", Some(&c2), None).await;
    assert_eq!(books.as_array().unwrap().len(), 2);
    let res = t.req(Method::DELETE, &format!("/api/books/{n}"), Some(&c2), None).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT, "el admin sí puede");
    let (_, lib) = t.json(Method::GET, "/api/library", Some(&c), None).await;
    assert!(lib.as_array().unwrap().is_empty(), "borrar el libro lo saca de las bibliotecas");
}

// ---------- Suwayomi falso ----------

async fn fake_graphql(axum::Json(body): axum::Json<Value>) -> axum::Json<Value> {
    let q = body["query"].as_str().unwrap_or("");
    let vars = &body["variables"];
    let out = if q.contains("aboutServer") {
        json!({ "data": { "aboutServer": { "version": "test" } } })
    } else if q.contains("sources") {
        json!({ "data": { "sources": { "nodes": [
            { "id": "0", "displayName": "Local source", "lang": "localsourcelang", "iconUrl": "/api/v1/extension/icon/local", "supportsLatest": true, "contentWarning": "SAFE" },
            { "id": "11", "displayName": "Buena (ES)", "lang": "es", "iconUrl": "/api/v1/extension/icon/eu.buena", "supportsLatest": true, "contentWarning": "SAFE" },
            { "id": "22", "displayName": "Rota (ES)", "lang": "es", "iconUrl": "/api/v1/extension/icon/eu.rota", "supportsLatest": false, "contentWarning": "NSFW" },
            { "id": "33", "displayName": "English", "lang": "en", "iconUrl": "/api/v1/extension/icon/eu.en", "supportsLatest": true, "contentWarning": "SAFE" }
        ] } } })
    } else if q.contains("fetchSourceManga") {
        if vars["source"] == "22" {
            json!({ "data": null, "errors": [{ "message": "Exception while fetching data (/fetchSourceManga) : HTTP 403 Cloudflare\n\tat x.y" }] })
        } else {
            let query = vars["query"].as_str().unwrap_or("Pop");
            let mut mangas = vec![json!({ "id": 7, "title": format!("{query} 1"), "thumbnailUrl": "/api/v1/manga/7/thumbnail", "source": { "displayName": "Buena (ES)" } })];
            if query.contains("Shingeki") {
                mangas.push(json!({ "id": 8, "title": "Shingeki no Kyojin", "thumbnailUrl": "/api/v1/manga/8/thumbnail", "source": { "displayName": "Buena (ES)" } }));
            }
            json!({ "data": { "fetchSourceManga": { "hasNextPage": true, "mangas": mangas } } })
        }
    } else if q.contains("manga(id") && q.contains("chapters") {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        json!({ "data": { "manga": {
            "id": 7, "title": "Berserk", "thumbnailUrl": "/api/v1/manga/7/thumbnail", "sourceId": "11",
            "source": { "displayName": "Buena (ES)" }, "author": "Miura", "artist": "", "description": "Guts",
            "genre": ["Acción"], "status": "ON_HIATUS", "realUrl": "https://x/berserk", "initialized": true,
            "chaptersLastFetchedAt": now.to_string(),
            "chapters": { "nodes": [
                { "id": 101, "name": "Cap 1", "chapterNumber": 1.0, "scanlator": "", "uploadDate": "1000", "sourceOrder": 1 },
                { "id": 103, "name": "Cap 3", "chapterNumber": 3.0, "scanlator": "Scan X", "uploadDate": "3000", "sourceOrder": 3 },
                { "id": 102, "name": "Cap 2", "chapterNumber": 2.0, "scanlator": null, "uploadDate": "2000", "sourceOrder": 2 },
                { "id": 104, "name": "Cap 3", "chapterNumber": 3.0, "scanlator": "[es-la] Scan Y", "uploadDate": "3500", "sourceOrder": 4 }
            ] } } } })
    } else if q.contains("fetchChapterPages") {
        json!({ "data": { "fetchChapterPages": { "pages": ["/api/v1/manga/7/chapter/101/page/0", "/api/v1/manga/7/chapter/101/page/1"] } } })
    } else {
        json!({ "data": null, "errors": [{ "message": format!("fake: no sé contestar {q}") }] })
    };
    axum::Json(out)
}

async fn fake_suwayomi() -> String {
    let app = Router::new()
        .route("/api/graphql", post(fake_graphql))
        .route("/api/v1/manga/7/thumbnail", get(|| async { ([(header::CONTENT_TYPE, "image/jpeg")], "IMG") }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn manga_through_suwayomi() {
    let t = setup(Some(fake_suwayomi().await)).await;
    let c = t.login("ana", "secreto2").await;

    let (_, h) = t.json(Method::GET, "/api/health", None, None).await;
    assert_eq!(h["suwayomi"], true);
    assert_eq!(h["ok"], true);

    let (s, v) = t.json(Method::GET, "/api/sources", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 3, "sin Local source");
    assert_eq!(v[0]["icon"], "/api/img/s/extension/icon/eu.buena");
    let names: Vec<&str> = v.as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Buena (ES)", "English", "Rota (ES)"], "por nombre");
    assert_eq!(v[2]["nsfw"], "nsfw");

    let (s, v) = t.json(Method::GET, "/api/sources/11/browse?type=search&q=Berserk&page=1", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["hasNext"], true);
    assert_eq!(v["items"][0]["id"], "s7");
    assert_eq!(v["items"][0]["cover"], "/api/img/s/manga/7/thumbnail");
    let (s, v) = t.json(Method::GET, "/api/sources/22/browse", Some(&c), None).await;
    assert_eq!((s, v["error"].as_str()), (StatusCode::BAD_GATEWAY, Some("source_error")));
    assert_eq!(v["message"], "HTTP 403 Cloudflare");

    // Búsqueda global: una línea por fuente de español (la rota con su error).
    let res = t.req(Method::GET, "/api/search?q=Berserk", Some(&c), None).await;
    assert_eq!(res.headers()[header::CONTENT_TYPE], "application/x-ndjson");
    let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    let ok = lines.iter().find(|l| l["source"]["id"] == "11").unwrap();
    assert_eq!(ok["items"][0]["title"], "Berserk 1");
    assert!(ok.get("error").is_none());
    let bad = lines.iter().find(|l| l["source"]["id"] == "22").unwrap();
    assert!(bad["error"].as_str().unwrap().contains("Cloudflare"));
    // Fuentes elegidas a mano.
    let res = t.req(Method::GET, "/api/search?q=x&sources=33", Some(&c), None).await;
    let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert_eq!(text.lines().count(), 1);

    // Ficha: capítulos del más nuevo al más viejo.
    let (s, d) = t.json(Method::GET, "/api/manga/s7", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["title"], "Berserk");
    assert_eq!(d["status"], "hiatus");
    assert_eq!(d["artist"], Value::Null);
    assert_eq!(d["sourceId"], "11");
    let ids: Vec<&str> = d["chapters"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["104", "103", "102", "101"]);
    // El idioma sale del prefijo del scan ("[es-la] Scan Y" → es-419, "Scan Y").
    assert_eq!(d["chapters"][0]["lang"], "es-419");
    assert_eq!(d["chapters"][0]["scanlator"], "Scan Y");
    assert_eq!(d["chapters"][1]["scanlator"], "Scan X");
    assert_eq!(d["chapters"][1]["lang"], Value::Null);
    assert_eq!(d["chapters"][3]["scanlator"], Value::Null);
    assert_eq!(d["chapters"][1]["date"], 3000);
    let (s, _) = t.json(Method::GET, "/api/manga/x7", Some(&c), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (_, p) = t.json(Method::GET, "/api/manga/s7/chapters/101/pages", Some(&c), None).await;
    assert_eq!(p["pages"][1], "/api/img/s/manga/7/chapter/101/page/1");

    // Proxy de imágenes: solo rutas permitidas, con caché.
    let res = t.req(Method::GET, "/api/img/s/manga/7/thumbnail", Some(&c), None).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "private, max-age=86400");
    assert_eq!(&res.into_body().collect().await.unwrap().to_bytes()[..], b"IMG");
    let res = t.req(Method::GET, "/api/img/s/graphql", Some(&c), None).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = t.req(Method::GET, "/api/img/s/manga/7/thumbnail", None, None).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Biblioteca con no leídos, progreso, siguiente capítulo.
    let (_, e) = t.json(Method::PUT, "/api/library/s7", Some(&c), Some(json!({ "status": "reading" }))).await;
    // Tres números (el 3 está dos veces): se cuentan por número. Recién agregado, nada es "nuevo".
    assert_eq!(e["unread"], 3);
    assert_eq!(e["newChapters"], 0);
    assert_eq!(e["latestChapterAt"], 3500);
    assert_eq!(e["work"]["source"], "Buena (ES)");
    let res = t.req(Method::PUT, "/api/progress/s7", Some(&c), Some(json!({ "chapter": "101", "position": "39", "fraction": 1.0, "done": true }))).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (s, _) = t.json(Method::PUT, "/api/progress/s7", Some(&c), Some(json!({ "position": "1", "fraction": 0.1 }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "manga sin capítulo");
    let (_, h) = t.json(Method::GET, "/api/history", Some(&c), None).await;
    assert_eq!(h[0]["chapterName"], "Cap 1");
    assert_eq!(h[0]["nextChapter"], "102");
    assert_eq!(h[0]["work"]["title"], "Berserk");
    let res = t.req(Method::POST, "/api/progress/s7/mark", Some(&c), Some(json!({ "chapters": ["102", "103"], "done": true }))).await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (_, lib) = t.json(Method::GET, "/api/library", Some(&c), None).await;
    assert_eq!(lib[0]["unread"], 0, "leer el 103 cuenta para el 3 (también está el 104)");
    t.req(Method::POST, "/api/progress/s7/mark", Some(&c), Some(json!({ "chapters": ["103"], "done": false }))).await;
    let (_, p) = t.json(Method::GET, "/api/progress/s7", Some(&c), None).await;
    assert_eq!(p["chapters"]["103"]["done"], false);
    assert_eq!(p["last"]["chapter"], "101");
    let (_, lib) = t.json(Method::GET, "/api/library", Some(&c), None).await;
    assert_eq!(lib[0]["unread"], 1);
}

#[tokio::test]
async fn suwayomi_down() {
    let t = setup(None).await;
    let c = t.login("ana", "secreto2").await;
    let (s, v) = t.json(Method::GET, "/api/sources", Some(&c), None).await;
    assert_eq!((s, v["error"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("unavailable")));
    let (_, h) = t.json(Method::GET, "/api/health", None, None).await;
    assert_eq!(h["suwayomi"], false);
    let (s, _) = t.json(Method::GET, "/api/nope", Some(&c), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

// ---------- AniList falso ----------

async fn fake_anilist() -> String {
    let app = Router::new().route(
        "/",
        post(|axum::Json(body): axum::Json<Value>| async move {
            let q = body["variables"]["q"].as_str().unwrap_or("").to_lowercase();
            let media = if q.contains("attack") || q.contains("titan") || q.contains("berserk") {
                json!([
                    { "id": 53390, "title": { "romaji": "Shingeki no Kyojin", "english": "Attack on Titan", "native": "進撃の巨人" },
                      "synonyms": ["AoT"], "coverImage": { "medium": "https://s4.anilist.co/file/anilistcdn/media/manga/cover/small/bx53390.jpg" },
                      "format": "MANGA", "countryOfOrigin": "JP", "startDate": { "year": 2009 }, "isAdult": false },
                    { "id": 30002, "title": { "romaji": "Berserk", "english": "Attack Berserk", "native": "ベルセルク" },
                      "synonyms": [], "coverImage": { "medium": null }, "format": "MANGA", "countryOfOrigin": "JP", "startDate": { "year": 1989 }, "isAdult": false }
                ])
            } else {
                json!([])
            };
            axum::Json(json!({ "data": { "Page": { "media": media } } }))
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}/")
}

async fn ndjson(t: &T, uri: &str, cookie: &str) -> Vec<Value> {
    let res = t.req(Method::GET, uri, Some(cookie), None).await;
    let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[tokio::test]
async fn suggestions_and_other_titles() {
    let t = setup_with(Some(fake_suwayomi().await), Some(fake_anilist().await)).await;
    let c = t.login("ana", "secreto2").await;
    // Berserk queda conocido (tarjeta guardada) al abrir la ficha.
    let (s, _) = t.json(Method::GET, "/api/manga/s7", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK);

    // Lo conocido por texto y, por AniList, los títulos en otros idiomas.
    let (s, v) = t.json(Method::GET, "/api/suggest?q=attack", Some(&c), None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let v = v.as_array().unwrap();
    assert_eq!(v[0]["kind"], "title");
    assert_eq!(v[0]["title"], "Shingeki no Kyojin");
    assert_eq!(v[0]["altTitles"][0], "Attack on Titan");
    assert_eq!(v[0]["format"], "manga");
    assert_eq!(v[0]["year"], 2009);
    assert_eq!(v[0]["cover"], "/api/img/al/file/anilistcdn/media/manga/cover/small/bx53390.jpg");
    // "Attack Berserk" es Berserk, que ya existe: abre la ficha.
    assert_eq!(v[1]["kind"], "work");
    assert_eq!(v[1]["workId"], "s7");
    assert_eq!(v[1]["altTitles"][0], "Attack Berserk");
    let (_, v) = t.json(Method::GET, "/api/suggest?q=bers", Some(&c), None).await;
    assert_eq!(v[0]["workId"], "s7", "lo conocido primero");
    let (_, v) = t.json(Method::GET, "/api/suggest?q=a", Some(&c), None).await;
    assert!(v.as_array().unwrap().is_empty(), "muy corto");

    // Búsqueda con otros títulos: cada fuente busca los dos y se juntan sin repetir.
    let lines = ndjson(&t, "/api/search?q=attack%20on%20titan&alt=Shingeki%20no%20Kyojin&sources=11", &c).await;
    assert_eq!(lines[0]["alts"], json!(["Shingeki no Kyojin"]));
    let ids: Vec<&str> = lines[1]["items"].as_array().unwrap().iter().map(|w| w["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["s7", "s8"]);
    // Enter sin elegir: el server le pregunta a AniList.
    let lines = ndjson(&t, "/api/search?q=attack%20on%20titan&resolve=1&sources=11", &c).await;
    assert_eq!(lines[0]["alts"], json!(["Shingeki no Kyojin", "AoT"]));
    assert!(lines[1]["items"].as_array().unwrap().iter().any(|w| w["title"] == "Shingeki no Kyojin"));
    // Sin otros títulos no hay línea extra.
    let lines = ndjson(&t, "/api/search?q=Berserk&resolve=1&sources=11", &c).await;
    assert_eq!(lines.len(), 1);
    assert!(lines[0].get("alts").is_none());
}
