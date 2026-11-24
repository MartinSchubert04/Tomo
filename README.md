# Library

Self-hosted reader for **manga** and **books**, like TuMangaOnline but on your own server.
The display name is provisional and lives only in [`brand.json`](brand.json).

- **Manga** from 1700+ sources (scanlation sites in Spanish, MangaDex, …) through
  [Suwayomi-Server](https://github.com/Suwayomi/Suwayomi-Server) and the community extensions.
  Global search, explore by source, library with new-chapter badges, read marks and history.
- **Reader** in cascade (webtoon) or paged, right-to-left or left-to-right, preloading, remembers
  the page, keyboard and touch.
- **Books**: upload EPUB, PDF, CBZ, MOBI, AZW3, FB2 and read them in the browser
  ([foliate-js](https://github.com/johnfactotum/foliate-js), [pdf.js](https://mozilla.github.io/pdf.js/)).
- Multi-user with login, Spanish / English, six themes with palettes, installable on the phone (PWA).

Stack: Rust (axum, SQLite) + Svelte 5, two Docker containers. See [`CLAUDE.md`](CLAUDE.md),
[`docs/API.md`](docs/API.md), [`DESIGN.md`](DESIGN.md) and [`deploy/README.md`](deploy/README.md).
