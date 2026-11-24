# Plan

## Decisions

- **Manga sources through Suwayomi-Server** (sidecar container). It runs the community
  Mihon/Tachiyomi extensions (Keiyoushi repo: ~1700 sources, 133 in Spanish, MangaDex included) and
  keeps them working as sites change. Writing our own scrapers would mean maintaining every site
  ourselves (HTML changes, Cloudflare, hotlink protection). Our server talks to it over GraphQL
  (`/api/graphql`) and proxies its images; the UI never sees it.
  - Discarded: native scrapers in Rust (maintenance), Suwayomi's own WebUI (no books, not our design).
  - Cost: ~600 MB RAM (JVM). Capped with `-Xmx` in compose.
- **Backend in Rust** (axum + rusqlite), same stack as Mythos: one small static binary, low RAM,
  streams everything. SQLite is plenty for a handful of users.
- **Interface in Svelte 5 + Vite** with Mythos' design system (themes, palettes, components,
  motion, i18n). PWA so it installs on the phone.
- **Books**: foliate-js (the engine of Foliate and Readest) for EPUB/MOBI/AZW3/FB2, pdf.js for PDF,
  our manga reader for CBZ. Stored on disk under `data/books/`, metadata in SQLite.
- **Read state is ours, not Suwayomi's**: per user, in our SQLite. Suwayomi's library/read flags are
  not used (they're global, single-user).

## Stages

1. **v0.1** — skeleton, auth, Suwayomi bridge (sources, browse, global search, detail, pages),
   manga reader (cascade + paged), library + progress + history, books (upload, shelf, EPUB/PDF/CBZ
   readers), admin (users, extensions), themes, deploy on MRTN-SERVER.
2. **v0.2** — library updates (new chapter badges), PWA offline shell, reader polish (double page,
   zoom), tracking of reading stats.
3. **v0.3** — books beyond uploads: free and legal catalogs (Elejandría, textos.info, Internet
   Archive public domain, optional Gutenberg and any OPDS), inbox folder, series and Open Library
   metadata.
4. Later — downloads for offline reading on the phone, FlareSolverr for Cloudflare sources,
   OPDS feed for e-readers, desktop/Android shells like Mythos.
