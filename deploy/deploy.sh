#!/usr/bin/env bash
# Despliegue en el server Linux (idempotente: se puede correr las veces que haga falta).
#   1. git pull
#   2. crea deploy/.env si no existe (con una contraseña de admin aleatoria que se muestra una vez)
#   3. docker compose up -d --build
#   4. espera a que el healthcheck dé "healthy"
#   5. publica la app en la tailnet con `tailscale serve` si todavía no está publicada
# Uso:  ./deploy/deploy.sh                          (desde cualquier carpeta)
#       LIBRARY_ADMIN_USER=martin ./deploy/deploy.sh  (nombre del primer admin; por defecto "admin")
set -euo pipefail

DEPLOY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$DEPLOY_DIR")"
PORT=7710
CONTAINER=library
HEALTH_TIMEOUT_S=180

say() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31mxx\033[0m %s\n' "$*" >&2; exit 1; }
as_root() { if [ "$(id -u)" -eq 0 ]; then "$@"; else sudo "$@"; fi; }

command -v docker >/dev/null || die "falta docker"
docker compose version >/dev/null 2>&1 || die "falta el plugin 'docker compose'"

# --- 1. código -----------------------------------------------------------------------------
say "Actualizando el repo"
git -C "$REPO_DIR" pull --ff-only

# --- 2. configuración ----------------------------------------------------------------------
cd "$DEPLOY_DIR"
mkdir -p data/library data/suwayomi
# Los dos contenedores corren con uid/gid 1000: las carpetas de datos tienen que ser suyas.
for d in data/library data/suwayomi; do
  if [ "$(stat -c '%u:%g' "$d")" != "1000:1000" ]; then
    say "Dando $d al uid 1000 (el usuario de los contenedores)"
    as_root chown -R 1000:1000 "$d"
  fi
done

if [ ! -f .env ]; then
  say "Creando deploy/.env"
  admin_user="${LIBRARY_ADMIN_USER:-admin}"
  admin_pass="$(head -c 18 /dev/urandom | base64 | tr -d '/+=\n')"
  sed -e "s/^LIBRARY_ADMIN_USER=.*/LIBRARY_ADMIN_USER=${admin_user}/" \
      -e "s/^LIBRARY_ADMIN_PASSWORD=.*/LIBRARY_ADMIN_PASSWORD=${admin_pass}/" \
      .env.example > .env
  chmod 600 .env
  if [ ! -f data/library/library.db ]; then
    printf '\n\033[1;32m  Primer admin:  %s  /  %s\033[0m\n' "$admin_user" "$admin_pass"
    printf '  (se muestra una sola vez; está en deploy/.env hasta que lo borres de ahí)\n\n'
  else
    warn "ya hay una base de datos: el admin de .env no se usa (los usuarios existentes siguen)"
  fi
fi

# --- 3. contenedores -----------------------------------------------------------------------
say "Construyendo y levantando los contenedores"
docker compose pull suwayomi --quiet || warn "no se pudo actualizar la imagen de Suwayomi (sigue la que hay)"
docker compose up -d --build

# --- 4. healthcheck ------------------------------------------------------------------------
say "Esperando el healthcheck"
deadline=$((SECONDS + HEALTH_TIMEOUT_S))
while :; do
  status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$CONTAINER" 2>/dev/null || echo missing)"
  case "$status" in
    healthy) break ;;
    unhealthy|exited|dead|missing)
      docker compose logs --tail 50 "$CONTAINER" >&2 || true
      die "el contenedor quedó '$status'" ;;
  esac
  [ "$SECONDS" -lt "$deadline" ] || { docker compose logs --tail 50 "$CONTAINER" >&2 || true; die "timeout esperando el healthcheck ($status)"; }
  sleep 2
done
say "La app responde en http://127.0.0.1:${PORT}"
docker image prune -f >/dev/null 2>&1 || true

# --- 5. tailscale serve --------------------------------------------------------------------
if command -v tailscale >/dev/null; then
  ts() { tailscale "$@" 2>/dev/null || as_root tailscale "$@"; }
  if ts serve status 2>/dev/null | grep -q ":${PORT}"; then
    say "tailscale serve ya publica el puerto ${PORT}"
  else
    say "Publicando en la tailnet (tailscale serve --https=${PORT})"
    ts serve --bg --https="${PORT}" "http://127.0.0.1:${PORT}"
  fi
  dns="$(ts status --json 2>/dev/null | sed -n 's/.*"DNSName": *"\([^"]*\)\.".*/\1/p' | head -n1)"
  if [ -n "$dns" ]; then say "Listo: https://${dns}:${PORT}"; fi
else
  warn "tailscale no está instalado: la app queda solo en http://127.0.0.1:${PORT}"
fi
