FROM node:24-bookworm-slim AS ui
WORKDIR /build
COPY package*.json ./
RUN npm ci
COPY scripts/build-ui.mjs scripts/
COPY THIRD_PARTY_NOTICES.md LICENSE ./
COPY web web
RUN npm run build

FROM rust:1-bookworm AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY desktop/profiles.json desktop/profiles.json
RUN cargo build --release --locked

FROM docker:28-cli AS dockercli
FROM debian:bookworm-slim
LABEL org.opencontainers.image.title="Bilskirnir" org.opencontainers.image.licenses="MIT"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl tini && rm -rf /var/lib/apt/lists/*
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=dockercli /usr/local/libexec/docker/cli-plugins/docker-buildx /usr/local/libexec/docker/cli-plugins/docker-buildx
COPY --from=backend /build/target/release/bilskirnir /usr/local/bin/bilskirnir
COPY --from=ui /build/web/dist /app/web
COPY desktop /app/desktop
COPY LICENSE /usr/share/doc/bilskirnir/LICENSE
ENV DATA_DIR=/data WEB_DIR=/app/web DESKTOP_CONTEXT=/app/desktop LISTEN_ADDR=0.0.0.0:8080
WORKDIR /app
EXPOSE 8080
HEALTHCHECK --interval=20s --timeout=3s CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/bilskirnir"]

