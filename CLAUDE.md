# CLAUDE.md

Library (display name **Tomo** for now, see "Name"): a self-hosted reader for manga and books, like
TuMangaOnline but on your own server. Manga comes from scanlation sites and MangaDex through
**Suwayomi-Server** (a sidecar container that runs the Mihon/Tachiyomi community extensions); books
and comics (EPUB, PDF, CBZ, MOBI, FB2) are uploaded by the users. One web interface for PC and
phone (installable as a PWA), multi-user with login, on `MRTN-SERVER` (Docker) behind Tailscale.

## Where each decision lives

| Question | File |
|---|---|
| What the user asked for | `docs/spec.md` |
| Stages, what's missing, discarded alternatives | `docs/PLAN.md` |
| HTTP contract between UI and server | `docs/API.md` |
| How the interface has to look and move | `DESIGN.md` (ported from Ethereon / Mythos) |
| Deployment on the server, ports, Suwayomi | `deploy/README.md` |
| What each backend module does | header comment of each file in `backend/server/src/` |
| The app's name, tagline and theme color | `brand.json` (the only place) |

## Name

The name is provisional. **It lives only in `brand.json`** (`name`, taglines, theme color): the web
imports it (title, login, sidebar, PWA manifest, `index.html` via a Vite plugin) and the server
reads it with `include_str!` (logs, `/api/health`). Never write "Tomo" anywhere else. Internal
identifiers use the neutral id `library` and don't change with the name: the crate and binary
`library`, the container `library`, env vars `LIBRARY_*`, `localStorage` keys `library.*`, the
cookie `lib_session`.

## Structure

```
backend/                  Rust workspace (the only thing compiled into the container)
  server/                 `library` binary: axum, SQLite (rusqlite), auth, library, progress, books,
                          Suwayomi bridge (GraphQL), image proxy, library updater; serves the UI
apps/web/                 the interface (Svelte 5 + Vite + TS), also a PWA
  src/vendor/foliate-js/  EPUB/MOBI/FB2 renderer (vendored, MIT; see its README for the commit)
deploy/                   Dockerfile, docker-compose.yml (library + suwayomi), deploy.sh, README
docs/                     API, plan, spec
brand.json                name and colors
```

## Ports (MRTN-SERVER)

- `127.0.0.1:7710` the app (API + UI). Outside only via `tailscale serve --https=7710`
  → `https://mrtn-server.<tailnet>.ts.net:7710`.
- Suwayomi listens on `4567` **only inside the compose network** (no published port, it has no
  auth). Debugging: `docker compose exec library curl suwayomi:4567/api/graphql`.
- Don't use: 22, 53, 443, 1883, 3000, 4000, 4517, 5678, 6052, 6080, 7700, 7790, 8080, 8090, 8123,
  8443, 8765, 8766, 8787, 8788, 10200, 10300, 11987, 42069, 51515, 61208 (already taken).

## Commands

```powershell
# backend (needs a Suwayomi on LIBRARY_SUWAYOMI_URL for manga; books work without it)
cd backend; cargo run -p library      # http://127.0.0.1:7710 (uses ./data)
cd backend; cargo test
cd backend; cargo clippy --all-targets -- -D warnings
# interface
cd apps/web; npm run dev              # http://localhost:5173, proxies /api to :7710
cd apps/web; npm run check
# deploy (on the server)
~/Projects/Library/deploy/deploy.sh
```

For manga while developing on the PC, tunnel the server's Suwayomi:
`ssh -L 4567:<suwayomi container ip>:4567 MRTN-SERVER` and `LIBRARY_SUWAYOMI_URL=http://127.0.0.1:4567`.

Before calling a change done: `cargo test`, `cargo clippy` and `npm run check` with no errors or
warnings, and the change tried in the browser (desktop width and phone width, 390 px).

## Versions: commit, tag and push

Repo: https://github.com/MartinSchubert04/Library (public: no secrets; `.env` and `data/` stay out).

