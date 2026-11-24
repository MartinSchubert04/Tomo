# syntax=docker/dockerfile:1
# Imagen de la app en tres etapas: build de la UI (Node), build del backend (Rust, con las
# dependencias cacheadas en su propia capa) y runtime mínimo.
# Contexto = raíz del repo:  docker build -f deploy/Dockerfile .

# ---------- 1. UI ----------
FROM node:22-alpine AS web
WORKDIR /repo/apps/web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY brand.json /repo/brand.json
COPY apps/web/ ./
RUN npm run build

# ---------- 2. backend ----------
FROM rust:1-trixie AS backend
WORKDIR /repo/backend
# Primero solo los manifiestos con fuentes vacías: esta capa (todas las dependencias compiladas)
# se reusa mientras no cambien Cargo.toml/Cargo.lock.
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/server/Cargo.toml server/Cargo.toml
COPY brand.json /repo/brand.json
RUN mkdir -p server/src \
 && echo 'fn main() {}' > server/src/main.rs \
 && touch server/src/lib.rs \
 && cargo build --release --locked -p library \
 && rm -rf server/src
COPY backend/server/src server/src
# `touch` + borrar las huellas del crate propio: COPY conserva fechas viejas y cargo podría creer
# que el binario vacío de arriba sigue vigente.
RUN touch server/src/main.rs server/src/lib.rs \
 && rm -rf target/release/.fingerprint/library-* \
 && cargo build --release --locked -p library \
 && cp target/release/library /library

# ---------- 3. runtime ----------
FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 1000 library \
 && useradd --system --uid 1000 --gid 1000 --home-dir /data --no-create-home library \
 && mkdir -p /data /app \
 && chown library:library /data
COPY --from=backend /library /usr/local/bin/library
COPY --from=web /repo/apps/web/dist /app/web
ENV LIBRARY_BIND=0.0.0.0:7710 \
    LIBRARY_DATA=/data \
    LIBRARY_WEB_DIR=/app/web \
    RUST_LOG=info
USER library
WORKDIR /data
VOLUME /data
EXPOSE 7710
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD ["library", "health"]
ENTRYPOINT ["/usr/bin/tini", "--", "library"]
