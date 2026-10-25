FROM oven/bun:1.4.3-slim AS ui
WORKDIR /build
COPY THIRD_PARTY_NOTICES.md LICENSE ./
WORKDIR /build/core/web
COPY core/web/package.json core/web/bun.lock core/web/bunfig.toml ./
RUN bun install --frozen-lockfile
COPY core/web .
RUN bun run build

FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /build

# The recipe names only the dependencies, with the crate's own version masked, so a source change
# or a version bump leaves it as it was and the dependencies come from the build cache.
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY core/manager/Cargo.toml core/manager/Cargo.toml
COPY core/manager/src core/manager/src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cooked
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json

FROM cooked AS backend
COPY Cargo.toml Cargo.lock ./
COPY core/manager/Cargo.toml core/manager/Cargo.toml
COPY core/manager/src core/manager/src
COPY core/manager/helpers core/manager/helpers
COPY images/desktop/profiles.json images/desktop/profiles.json
# A release build names its version, and the manager then downloads the desktop images published
# with it. Without it, the manager downloads each catalog image's reference.
ARG BILSKIRNIR_RELEASE
# Only the binary is left in this layer, so the build cache keeps one copy of the compiled
# dependencies, the one above, instead of one with every commit.
RUN cargo build --release --locked && mv target/release/bilskirnir /bilskirnir && rm -rf target

# The list of the Dependencies page, from this build's lockfiles, installed interface packages,
# Dockerfiles, Compose manifests and workflows, so it cannot fall behind them. It fails when a
# package that ships names no license. `cargo metadata` reads the crates the cooked stage holds and
# needs only the binary's entry point; the interface stage is mounted, not copied.
FROM cooked AS dependencies
COPY Cargo.toml Cargo.lock compose.yml compose.ghcr.yml ./
COPY core/manager/Cargo.toml core/manager/Dockerfile core/manager/dependencies.py core/manager/
COPY core/manager/src/main.rs core/manager/src/main.rs
COPY images/desktop/Dockerfile images/desktop/Dockerfile
COPY packaging/wsl/Dockerfile packaging/wsl/Dockerfile
COPY .github/workflows .github/workflows
RUN --mount=type=bind,from=ui,source=/build/core/web,target=/build/core/web \
    python3 core/manager/dependencies.py --strict --output /dependencies.json

FROM docker:28-cli AS dockercli
FROM debian:bookworm-slim
LABEL org.opencontainers.image.title="Bilskirnir" org.opencontainers.image.licenses="GPL-3.0-or-later"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl tini python3 && rm -rf /var/lib/apt/lists/*
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=dockercli /usr/local/libexec/docker/cli-plugins/docker-buildx /usr/local/libexec/docker/cli-plugins/docker-buildx
COPY --from=backend /bilskirnir /usr/local/bin/bilskirnir
COPY --from=ui /build/core/web/dist /app/web
COPY images/desktop /app/desktop
COPY core/bridge/mcp_bridge.py /app/mcp_bridge.py
COPY compose.ghcr.yml /app/install/compose.yml
COPY .env.example /app/install/.env.example
COPY deploy/Caddyfile /app/install/deploy/Caddyfile
COPY packaging/wsl/bilskirnir /app/install/bilskirnir
COPY packaging/wsl/bilskirnir-update.path /app/install/systemd/bilskirnir-update.path
COPY packaging/wsl/bilskirnir-update.service /app/install/systemd/bilskirnir-update.service
COPY packaging/wsl/bilskirnir-settings.path /app/install/systemd/bilskirnir-settings.path
COPY packaging/wsl/bilskirnir-settings.service /app/install/systemd/bilskirnir-settings.service
COPY LICENSE /usr/share/doc/bilskirnir/LICENSE
COPY --from=dependencies /dependencies.json /usr/share/doc/bilskirnir/dependencies.json
ENV DATA_DIR=/data WEB_DIR=/app/web DESKTOP_CONTEXT=/app/desktop LISTEN_ADDR=0.0.0.0:8080
WORKDIR /app
EXPOSE 8080
HEALTHCHECK --interval=20s --timeout=3s CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/bilskirnir"]
# The same release as a label, which `bilskirnir update` in the WSL package reads to download the
# desktop images this manager would. Declared last so it leaves the cache of the layers above alone.
ARG BILSKIRNIR_RELEASE
LABEL bilskirnir.release="${BILSKIRNIR_RELEASE}"

