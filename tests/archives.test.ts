/* eslint-disable @typescript-eslint/no-explicit-any -- the fakes stand in for XMLHttpRequest */
import { test } from "bun:test"
import assert from "node:assert/strict"
import { gzipSync } from "node:zlib"

import {
  ArchiveImport,
  archiveName,
  archiveUrl,
  parseArchiveInfo,
  readArchiveInfo,
} from "../web/src/lib/archives"

/** One USTAR entry as the manager writes it, padded to whole blocks. */
function entry(name: string, data: Uint8Array) {
  const header = new Uint8Array(512)
  const put = (offset: number, text: string) => header.set(new TextEncoder().encode(text), offset)
  const octal = (offset: number, width: number, value: number) =>
    put(offset, value.toString(8).padStart(width - 1, "0") + "\0")
  put(0, name)
  octal(100, 8, 0o644)
  octal(108, 8, 0)
  octal(116, 8, 0)
  octal(124, 12, data.length)
  octal(136, 12, 0)
  put(156, "0")
  put(257, "ustar\0")
  put(263, "00")
  put(148, "        ")
  const sum = header.reduce((total, byte) => total + byte, 0)
  put(148, sum.toString(8).padStart(6, "0") + "\0 ")
  const out = new Uint8Array(512 + Math.ceil(data.length / 512) * 512)
  out.set(header)
  out.set(data, 512)
  return out
}

const json = (value: unknown) => new TextEncoder().encode(JSON.stringify(value))
const info = {
  format: 1,
  name: "Unity workspace",
  description: "Editor and tools",
  kind: "snapshot",
  desktop: { catalog_id: "ubuntu-xfce", title: "Ubuntu 24.04 · XFCE", session: "xfce" },
}

/** An exported archive: the metadata entry, then a large layer that does not compress. */
function exported(metadata: unknown = info) {
  let seed = 2463534242
  const layer = new Uint8Array(3 << 20).map(() => {
    seed ^= seed << 13
    seed ^= seed >>> 17
    seed ^= seed << 5
    return seed & 0xff
  })
  const parts = [entry("bilskirnir.json", json(metadata)), entry("layer", layer)]
  const out = new Uint8Array(parts[0].length + parts[1].length)
  out.set(parts[0])
  out.set(parts[1], parts[0].length)
  return out
}

test("archive names and download URLs", () => {
  assert.equal(archiveName("Unity workspace.tar.gz"), "Unity workspace")
  assert.equal(archiveName("saved.TGZ"), "saved")
  assert.equal(archiveName("plain.tar"), "plain")
  assert.equal(archiveName("x".repeat(150) + ".tar"), "x".repeat(100))
  assert.equal(archiveUrl("a/b"), "/api/images/a%2Fb/archive")
})

test("the metadata entry is read only when it opens the archive", () => {
  assert.deepEqual(parseArchiveInfo(entry("bilskirnir.json", json(info))), info)
  assert.equal(parseArchiveInfo(entry("manifest.json", json(info))), null)
  assert.equal(parseArchiveInfo(entry("bilskirnir.json", json({ ...info, format: 2 }))), null)
  assert.equal(parseArchiveInfo(entry("bilskirnir.json", new TextEncoder().encode("{"))), null)
  assert.equal(parseArchiveInfo(new Uint8Array(100)), null)
})

test("a build's metadata with its megabyte script is read in full", async () => {
  const built = { ...info, kind: "build", build: { script: "echo tools\n".repeat(100_000) } }
  const tar = exported(built)
  assert.deepEqual(await readArchiveInfo(new Blob([gzipSync(tar)])), built)
  assert.deepEqual(await readArchiveInfo(new Blob([tar])), built)
})

test("the metadata comes from the start of a gzip or plain archive", async () => {
  const tar = exported()
  const gzip = gzipSync(tar, { level: 1 })
  assert.ok(gzip.length > 1 << 20, "the start is only a slice of the file")
  assert.deepEqual(await readArchiveInfo(new Blob([gzip])), info)
  assert.deepEqual(await readArchiveInfo(new Blob([tar])), info)
  assert.equal(await readArchiveInfo(new Blob([new Uint8Array(2048)])), null)
  assert.equal(await readArchiveInfo(new Blob([gzip.subarray(0, 40)])), null)
})

function fixture(path?: string) {
  const requests: any[] = []
  let unauthorized = 0
  const upload = new ArchiveImport({
    path,
    createRequest: () => {
      const request: any = {
        upload: {},
        headers: {},
        open(_method: string, url: string) {
          request.url = new URL(url, "http://localhost")
        },
        setRequestHeader(key: string, value: string) {
          request.headers[key] = value
        },
        send(file: unknown) {
          request.file = file
        },
        abort() {
          request.onabort()
        },
        complete(status: number, result: unknown) {
          request.status = status
          request.responseText = JSON.stringify(result)
          request.onload()
        },
      }
      requests.push(request)
      return request
    },
    onUnauthorized: () => unauthorized++,
  })
  return {
    upload,
    requests,
    get unauthorized() {
      return unauthorized
    },
  }
}
const file = (size: number) => ({ size }) as any as Blob
const details = { name: "Saved & shared", description: "Line one\nline two" }

test("an archive uploads with its details and resolves with the job", async () => {
  const { upload, requests } = fixture()
  const done = upload.start(file(10), details, 100)
  const request = requests[0]
  assert.equal(request.url.pathname, "/api/images/import")
  assert.equal(request.url.searchParams.get("size"), "10")
  assert.equal(request.url.searchParams.get("name"), details.name)
  assert.equal(request.url.searchParams.get("description"), details.description)
  assert.equal(request.headers["X-Bilskirnir-Request"], "1")
  assert.equal(request.headers["Content-Type"], "application/octet-stream")
  request.upload.onprogress({ loaded: 4 })
  assert.equal(upload.loaded, 4)
  assert.ok(upload.running)
  request.upload.onload()
  assert.equal(upload.status, "checking")
  request.complete(200, { job_id: "job-1" })
  assert.equal(await done, "job-1")
  assert.equal(upload.status, "started")
  assert.ok(!upload.running)
})

test("a volume archive goes to the volume import with the new volume's name", async () => {
  const { upload, requests } = fixture("volumes/import")
  const done = upload.start(file(12), { name: "unity-projects" }, 100)
  const request = requests[0]
  assert.equal(request.url.pathname, "/api/volumes/import")
  assert.deepEqual(Object.fromEntries(request.url.searchParams), {
    size: "12",
    name: "unity-projects",
  })
  request.complete(200, { job_id: "job-2" })
  assert.equal(await done, "job-2")
})

test("refusals, an expired session, cancellation and the size limit", async () => {
  const refused = fixture()
  const done = refused.upload.start(file(10), details, 100)
  refused.requests[0].complete(400, { error: "The archive has no manifest.json" })
  assert.equal(await done, "")
  assert.equal(refused.upload.status, "failed")
  assert.equal(refused.upload.error, "The archive has no manifest.json")

  const expired = fixture()
  const signedOut = expired.upload.start(file(10), details, 100)
  expired.requests[0].complete(401, { error: "Session expired. Sign in again." })
  await signedOut
  assert.equal(expired.unauthorized, 1)

  const cancelled = fixture()
  const stopped = cancelled.upload.start(file(10), details, 100)
  cancelled.upload.cancel()
  await stopped
  assert.equal(cancelled.upload.status, "cancelled")

  const large = fixture()
  assert.equal(await large.upload.start(file(101), details, 100), "")
  assert.equal(large.requests.length, 0)
  assert.match(large.upload.error, /100 B or smaller/)
})