Every finished fix or feature (tests and check green) is published without waiting to be asked:
bump the version, commit, tag, and push `main` and the tag. Then deploy it on the server
(`deploy/deploy.sh`) and check `/api/health`.

- **Which number goes up** (semver, `MAJOR.MINOR.PATCH`):
  - `x.y.Z` (patch): fixes and small tweaks to something that already existed (a string, a width,
    a bug, a default value).
  - `x.Y.0` (minor): something new that can be used (an option, a screen, a feature), without
    breaking anything.
  - `X.0.0` (major): breaks something that already exists (the API without compatibility, the
    database without a migration, removing a feature). Ask before bumping it.
- **Where the version lives** (all the same): `backend/server/Cargo.toml`, `apps/web/package.json`,
  and those packages' entries in `backend/Cargo.lock` and `apps/web/package-lock.json` (careful
  with blind replacing: other dependencies may have the same number).
- **Commit**: `vX.Y.Z: <summary>` and in the body one line per change, in Spanish, ending with the
  version (see `git log`). Tag `vX.Y.Z` on that commit.
- Changes that are neither fix nor feature (docs, this file, CI) are committed and pushed without a tag.

## Rules

- **Pending tasks in `TODO.md`** (if it exists): read it at the start. Each task is deleted as soon
  as it's completed, in the same commit that resolves it; when none are left, the file is deleted.
- **Performance first.** The server is a home box with a 89 % full disk and other 20 containers:
  - Stream bodies (images, book files) instead of buffering them; no image re-encoding on the hot path.
  - Nothing grows without a limit: caches have a size cap, logs rotate, uploads have a max size.
  - In the UI: lazy views, `loading="lazy"` images with reserved `aspect-ratio`, virtualize or
    window long lists (chapter lists can have 1500 entries), preload only the next 3 pages in the
    reader, animations only on `transform` and `opacity`.
- **Suwayomi is an implementation detail.** The UI never talks to Suwayomi and never sees its ids
  except inside work ids (`s123`). Everything goes through `docs/API.md`. If Suwayomi is down, the
  books side keeps working and manga views show "The manga server isn't responding".
- **No `/api` route without a session**, except `GET /api/health` and `POST /api/auth/login`.
  Passwords never come back in any response; they're stored with Argon2id.
- **The image proxy is not an open proxy**: it only forwards to Suwayomi, only `manga/` and
  `extension/icon/` paths.
- **Themes only through variables** (same as Ethereon): colors in `apps/web/src/styles/themes.css`,
  never hardcoded in components. A new theme = one block there + one line in `lib/themes.svelte.ts`.
- **Animations only through `apps/web/src/lib/motion.ts`** (they respect "reduce motion").
- **Interface in Spanish by default, English as an option** (Settings → Appearance). No hardcoded
  visible text in a component: each string is a key in `apps/web/src/lib/i18n/es/<zone>.ts` with
  its translation in `en/<zone>.ts` (TypeScript flags any missing or extra one), used with
  `t("zone.key", { n })`. Neutral Rioplatense Spanish, short, with voseo ("Leé el capítulo 12",
  "Subí tus libros"); manga, manhwa, scan, webtoon, EPUB, PDF stay as they are.
- Docs in English; code comments in Spanish.
- **Rust disk space** (lesson from Ethereon and Mythos, whose `target/` filled C: in November 2026):
  - `--release` only inside Docker.
  - In a worktree (`.claude/worktrees/`) build with `CARGO_TARGET_DIR` pointing to the main repo's
    `backend/target`, don't create a new one. When the task is done, `git worktree remove` it.
  - When the tests of a task that compiled are done (before the push), `cargo clean` in
    `backend/`. Rebuilding takes a few minutes, but the disk doesn't fill up.
- **Testing doesn't touch real data.** Local tests use `./data` on the PC; never point a dev build
  at the server's database. Users created to test on the server are deleted afterwards.
