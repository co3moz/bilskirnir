import { t } from "./i18n"
import { fileSize, type UploadRequest } from "./uploads"
import type { DesktopInfo } from "./types"

/** Thoric's description of an exported image, the first entry of its archives. */
export type ArchiveInfo = {
  format: number
  name: string
  description?: string
  kind?: string
  desktop?: DesktopInfo | null
}

const METADATA = "thoric.json"
const BLOCK = 512

export const archiveUrl = (id: string) => `/api/images/${encodeURIComponent(id)}/archive`

/** A record name from the archive's file name, without its extension. */
export const archiveName = (fileName: string) =>
  fileName
    .replace(/\.(tar\.gz|tgz|tar|gz)$/i, "")
    .trim()
    .slice(0, 100)

const isGzip = (bytes: Uint8Array) => bytes[0] === 0x1f && bytes[1] === 0x8b

/** The metadata of a script build carries its setup script, as the manager allows. */
const METADATA_LIMIT = 8 << 20

/** The length of the metadata entry an archive opens with, or 0 when it opens with another. */
function metadataLength(tar: Uint8Array) {
  if (tar.length < BLOCK) return 0
  const field = (start: number, length: number) => {
    const bytes = tar.subarray(start, start + length)
    const end = bytes.indexOf(0)
    return new TextDecoder().decode(end < 0 ? bytes : bytes.subarray(0, end))
  }
  if (field(0, 100) !== METADATA || tar[156] !== 0x30) return 0
  const size = parseInt(field(124, 12).trim(), 8)
  return size > 0 && size <= METADATA_LIMIT ? BLOCK + size : 0
}

/** The metadata entry at the start of an unpacked archive; null for a plain `docker save`. */
export function parseArchiveInfo(tar: Uint8Array): ArchiveInfo | null {
  const length = metadataLength(tar)
  if (!length || tar.length < length) return null
  try {
    const info = JSON.parse(new TextDecoder().decode(tar.subarray(BLOCK, length)))
    return info?.format === 1 && typeof info.name === "string" ? (info as ArchiveInfo) : null
  } catch {
    return null
  }
}

/** Reads the metadata from the start of an archive file, without reading the rest of it. */
export async function readArchiveInfo(file: Blob): Promise<ArchiveInfo | null> {
  const head = new Uint8Array(await file.slice(0, METADATA_LIMIT + 2 * BLOCK).arrayBuffer())
  if (!isGzip(head)) return parseArchiveInfo(head)
  const reader = new Blob([head]).stream().pipeThrough(new DecompressionStream("gzip")).getReader()
  const chunks: Uint8Array[] = []
  let length = 0
  let wanted = BLOCK
  try {
    while (length < wanted) {
      const { done, value } = await reader.read()
      if (done) break
      chunks.push(value)
      length += value.length
      // Once the first header is in, read on only as far as the metadata entry reaches.
      if (wanted === BLOCK && length >= BLOCK) wanted = metadataLength(join(chunks, length))
    }
  } catch {
    // The slice ends inside the compressed stream; the entry, if any, was read before that.
  }
  reader.cancel().catch(() => {})
  return parseArchiveInfo(join(chunks, length))
}

function join(chunks: Uint8Array[], length: number) {
  const out = new Uint8Array(length)
  let offset = 0
  for (const chunk of chunks) {
    out.set(chunk, offset)
    offset += chunk.length
  }
  return out
}

export type ImportStatus = "idle" | "uploading" | "checking" | "started" | "failed" | "cancelled"

/**
 * One archive upload; the manager answers with a job once the archive has passed its checks.
 * `path` is the import route: `images/import` for an image, `volumes/import` for a volume.
 */
export class ArchiveImport {
  path: string
  createRequest: () => UploadRequest
  onChange: () => void
  onUnauthorized: () => void
  status: ImportStatus = "idle"
  loaded = 0
  size = 0
  error = ""
  jobId = ""
  cancelled = false
  request: UploadRequest | null = null

  constructor({
    path = "images/import",
    createRequest,
    onChange = () => {},
    onUnauthorized = () => {},
  }: {
    path?: string
    createRequest: () => UploadRequest
    onChange?: () => void
    onUnauthorized?: () => void
  }) {
    this.path = path
    this.createRequest = createRequest
    this.onChange = onChange
    this.onUnauthorized = onUnauthorized
  }
  get running() {
    return this.status === "uploading" || this.status === "checking"
  }
  cancel() {
    if (!this.running) return
    this.cancelled = true
    this.request?.abort()
  }
  /** Resolves with the job ID, or with an empty string after a failure described by `error`. */
  async start(file: Blob, details: Record<string, string>, limit: number) {
    if (this.running) return ""
    this.size = file.size
    this.loaded = 0
    this.error = ""
    this.jobId = ""
    this.cancelled = false
    if (file.size > limit) {
      this.status = "failed"
      this.error = t("archives.tooLarge", { size: fileSize(limit) })
      this.onChange()
      return ""
    }
    this.status = "uploading"
    this.onChange()
    try {
      this.jobId = await this.send(file, details)
      this.status = "started"
    } catch (error) {
      this.status = this.cancelled ? "cancelled" : "failed"
      this.error = (error as Error).message
    } finally {
      this.request = null
      this.onChange()
    }
    return this.jobId
  }
  send(file: Blob, details: Record<string, string>) {
    return new Promise<string>((resolve, reject) => {
      const request = this.createRequest()
      this.request = request
      const query = new URLSearchParams({ size: String(file.size), ...details })
      request.open("POST", `/api/${this.path}?${query}`)
      request.setRequestHeader("Content-Type", "application/octet-stream")
      request.setRequestHeader("X-Thoric-Request", "1")
      request.withCredentials = true
      // Docker reads the archive as it arrives, so a large one takes as long as Docker needs.
      request.timeout = 4 * 3_600_000
      request.upload.onprogress = (event) => {
        this.loaded = Math.min(event.loaded, file.size)
        this.onChange()
      }
      request.upload.onload = () => {
        this.loaded = file.size
        this.status = "checking"
        this.onChange()
      }
      request.onload = () => {
        let result: { job_id?: string; error?: string }
        try {
          result = JSON.parse(request.responseText)
        } catch {
          result = {}
        }
        if (request.status >= 200 && request.status < 300 && result.job_id) resolve(result.job_id)
        else {
          if (request.status === 401) this.onUnauthorized()
          reject(new Error(result.error || t("archives.failed", { status: request.status })))
        }
      }
      request.onerror = () => reject(new Error(t("archives.connectionLost")))
      request.ontimeout = () => reject(new Error(t("archives.timedOut")))
      request.onabort = () => reject(new Error(t("archives.cancelled")))
      request.send(file)
    })
  }
}
