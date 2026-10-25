import { t } from "./i18n"
import type { ImageItem } from "./types"

/** The largest width and height of an icon; a larger picture is scaled down to fit. */
export const ICON_SIZE = 256
/** The manager refuses a larger PNG; an uncompressed 256 by 256 one still fits. */
export const ICON_LIMIT = 300 * 1024
const PREFIX = "data:image/png;base64,"

/** Where a saved image's icon loads from; the version keeps a cached copy until it changes. */
export const iconUrl = (image: Pick<ImageItem, "id" | "icon">) =>
  image.icon
    ? `/api/images/${encodeURIComponent(image.id)}/icon?v=${encodeURIComponent(image.icon)}`
    : undefined

/** How many bytes a base64 data URL holds. */
export function dataUrlBytes(url: string) {
  const data = url.slice(url.indexOf(",") + 1)
  const padding = data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0
  return Math.floor((data.length * 3) / 4) - padding
}

/** An icon as a configuration carries it, or throws why the manager would refuse it. */
export function readIcon(value: unknown): string {
  if (typeof value !== "string" || !value.startsWith(PREFIX)) throw new Error(t("icons.format"))
  if (dataUrlBytes(value) > ICON_LIMIT) throw new Error(t("icons.tooLarge"))
  return value
}

function dataUrl(blob: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(blob)
  })
}

/**
 * Turns a picture into an icon: scaled down to fit 256 pixels a side, never up, and saved as a
 * PNG. It is read as a data URL because the interface's content security policy allows no
 * `blob:` images, and a canvas drawn from a data URL can still be saved.
 */
export async function iconFromBlob(blob: Blob): Promise<string> {
  const image = new Image()
  image.src = await dataUrl(blob)
  try {
    await image.decode()
  } catch {
    throw new Error(t("icons.unreadable"))
  }
  // An SVG without its own size reports none; it is drawn at the full icon size.
  const width = image.naturalWidth || ICON_SIZE
  const height = image.naturalHeight || ICON_SIZE
  const scale = Math.min(1, ICON_SIZE / Math.max(width, height))
  const canvas = document.createElement("canvas")
  canvas.width = Math.max(1, Math.round(width * scale))
  canvas.height = Math.max(1, Math.round(height * scale))
  const context = canvas.getContext("2d")
  if (!context) throw new Error(t("icons.unreadable"))
  context.imageSmoothingQuality = "high"
  context.drawImage(image, 0, 0, canvas.width, canvas.height)
  return readIcon(canvas.toDataURL("image/png"))
}
