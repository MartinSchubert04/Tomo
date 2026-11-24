//! Configuración del server. El binario la arma desde las variables `LIBRARY_*` (ver `main.rs` y
//! `deploy/.env.example`); los tests la arman en código con `Config::new`.
use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Clone, Debug)]
pub struct Config {
    /// `127.0.0.1:0` = puerto libre cualquiera (el real lo da `Handle::addr`).
    pub bind: SocketAddr,
    /// Base de datos (`library.db`), libros (`books/`) y portadas (`covers/`).
    pub data_dir: PathBuf,
    /// UI compilada para servir en `/`. `None` = no se sirve.
    pub web_dir: Option<PathBuf>,
    /// Sin barra final, p. ej. `http://suwayomi:4567`.
    pub suwayomi_url: String,
    /// Repo de extensiones que se agrega a Suwayomi si no tiene ninguno. Vacío = ninguno.
    pub extension_repo: String,
    /// Tope de cada archivo subido.
    pub max_upload_bytes: u64,
    /// Cada cuánto buscar capítulos nuevos de lo que está en las bibliotecas. `None` = nunca.
    pub update_every: Option<Duration>,
    /// GraphQL de AniList para las sugerencias (títulos en otros idiomas). Vacío = sin AniList.
    pub anilist_url: String,
    /// Catálogos de libros de fábrica. Vacío = ese catálogo no se ofrece.
    pub archive_url: String,
    pub elejandria_url: String,
    /// Raíz OPDS de textos.info (biblioteca digital abierta en español).
    pub textos_url: String,
    /// Raíz OPDS de Project Gutenberg.
    pub gutenberg_url: String,
    /// Open Library para completar datos de los libros. Vacío = no se usa.
    pub openlibrary_url: String,
    /// Cada cuánto revisar la carpeta de entrada (`<datos>/inbox`). `None` = nunca.
    pub inbox_every: Option<Duration>,
    /// Cuánto tiene que estar quieto un archivo de la carpeta de entrada antes de importarlo.
    pub inbox_stable: Duration,
    /// Dónde está la carpeta de entrada en la máquina (solo para mostrarlo en Admin).
    pub inbox_label: Option<String>,
    /// Primer admin si la base está vacía.
    pub admin_user: Option<String>,
    pub admin_password: Option<String>,
}

pub const DEFAULT_ANILIST_URL: &str = "https://graphql.anilist.co";

pub const DEFAULT_EXTENSION_REPO: &str = "https://raw.githubusercontent.com/keiyoushi/extensions/repo/index.min.json";

impl Config {
    /// Valores por defecto (los mismos que sin variables de entorno).
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 7710)),
            data_dir: data_dir.into(),
            web_dir: Some("../apps/web/dist".into()),
            suwayomi_url: "http://127.0.0.1:4567".into(),
            extension_repo: DEFAULT_EXTENSION_REPO.into(),
            max_upload_bytes: 1024 * 1024 * 1024,
            update_every: Some(Duration::from_secs(6 * 3600)),
            anilist_url: DEFAULT_ANILIST_URL.into(),
            archive_url: "https://archive.org".into(),
            elejandria_url: "https://www.elejandria.com".into(),
            textos_url: "https://www.textos.info/opds".into(),
            gutenberg_url: "https://www.gutenberg.org/ebooks.opds/".into(),
            openlibrary_url: "https://openlibrary.org".into(),
            inbox_every: Some(Duration::from_secs(30)),
            inbox_stable: Duration::from_secs(10),
            inbox_label: None,
            admin_user: None,
            admin_password: None,
        }
    }
}
