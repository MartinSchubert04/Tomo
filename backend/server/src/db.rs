//! SQLite (rusqlite) en `$LIBRARY_DATA/library.db`: una sola conexión protegida por un mutex y
//! usada desde `spawn_blocking` (para unos pocos usuarios sobra y es lo más liviano). Migraciones
//! con `PRAGMA user_version` al arrancar.
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use rusqlite::Connection;

use crate::error::{ApiError, ApiResult};

/// Cada entrada es una migración; se aplican en orden y nunca se editan las ya publicadas.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    admin         INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL                -- unix ms
);
CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,                  -- sha256 hex del token, nunca el token
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,                  -- unix s
    last_used  INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX sessions_user ON sessions(user_id);
CREATE TABLE prefs (
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    data    TEXT NOT NULL                         -- objeto JSON libre de la UI
);

-- Copia de la tarjeta de cada obra de manga (título, portada, fuente), para mostrar biblioteca e
-- historial sin preguntarle a Suwayomi. Los libros salen de `books`.
CREATE TABLE works (
    id         TEXT PRIMARY KEY,                  -- "s123"
    title      TEXT NOT NULL,
    cover      TEXT,
    source     TEXT,
    updated_at INTEGER NOT NULL
);
-- Capítulos conocidos de cada manga (de la ficha o de la búsqueda de novedades): para contar los
-- no leídos, el nombre del capítulo en el historial y el siguiente capítulo.
CREATE TABLE chapters (
    work_id TEXT NOT NULL,
    id      TEXT NOT NULL,
    name    TEXT NOT NULL,
    number  REAL NOT NULL,
    ord     INTEGER NOT NULL,                     -- sourceOrder de Suwayomi: más alto = más nuevo
    date    INTEGER NOT NULL,
    PRIMARY KEY (work_id, id)
);
CREATE INDEX chapters_ord ON chapters(work_id, ord);
CREATE TABLE manga_meta (
    work_id    TEXT PRIMARY KEY,
    latest_at  INTEGER,                           -- fecha del capítulo más nuevo (ms)
    checked_at INTEGER NOT NULL                   -- última vez que se pidieron los capítulos (ms)
);

CREATE TABLE library (
    user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    work_id  TEXT NOT NULL,
    status   TEXT NOT NULL,
    favorite INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, work_id)
);
CREATE INDEX library_work ON library(work_id);
-- Progreso por capítulo (libros: capítulo "").
CREATE TABLE progress (
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    work_id    TEXT NOT NULL,
    chapter    TEXT NOT NULL,
    position   TEXT NOT NULL,
    fraction   REAL NOT NULL,
    done       INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, work_id, chapter)
);
-- Lo último leído de cada obra ("Seguir leyendo"). `hidden` = sacado del historial a mano.
CREATE TABLE last_read (
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    work_id    TEXT NOT NULL,
    chapter    TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    hidden     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, work_id)
);
CREATE INDEX last_read_recent ON last_read(user_id, updated_at DESC);

-- Libros y cómics subidos (compartidos por todos). El archivo: books/<id>.<ext>; la portada:
-- covers/b<id>.<cover_ext>.
CREATE TABLE books (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT NOT NULL,
    author      TEXT,
    format      TEXT NOT NULL,
    ext         TEXT NOT NULL,
    size        INTEGER NOT NULL,
    added_at    INTEGER NOT NULL,
    uploader_id INTEGER,
    uploader    TEXT NOT NULL,
    pages       INTEGER,
    cover_ext   TEXT
);
"#,
    // 2: cuándo se vio cada capítulo por primera vez (0 = ya estaba en la primera lectura de la
    // fuente; así "capítulos nuevos" no cuenta todo un manga recién agregado), y el idioma y el
    // scan de cada uno (un mismo número puede venir en varios idiomas o de varios scans: se
    // cuentan por número y el siguiente capítulo sigue con el mismo idioma y scan).
    r#"
ALTER TABLE chapters ADD COLUMN found_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chapters ADD COLUMN lang TEXT;
ALTER TABLE chapters ADD COLUMN scanlator TEXT;
CREATE INDEX chapters_number ON chapters(work_id, number);
"#,
    // 3: datos de los libros que se leen del archivo o de Open Library (saga y número, idioma,
    // sinopsis, año), de dónde vino (`origin`: catálogo, "inbox" o NULL = subido) y la URL de
    // origen (para no importar dos veces lo mismo de un catálogo). Y los catálogos de libros: los
    // que vienen de fábrica (`preset`) y los OPDS que agrega el admin.
    r#"
ALTER TABLE books ADD COLUMN series TEXT;
ALTER TABLE books ADD COLUMN series_index REAL;
ALTER TABLE books ADD COLUMN language TEXT;
ALTER TABLE books ADD COLUMN description TEXT;
ALTER TABLE books ADD COLUMN year INTEGER;
ALTER TABLE books ADD COLUMN source_url TEXT;
ALTER TABLE books ADD COLUMN origin TEXT;
CREATE INDEX books_source_url ON books(source_url);
CREATE INDEX books_series ON books(series, series_index);
CREATE TABLE catalogs (
    id       TEXT PRIMARY KEY,                  -- "archive", "gutenberg", "elejandria" u "o<n>"
    name     TEXT NOT NULL,
    kind     TEXT NOT NULL,                     -- "opds" | "archive" | "elejandria"
    url      TEXT NOT NULL,
    enabled  INTEGER NOT NULL DEFAULT 1,
    preset   INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL
);
"#,
];

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("no se pudo abrir {}", path.display()))?;
        Self::init(conn)
    }

    /// Base en memoria (tests).
    pub fn open_in_memory() -> anyhow::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql).with_context(|| format!("migración {} falló", i + 1))?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
            tracing::info!("migración de base de datos {} aplicada", i + 1);
        }
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Corre `f` con la conexión en un hilo bloqueante.
    pub async fn call<T, F>(&self, f: F) -> ApiResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> ApiResult<T> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap_or_else(|p| p.into_inner());
            f(&mut guard)
        })
        .await
        .map_err(ApiError::internal)?
    }
}

pub fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_reopens() {
        let path = std::env::temp_dir().join(format!("library-mig-{}.db", now_ms()));
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        let c = db.conn.lock().unwrap();
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        drop(c);
        drop(db);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
