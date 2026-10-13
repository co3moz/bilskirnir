# Third-party components

Bilskirnir uses the components below to provide its browser desktop, GPU rendering and Ubuntu environment. Bilskirnir's own source is covered by the [MIT license](LICENSE); these components keep their upstream licenses.

| Component | License information |
| --- | --- |
| [noVNC](https://github.com/novnc/noVNC) 1.5.0 | MPL-2.0. The generated frontend includes a notice, and the full npm license is served at `/novnc-LICENSE.txt`. Unmodified sources are available from the pinned npm package and repository tag; the source map also embeds the bundled source files. |
| [VirtualGL](https://github.com/VirtualGL/virtualgl) 3.1.4 | Upstream licenses. The Debian package installs its license files under `/opt/VirtualGL`. |
| TigerVNC, XFCE, GNOME, Mesa, PulseAudio and other Ubuntu packages | Each package retains its own license. The desktop images include package documentation under `/usr/share/doc`. |
| Rust dependencies | Each dependency retains its own license. `Cargo.lock` records the exact versions used. |
| [Rust MCP SDK (rmcp)](https://github.com/modelcontextprotocol/rust-sdk) 3.5.1 | Apache-2.0. Provides the embedded MCP protocol and HTTP transport. |
