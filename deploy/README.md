# Deploying

Two containers from `deploy/docker-compose.yml`:

| Container | What | Port |
|---|---|---|
| `library` | API + web interface (Rust binary + the built UI) | `127.0.0.1:7710`, outside via `tailscale serve --https=7710` |
| `library-suwayomi` | Suwayomi-Server: runs the manga extensions (scrapers) | `4567` inside the compose network only (it has no auth) |

Data lives next to the compose file: `deploy/data/library/` (SQLite `library.db`, uploaded books
under `books/`, covers under `covers/`) and `deploy/data/suwayomi/` (Suwayomi's database,
installed extensions, its thumbnail cache). Both owned by uid 1000. Back them up with Kopia like
the rest of the server.

## First time and updates

```bash
mkdir -p ~/Projects && cd ~/Projects
git clone https://github.com/MartinSchubert04/Library.git
LIBRARY_ADMIN_USER=martin ./Library/deploy/deploy.sh   # the name is optional (default "admin")
```

`deploy.sh` (run it again for every update):

1. `git pull --ff-only`.
2. Creates `deploy/data/*` and gives them to uid 1000.
3. If `deploy/.env` doesn't exist, creates it from `.env.example` with a **random admin password
   shown only once**. An existing `.env` is never touched.
4. Pulls the latest Suwayomi image and runs `docker compose up -d --build`.
5. Waits for the healthcheck (`library health` → `GET /api/health`).
6. Publishes the app on the tailnet with `tailscale serve --bg --https=7710` (once).

## Sources (extensions)

On start the app adds `LIBRARY_EXTENSION_REPO` (Keiyoushi by default) to Suwayomi if it has no
repo. Installing sources is done from the app: **Admin → Fuentes** (filter "Español", Install).
Suwayomi itself is never exposed; for debugging:

```bash
docker compose exec library sh -c 'wget -qO- --post-data "{\"query\":\"{aboutServer{version}}\"}" \
  --header content-type:application/json http://suwayomi:4567/api/graphql'
```

## Resources

- `library`: ~20 MB RAM idle, capped at 512 MB.
- `library-suwayomi`: JVM with `-Xmx768m`, capped at 1.28 GB. Its image cache lives in
  `data/suwayomi`; the app asks it to keep it small (see `backend/server/src/suwayomi.rs`).

## Ports taken on MRTN-SERVER

22, 53, 443, 1883, 3000, 4000, 4517, 5678, 6052, 6080, 7700, 7790, 8080, 8090, 8123, 8443, 8765,
8766, 8787, 8788, 10200, 10300, 11987, 42069, 51515, 61208. This app uses 7710 only.
