# Bilskirnir contributor instructions

## Project structure

- `src/`: Rust/Axum API, authentication, Docker operations, SQLite metadata, jobs and WebSocket proxies. `src/tests.rs` holds backend unit tests.
- `web/`: plain JavaScript UI, noVNC integration, audio, CSS, SVG assets and shared helpers. `web/dist/` is generated and ignored.
- `desktop/`: Ubuntu 24.04 Docker stages, XFCE/GNOME sessions, GPU wrappers, health checks and the bundled profile catalog.
- `deploy/`: optional Caddy configuration for LAN HTTPS.
- `compose.yml`: source builds and development installation. `compose.ghcr.yml`: installation from published GHCR images.
- `.github/workflows/`: checks and GHCR publishing. The root `Dockerfile` builds the manager and bundles installation files under `/app/install`.
- `scripts/`: frontend build utilities. `tests/`: JavaScript tests and WSL/Docker acceptance tests.
- `docs/`: design rules, verification notes and screenshots. `README.md` and `.env.example` describe installation and configuration.

## Implementation rules

- Use Bilskirnir as the product name. Write UI copy, documentation, code comments and commit messages in English. Keep UI copy concise and useful.
- Follow `docs/DESIGN.md` for interface work. Prefer existing CSS tokens and SVG assets; preserve keyboard focus and narrow-screen layouts.
- Preserve compatibility labels `io.wsl-desktop.*`, `wsld-*` resource IDs and existing session/API identifiers. Do not rename them without a migration.
- This manages containers through the host Docker socket. Desktop containers must not receive that socket. Snapshots are image layers; named volumes and running memory are excluded.
- Preserve user containers, image layers and named volumes during development. Do not run daemon-wide pruning, delete user resources or replace their data volumes. Acceptance tests may clean only their own verified resources.
- Keep image references aligned across the profile catalog, Rust default, Compose manifests, environment example and publishing workflow. GHCR desktop images target `linux/amd64` because the bundled VirtualGL package is amd64.
- Never commit credentials, `.env`, SQLite data, generated bundles or dependencies. Publish with `GITHUB_TOKEN`; do not add personal tokens to workflows.

## Verification

- Frontend logic: `npm ci`, `npm test`, `npm run build`.
- Backend: `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`.
- Compose: validate both manifests with `.env.example`, including `images` and `https` profiles. Building from source must remain possible.
- Visual changes: inspect the live desktop and a narrow viewport. Save screenshots using the actual page width, then open the saved file to verify it. Restore temporary viewport overrides.
- Docker/GPU lifecycle changes: run `python3 tests/integration.py` in WSL after building both desktop profiles and the manager. Ordinary hosted CI has no WSL GPU; do not claim it verifies GPU, Unity or physical microphone hardware.
- Add tests for meaningful behavior or regressions, not for cosmetic changes or assertions that only repeat the implementation.

## Git and commits

- Inspect status and diffs before editing. Preserve unrelated user changes; never silently discard them.
- Work on the existing branch unless a different branch is requested. Keep each commit focused and reviewable.
- Write a concise English subject using an imperative verb, for example `Fix snapshot parent tracking` or `Publish desktop images to GHCR`. Explain non-obvious tradeoffs in the body when needed.
- Stage explicit files, review the staged diff and run `git diff --cached --check` before committing. Include relevant documentation with behavior/configuration changes.
- Run checks appropriate to the change before committing. Report what passed and any remaining limitations accurately.
- Do not rewrite published history, force-push or create release tags without explicit authorization. Push only when the current task or established user instruction authorizes it.
- Workflow actions are pinned to verified commit SHAs with version comments. Publishing occurs from `master`, `v*` tags or a manual run on those refs; feature branches must not overwrite published stable tags.
