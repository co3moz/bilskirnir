# Bilskirnir contributor instructions

This file is for coding agents and contributors making changes. Start with `README.md` for the product overview; `docs/USAGE.md`, `docs/INSTALLATION.md` and `docs/DEVELOPMENT.md` are the guides for people using, maintaining or developing Bilskirnir.

## Project structure

- `src/`: Rust/Axum API, authentication, Docker operations, SQLite metadata, jobs and WebSocket proxies. `src/tests.rs` holds backend unit tests.
- `src/mcp.rs`: embedded Streamable HTTP MCP service using the official Rust SDK. Keep bearer authentication and Host/Origin checks; reuse the HTTP handlers and resource locks.
- `scripts/mcp_bridge.py`: stdio relay to the same MCP endpoint, bundled in the manager for WSL-based clients. Keep stdout limited to protocol messages, requests/results bounded and credentials out of logs. Run `python3 -m unittest discover -s tests -p 'test_*.py'` for bridge changes.
- `src/exec.rs`, `src/files.rs`, `src/read_file.py` and `src/desktop_input.rs`: script jobs, bounded file access and shared VNC mouse/keyboard input. Root execution must be explicit and scripts run with stdin closed; file operations stay under desktop-user permissions and must not follow symlinks. Held keys and buttons are released when an input request ends.
- `src/apps.rs`, `src/app_runner.py` and `src/app_control.py`: background applications supervised inside the desktop as the desktop user, with bounded logs and state under the desktop's tmpfs. `src/windows.rs` and `src/capture.rs`: X11 window management and screenshots through `xdotool`, `wmctrl` and `desktop-screenshot`; keep screenshots at desktop resolution by default so coordinates match input. `src/recordings.rs`, `src/recorder.py` and `src/export_artifacts.py`: frame-sequence recordings and tar.gz export; exports follow the upload destination rules. `src/waits.rs`: condition polling without mutations. `src/audio.rs` and `src/audio_tools.py`: WAV capture and playback through the desktop's PulseAudio. `src/terminal.rs` with `terminal_runner.py`/`terminal_control.py`: pseudo-terminal sessions with bounded output; root shells must be explicit. `src/gamepad.rs` with `gamepad_daemon.py`/`gamepad_control.py`: a uinput controller created inside the desktop; `/dev/uinput` is attached only when an environment opts in. `src/inspect_ui.py`: AT-SPI reads only.
- `src/uploads.rs` and `src/upload_file.py`: bounded file staging and atomic delivery as the desktop user. Upload destinations must stay in Downloads or writable named-volume mounts; never follow path symlinks or grant root writes.
- `web/`: plain JavaScript UI, noVNC integration, audio, CSS, SVG assets and shared helpers. `web/dist/` is generated and ignored.
- `web/uploads.mjs`: sequential upload queue, progress, cancellation and session-expiry handling.
- `desktop/`: Ubuntu 24.04 Docker stages, XFCE/GNOME sessions, GPU wrappers, health checks and the bundled profile catalog.
- `deploy/`: optional Caddy configuration for LAN HTTPS.
- `compose.yml`: source builds and development installation. `compose.ghcr.yml`: installation from published GHCR images.
- `.github/workflows/`: checks and GHCR publishing. The root `Dockerfile` builds the manager and bundles installation files under `/app/install`.
- `scripts/`: frontend build utilities. `tests/`: JavaScript tests and WSL/Docker acceptance tests.
- `docs/`: user, installation and development guides, design rules, historical verification notes and screenshots. `README.md` provides the product overview and quick start; `.env.example` lists installation settings.

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
