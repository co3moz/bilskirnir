import { formatNumber, t } from "./i18n"

const oneDecimal = { minimumFractionDigits: 1, maximumFractionDigits: 1 }

export const fileSize = (bytes: number) =>
  bytes < 1024
    ? `${formatNumber(bytes)} B`
    : bytes < 1024 ** 2
      ? `${formatNumber(bytes / 1024, oneDecimal)} KiB`
      : bytes < 1024 ** 3
        ? `${formatNumber(bytes / 1024 ** 2, oneDecimal)} MiB`
        : `${formatNumber(bytes / 1024 ** 3, oneDecimal)} GiB`

export type UploadStatus = "queued" | "uploading" | "saving" | "completed" | "failed" | "cancelled"

export const uploadStatus = (status: UploadStatus) => t(`uploads.status.${status}`)

export type UploadItem = {
  file: Blob
  name: string
  size: number
  loaded: number
  status: UploadStatus
  error: string
  path: string
}

type UploadFile = Blob & { name: string }

/** The part of XMLHttpRequest an upload uses, so tests can fake it. */
export type UploadRequest = Pick<
  XMLHttpRequest,
  | "upload"
  | "status"
  | "responseText"
  | "withCredentials"
  | "timeout"
  | "open"
  | "setRequestHeader"
  | "send"
  | "abort"
  | "onload"
  | "onerror"
  | "ontimeout"
  | "onabort"
>

/**
 * Where an upload goes: `target` is the API collection that receives files, such as
 * `containers/<id>` or `volumes/<name>`, and `destination` a directory within it.
 */
export type UploadOptions = { target: string; destination: string; replace: boolean }

export class FileUploads {
  createRequest: () => UploadRequest
  onChange: () => void
  onUnauthorized: () => void
  items: UploadItem[] = []
  running = false
  cancelled = false
  request: UploadRequest | null = null

  constructor({
    createRequest,
    onChange = () => {},
    onUnauthorized = () => {},
  }: {
    createRequest: () => UploadRequest
    onChange?: () => void
    onUnauthorized?: () => void
  }) {
    this.createRequest = createRequest
    this.onChange = onChange
    this.onUnauthorized = onUnauthorized
  }
  select(files: ArrayLike<UploadFile> | Iterable<UploadFile>, limit: number) {
    const list = Array.from(files)
    if (this.running) throw new Error(t("uploads.busy"))
    if (list.length > 100) throw new Error(t("uploads.tooMany"))
    if (list.some((file) => file.size > limit))
      throw new Error(t("uploads.tooLarge", { size: fileSize(limit) }))
    this.items = list.map((file) => ({
      file,
      name: file.name,
      size: file.size,
      loaded: 0,
      status: "queued",
      error: "",
      path: "",
    }))
    this.cancelled = false
    this.onChange()
  }
  cancel() {
    if (!this.running) return
    this.cancelled = true
    this.request?.abort()
    for (const item of this.items) if (item.status === "queued") item.status = "cancelled"
    this.onChange()
  }
  async start(options: UploadOptions) {
    if (this.running) return
    this.running = true
    this.cancelled = false
    this.onChange()
    try {
      for (const item of this.items.filter((item) => item.status !== "completed")) {
        if (this.cancelled) break
        item.status = "uploading"
        item.error = ""
        item.loaded = 0
        this.onChange()
        try {
          const result = await this.send(item, options)
          item.status = "completed"
          item.loaded = item.size
          item.path = result.path ?? ""
        } catch (error) {
          item.status = this.cancelled ? "cancelled" : "failed"
          item.error = (error as Error).message
        }
        this.onChange()
      }
    } finally {
      this.request = null
      this.running = false
      this.onChange()
    }
  }
  send(item: UploadItem, { target, destination, replace }: UploadOptions) {
    return new Promise<{ path?: string }>((resolve, reject) => {
      const request = this.createRequest()
      this.request = request
      const query = new URLSearchParams({
        destination,
        name: item.name,
        size: String(item.size),
        replace: String(replace),
      })
      request.open("POST", `/api/${target}/files?${query}`)
      request.setRequestHeader("Content-Type", "application/octet-stream")
      request.setRequestHeader("X-Wsld-Request", "1")
      request.withCredentials = true
      request.timeout = 3_600_000
      request.upload.onprogress = (event) => {
        item.loaded = Math.min(event.loaded, item.size)
        this.onChange()
      }
      request.upload.onload = () => {
        item.status = "saving"
        this.onChange()
      }
      request.onload = () => {
        let result: { path?: string; error?: string }
        try {
          result = JSON.parse(request.responseText)
        } catch {
          result = {}
        }
        if (request.status >= 200 && request.status < 300) resolve(result)
        else {
          if (request.status === 401) {
            this.cancelled = true
            this.onUnauthorized()
          }
          reject(new Error(result.error || t("uploads.failed", { status: request.status })))
        }
      }
      request.onerror = () => reject(new Error(t("uploads.connectionLost")))
      request.ontimeout = () => reject(new Error(t("uploads.timedOut")))
      request.onabort = () => reject(new Error(t("uploads.cancelled")))
      request.send(item.file)
    })
  }
}
