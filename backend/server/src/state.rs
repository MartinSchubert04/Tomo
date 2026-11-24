//! Estado compartido de la app (todo `Clone` barato: `Arc`s por dentro).
use std::sync::Arc;

use axum::extract::FromRef;

use crate::{anilist::AniList, auth::Auth, catalogs::Catalogs, config::Config, db::Db, inbox::Inbox, openlibrary::OpenLibrary, suwayomi::Suwayomi};

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub db: Db,
    pub auth: Auth,
    pub suwa: Suwayomi,
    pub anilist: AniList,
    /// Cliente para las portadas de AniList (`/api/img/al/...`), con tiempo máximo.
    pub anilist_img: reqwest::Client,
    /// Catálogos de libros: cliente HTTP, firma de URLs y cachés.
    pub catalogs: Catalogs,
    pub openlib: OpenLibrary,
    pub inbox: Inbox,
}

impl FromRef<AppState> for Auth {
    fn from_ref(s: &AppState) -> Self {
        s.auth.clone()
    }
}

/// Nombre y textos de la app (el único lugar donde están es `brand.json`, en la raíz del repo).
pub fn brand_name() -> String {
    static BRAND: &str = include_str!("../../../brand.json");
    serde_json::from_str::<serde_json::Value>(BRAND)
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .unwrap_or_else(|| "Library".into())
}
