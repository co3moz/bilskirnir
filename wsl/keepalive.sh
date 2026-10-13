#!/usr/bin/env bash
# WSL stops a distribution shortly after its last Windows-started process exits, even when systemd
# services are running. The Windows launcher keeps this process open so Docker and Bilskirnir stay up.
exec sleep infinity
