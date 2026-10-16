FROM oven/bun:1.4.3-slim AS ui
WORKDIR /build
COPY package.json bun.lock bunfig.toml ./
RUN bun install --frozen-lockfile
COPY tsconfig*.json vite.config.ts ./
COPY THIRD_PARTY_NOTICES.md LICENSE ./
COPY web web
RUN bun run build

FROM rust:1-bookworm AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY desktop/profiles.json desktop/profiles.json
# A release build names its version, and the manager then downloads the desktop images published
# with it. Without it, the manager downloads each profile's catalog reference.
ARG BILSKIRNIR_RELEASE
RUN cargo build --release --locked

FROM docker:28-cli AS dockercli
FROM debian:bookworm-slim
LABEL org.opencontainers.image.title="Bilskirnir" org.opencontainers.image.licenses="MIT"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl tini python3 && rm -rf /var/lib/apt/lists/*
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=dockercli /usr/local/libexec/docker/cli-plugins/docker-buildx /usr/local/libexec/docker/cli-plugins/docker-buildx
COPY --from=backend /build/target/release/bilskirnir /usr/local/bin/bilskirnir
COPY --from=ui /build/web/dist /app/web
COPY desktop /app/desktop
COPY scripts/mcp_bridge.py /app/mcp_bridge.py
COPY compose.ghcr.yml /app/install/compose.yml
COPY .env.example /app/install/.env.example
COPY deploy/Caddyfile /app/install/deploy/Caddyfile
COPY wsl/bilskirnir /app/install/bilskirnir
COPY LICENSE /usr/share/doc/bilskirnir/LICENSE
ENV DATA_DIR=/data WEB_DIR=/app/web DESKTOP_CONTEXT=/app/desktop LISTEN_ADDR=0.0.0.0:8080
WORKDIR /app
EXPOSE 8080
HEALTHCHECK --interval=20s --timeout=3s CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/bilskirnir"]
# The same release as a label, which `bilskirnir update` in the WSL package reads to download the
# desktop images this manager would. Declared last so it leaves the cache of the layers above alone.
ARG BILSKIRNIR_RELEASE
LABEL io.wsl-desktop.release="${BILSKIRNIR_RELEASE}"

