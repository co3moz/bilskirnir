# Bilskirnir bug hunt and completed fixes — 2026-10-12

Verified through code review, real WSL Docker Engine acceptance tests and localhost browser checks.

| Finding | Fix |
| --- | --- |
| A long image build blocked all Docker mutations | Operations use environment, profile or volume locks. Unrelated resources progress independently. |
| Sign-out and session expiry did not revoke active WebSockets | VNC and PCM connections follow session revocation and expiry. Signing out closes them immediately. |
| Container-side audio recording could outlive its CLI connection | The output supervisor terminates `parec` when stdin reaches EOF. |
| Network errors could be discovered after the container layer was deleted | Ownership, network type and all endpoints are checked before deletion. Commands use verified Docker IDs. |
| Partial setup had no retry path | **Retry setup** preserves an existing container, completes missing networks/connections and retries startup. Missing containers use the saved immutable source image. Missing images or volumes stop recovery. |
| One invalid network stopped the entire reconciliation pass | Each environment is handled independently. Resource locks prevent stale metadata writes. |
| Alternative mount paths could bypass protected directories | Canonical absolute paths are required. Duplicate separators, `.`/`..`, system paths, aliases such as `/var/run` and `/lib64`, and read-only desktop home/configuration mounts are rejected. `/opt/game` is allowed. |
| Image-declared `VOLUME` paths could create hidden anonymous volumes | Each declared target requires an explicit named-volume mount, keeping snapshot exclusions visible. |
| CLI output and line length were unbounded; binary output could fail a successful build | CLI output is bounded. Build logs use chunks, preserve split UTF-8 and replace invalid bytes. Logs retain the last 256 KiB. The timeout also covers waiting for process exit. |
| Preparation failures could leave scripts or temporary image tags behind | Shared finalization cleans resources after success, failure or panic. Startup also cleans temporary resources from previous build jobs; scripts are not replayed automatically. |
| Listing jobs loaded every historical log into memory | SQLite removes the log field in the query and returns the latest 100 jobs. The individual job endpoint provides its detailed log. |
| Database errors were reported as missing records | Missing records return 404; SQLite operation errors return 500. |
| Old audio socket callbacks could close a newer connection | Desktop and channel generations ensure callbacks affect only their own connection. |
| Microphone permission resolving after navigation could leave the device active | Late tracks are stopped immediately. Source, mute/worklet nodes and idle audio contexts are released. Audio and microphone can be stopped independently. |
| Remote clipboard text was lost while the dialog was closed | The latest clipboard text is retained for the desktop session and shown when the dialog opens. |
| One failed API request stopped the whole dashboard refresh | Independent successful responses are applied separately. Parallel 401 responses do not recreate the sign-in form, and stale refreshes cannot overwrite a newer session. |
| The job log stopped updating while its dialog was open | Two-second polling updates the log and status. Completed image jobs offer an environment creation action. |
| Images removed from Docker remained selectable | Records are compared against immutable Engine image IDs; missing images are disabled. |
| Creating an existing volume reported success | Duplicate names return 409. New volume ownership is verified after creation. |
| Profile build targets were coupled to desktop session names | The catalog has an independent `build_target` field, allowing multiple image variants of the same desktop session. |
| Microphone support appeared available when audio was disabled | Installation-level microphone support depends on audio; form checkboxes stay consistent. |
| The 100-character name limit counted bytes | Environment, image and snapshot names are validated by Unicode character count. |
| Deep snapshot histories could overflow the JavaScript call stack | Tree traversal is iterative; the 20,000-layer regression test passed. |
| Arbitrary Host headers were accepted with sign-in disabled | localhost and private LAN IPs are accepted. DNS names require `ALLOWED_HOSTS` or Compose `DESKTOP_HOST`. Origin and CSRF validation remains active. |

## Validation

- `cargo test`: **20/20 passed**.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo fmt --check`: passed.
- `npm test`: **11/11 passed** (6 media lifecycle tests and 5 UI/helper tests).
- The manager image was rebuilt in Docker.
- `python3 tests/integration.py`: **ALL DOCKER ACCEPTANCE CHECKS PASSED**. Coverage includes XFCE/GNOME GPU desktops, VNC/JPEG/audio, recorder cleanup after sign-out, offline builds, live internet switching, layered snapshots and volume exclusion, concurrent operations, deletion preflight, missing-volume/container recovery, isolated network reconciliation, repair of a real OCI start failure in the same container, implicit VOLUME rejection, binary logs and successful/failed build cleanup.
- A browser script build transitioned from **Running** to **Completed** without closing its dialog. `UI_LOG_BEGIN`, `UI_LOG_END` and the image result appeared automatically. Its temporary image and library record were removed; job history was retained.
- The localhost preview was updated by replacing the manager while preserving existing desktops and its data volume. Acceptance-test resources were cleaned up.
- Outdated screenshots from the previous interface were removed during the branding audit. [Current Bilskirnir interface](screenshots/bilskirnir.png).

## Scope and limitations

Unity Editor/Player itself, Vulkan acceleration, other GPU vendors and physical browser microphone hardware have not been verified. Hardware OpenGL and the virtual microphone channel do not replace application/device testing. Retry cannot restore an unsnapshotted layer from a deleted container; it preserves existing containers, image references and volumes during recovery. External Docker mutations cannot be locked as one transaction; preflight checks and immutable IDs reduce the associated risk.
