# Third-party components

Bilskirnir uses the components below to provide its browser desktop, GPU rendering and Ubuntu environment. Bilskirnir's own source is covered by the [MIT license](LICENSE); these components keep their upstream licenses.

| Component | License information |
| --- | --- |
| [noVNC](https://github.com/novnc/noVNC) 1.5.0 | MPL-2.0. The generated frontend includes a notice, and the full npm license is served at `/novnc-LICENSE.txt`. Unmodified sources are available from the pinned npm package and repository tag; the source map also embeds the bundled source files. |
| [Oxanium](https://github.com/sevmeyer/oxanium) and [Geist Mono](https://github.com/vercel/geist-font) | SIL Open Font License 1.1. The interface bundles the variable fonts from the pinned Fontsource packages; their licenses are served at `/oxanium-LICENSE.txt` and `/geist-mono-LICENSE.txt`. |
| [Remix Icon](https://remixicon.com) | This project uses icons from Remix Icon, licensed under the [Remix Icon License v1.0](https://github.com/Remix-Design/RemixIcon/blob/master/License). The icons are interface elements only; the Bilskirnir mark is not derived from them. |
| [Monaco Editor](https://github.com/microsoft/monaco-editor) 0.57.0 | MIT. The MCP inspector's JSON editor, loaded only on that page. The notices of the code it bundles from other projects are served at `/monaco-third-party-notices.txt`; its own license is in `/third-party-licenses.txt`. |
| Frontend libraries such as React, Radix UI, shadcn/ui, TanStack Query and Table, Recharts, Sonner and Vaul | MIT, ISC, Apache-2.0 (class-variance-authority) or 0BSD (tslib). The build collects the license of every package in the bundle into `/third-party-licenses.txt`; `bun.lock` records the exact versions. |
| [VirtualGL](https://github.com/VirtualGL/virtualgl) 3.1.5 | Upstream licenses. The Debian package installs its license files under `/opt/VirtualGL`. |
| TigerVNC, XFCE, GNOME, Mesa, PulseAudio and other Ubuntu packages | Each package retains its own license. The desktop images include package documentation under `/usr/share/doc`. |
| Rust dependencies | Each dependency retains its own license. `Cargo.lock` records the exact versions used. |
| [Rust MCP SDK (rmcp)](https://github.com/modelcontextprotocol/rust-sdk) 3.5.1 | Apache-2.0. Provides the embedded MCP protocol and HTTP transport. |
