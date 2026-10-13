# Bug audit: October 12, 2026

[Back to the README](../README.md)

This records an earlier review of Bilskirnir's container lifecycle, browser connections and recovery behavior. It explains the problems found, the changes made and the checks performed at the time. It is not a report of a new test run or a complete list of later changes.

## Desktops should remain usable while other work runs

A long image build could block unrelated Docker operations. The manager now locks the resource being changed rather than every operation at once. A build can continue while another environment stops or restarts.

Partial creation also lacked a recovery path. **Retry setup** now keeps an existing container's files and completes missing setup. If the container is gone, it recreates it from the recorded source image. Missing images or volumes stop the retry rather than replacing data. Network reconciliation handles environments independently, so one damaged network does not stop checks for every desktop.

## Closing a session should close its connections

Signing out or letting a session expire previously left VNC and audio connections open. They now follow session revocation. The container-side audio recorder also exits when its connection closes.

Browser media cleanup had several timing bugs: an old socket callback could close a newer session, and microphone permission granted after navigation could leave the device active. Connections now belong to a specific desktop session, and late microphone tracks are stopped. Sound and microphone controls release their own resources independently.

The browser also keeps received clipboard text until the clipboard dialog is opened. Dashboard refreshes apply successful responses even when another request fails, and a stale refresh cannot replace a newer sign-in session. Microphone options no longer appear available when installation-level audio is disabled.

## Deletion and snapshots must treat data predictably

Deletion could remove a container before discovering an unexpected network endpoint. The manager now checks ownership, network type and endpoints first, then uses the verified Docker IDs.

Mount validation rejects alternative paths that could bypass protected directories, including repeated separators, dot segments and system aliases. Valid application paths such as `/opt/game` remain allowed.

Images declaring a Docker `VOLUME` now require an explicit named-volume mount for that target. This prevents hidden anonymous volumes from making home files unexpectedly disappear from a snapshot.

Duplicate volume names return a conflict instead of reporting a new volume as created. Newly created volumes are checked for ownership. Missing Docker images are disabled in the image picker rather than offered for new environments.

## Job output should stay useful without exhausting memory

Docker command output and build-log lines were unbounded. Binary output could also turn a successful build into a logging failure. Output is now bounded, split UTF-8 is preserved, invalid bytes are replaced and logs keep their last 256 KiB. Timeouts include waiting for the process to exit.

Build preparation failures could leave scripts or temporary image tags behind. Cleanup now runs after success, failure or panic; startup also cleans temporary resources from earlier build jobs.

The job list previously loaded all historical logs. It now returns the latest 100 entries without their log bodies; opening an individual job loads the detail. Job dialogs poll while open, so users see status changes and the image result without reopening them.

Database failures are reported as server errors rather than mistaken for missing records.

## Smaller correctness fixes

Profile build targets are separate from desktop session names, allowing more than one image variant for a session. Name limits count Unicode characters rather than bytes. Snapshot-tree traversal is iterative, including a regression case with 20,000 layers.

The manager also validates Host headers when sign-in is disabled. Localhost and private LAN IPs are accepted; DNS names must be configured. Origin and CSRF checks remain active.

## What was checked

At the time of this audit, Rust's 20 unit tests and the frontend's 11 tests passed, along with formatting and Clippy. These are historical counts; use the [development commands](DEVELOPMENT.md#check-a-change) for the current suite.

The real WSL/Docker acceptance suite passed with XFCE and GNOME GPU desktops. It checked VNC, screenshots, audio, sign-out cleanup, offline image builds, live internet switching, snapshot ancestry and volume exclusion, concurrent operations, deletion preflight and recovery. It also exercised an actual OCI startup failure, implicit image volumes, binary logs and cleanup after both successful and failed builds.

A browser check confirmed that an open build-log dialog moved from **Running** to **Completed** and displayed its output and image result. Temporary test resources were removed. The local manager was rebuilt without replacing its existing data volume or desktops.

## What this did not prove

The audit did not run Unity Editor or Player, verify Vulkan or other GPU vendors, or use a physical browser microphone. Hardware OpenGL and a working virtual microphone channel still need application and device testing.

Recovery cannot restore unsaved changes from a deleted container. Direct Docker changes can also race with manager operations; ownership checks and immutable IDs reduce that risk but do not make separate Docker commands a transaction.
