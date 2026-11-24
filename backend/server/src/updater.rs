//! Búsqueda de capítulos nuevos: 30 s después de arrancar y después cada `LIBRARY_UPDATE_HOURS`,
//! le pide a la fuente los capítulos de cada manga que esté en la biblioteca de alguien (3 a la
//! vez) y los guarda en `chapters`/`manga_meta`. De ahí salen `unread` y `latestChapterAt` de
//! `GET /api/library`.
use std::time::Duration;

use futures_util::{stream, StreamExt};

use crate::{library::save_chapters, state::AppState, suwayomi::manga_id};

const PARALLEL: usize = 3;

pub async fn run(s: AppState, every: Duration) {
    tokio::time::sleep(Duration::from_secs(30)).await;
    loop {
        update_all(&s).await;
        tokio::time::sleep(every).await;
    }
}

/// Una pasada por todos los mangas de las bibliotecas. Devuelve cuántos se actualizaron.
pub async fn update_all(s: &AppState) -> usize {
    let works = match s
        .db
        .call(|c| {
            let mut st = c.prepare("SELECT DISTINCT work_id FROM library WHERE work_id LIKE 's%'")?;
            let v = st.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("novedades: no se pudo leer la biblioteca: {}", e.message);
            return 0;
        }
    };
    if works.is_empty() {
        return 0;
    }
    let t0 = std::time::Instant::now();
    let ok = stream::iter(works.clone())
        .map(|w| async move {
            let id = manga_id(&w)?;
            match s.suwa.fetch_chapters(id).await {
                Ok(chapters) => {
                    let work = w.clone();
                    match s.db.call(move |c| Ok(save_chapters(c, &work, &chapters)?)).await {
                        Ok(()) => Some(()),
                        Err(e) => {
                            tracing::warn!("novedades: {w}: {}", e.message);
                            None
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("novedades: {w}: {}", e.message);
                    None
                }
            }
        })
        .buffer_unordered(PARALLEL)
        .filter_map(|r| async move { r })
        .count()
        .await;
    tracing::info!("novedades: {ok}/{} mangas actualizados en {:.0?}", works.len(), t0.elapsed());
    ok
}
