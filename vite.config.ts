import { readdir, readFile } from "node:fs/promises"
import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"

const root = import.meta.dirname
// `bun run dev` proxies the API to a running manager, by default the one Compose publishes.
const manager = process.env.BILSKIRNIR_URL ?? "http://127.0.0.1:8080"
const novncNotice =
  "/*! Includes noVNC 1.5.0 (MPL-2.0). License: /novnc-LICENSE.txt; notices: /third-party-notices.txt. */"

/**
 * The license of every package whose code is in the bundle, collected while building so a new
 * dependency cannot ship without its notice.
 */
async function bundledLicenses(moduleIds: Iterable<string>) {
  const directories = new Set<string>()
  for (const id of moduleIds) {
    const path = id.replaceAll("\\", "/")
    const at = path.lastIndexOf("/node_modules/")
    if (at < 0) continue
    const rest = path.slice(at + "/node_modules/".length).split("/")
    directories.add(
      path.slice(0, at) +
        "/node_modules/" +
        rest.slice(0, rest[0].startsWith("@") ? 2 : 1).join("/")
    )
  }
  const sections: string[] = []
  for (const directory of [...directories].sort()) {
    const manifest = JSON.parse(await readFile(`${directory}/package.json`, "utf8"))
    const file = (await readdir(directory)).find((name) =>
      /^(licen[cs]e|copying)(\.(md|txt))?$/i.test(name)
    )
    const text = file
      ? await readFile(`${directory}/${file}`, "utf8")
      : `License: ${manifest.license}`
    sections.push(`${manifest.name} ${manifest.version} (${manifest.license})\n\n${text.trim()}\n`)
  }
  return sections.join(`\n${"-".repeat(72)}\n\n`)
}

// The bundle points at these licenses, so they are served next to it.
function notices(): Plugin {
  const files = {
    "novnc-LICENSE.txt": "node_modules/@novnc/novnc/LICENSE.txt",
    "geist-LICENSE.txt": "node_modules/@fontsource-variable/geist/LICENSE",
    "geist-mono-LICENSE.txt": "node_modules/@fontsource-variable/geist-mono/LICENSE",
    "third-party-notices.txt": "THIRD_PARTY_NOTICES.md",
    "LICENSE.txt": "LICENSE",
  }
  return {
    name: "bilskirnir-notices",
    apply: "build",
    async generateBundle(_options, bundle) {
      for (const [fileName, source] of Object.entries(files)) {
        this.emitFile({ type: "asset", fileName, source: await readFile(resolve(root, source)) })
      }
      const ids = Object.values(bundle).flatMap((chunk) =>
        chunk.type === "chunk" ? chunk.moduleIds : []
      )
      this.emitFile({
        type: "asset",
        fileName: "third-party-licenses.txt",
        source: await bundledLicenses(ids),
      })
    },
  }
}

export default defineConfig({
  root: resolve(root, "web"),
  plugins: [react(), tailwindcss(), notices()],
  resolve: {
    alias: { "@": resolve(root, "web/src") },
  },
  build: {
    outDir: resolve(root, "web/dist"),
    emptyOutDir: true,
    sourcemap: true,
    rolldownOptions: {
      output: {
        // Added after minification, which would otherwise drop the comment.
        postBanner: (chunk) =>
          chunk.moduleIds.some((id) => id.includes("@novnc/novnc")) ? novncNotice : "",
        // Libraries change less often than the app, so they get chunks a browser can keep. Only
        // libraries every page loads belong here; a group pulls its modules into the first page.
        codeSplitting: {
          groups: [
            { name: "react", test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/ },
            {
              name: "ui",
              test: /node_modules[\\/](radix-ui|@radix-ui|@floating-ui|vaul|sonner)[\\/]/,
            },
            { name: "data", test: /node_modules[\\/]@tanstack[\\/]/ },
          ],
        },
      },
    },
  },
  server: {
    proxy: {
      "/api": {
        target: manager,
        ws: true,
        changeOrigin: true,
        // The manager only accepts requests from its own origin; present proxied ones as such.
        configure: (proxy) => {
          proxy.on("proxyReq", (request) => request.setHeader("origin", manager))
          proxy.on("proxyReqWs", (request) => request.setHeader("origin", manager))
        },
      },
    },
  },
})
