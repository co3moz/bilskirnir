import * as React from "react"
import { RiCloudLine, RiImageAddLine, type RemixiconComponentType } from "@remixicon/react"
import { cn } from "cn"

import { CONCEPT_ICONS, ConceptIcon } from "@/components/concept"
import { Button } from "@/components/ui/button"
import { SelectItem } from "@/components/ui/select"
import { Spinner } from "@/components/ui/spinner"
import type { Concept } from "@/lib/concepts"
import type { ImageChoice } from "@/lib/format"
import { iconFromBlob } from "@/lib/icons"
import { t } from "@/lib/i18n"

/**
 * An image's or a template's icon, or a line icon in its concept's color when it has none or the
 * picture does not load.
 */
export function ImageIcon({
  src,
  concept = "image",
  fallback = CONCEPT_ICONS[concept],
  className,
}: {
  src?: string
  concept?: Concept
  fallback?: RemixiconComponentType
  className?: string
}) {
  const [failed, setFailed] = React.useState<string>()
  if (src && failed !== src)
    return (
      <img
        src={src}
        alt=""
        draggable={false}
        onError={() => setFailed(src)}
        className={cn("shrink-0 rounded-md object-contain", className)}
      />
    )
  return <ConceptIcon concept={concept} icon={fallback} className={cn("shrink-0", className)} />
}

/** An image in a select, with its own icon or a line icon saying where it comes from. */
export function ImageChoiceItem({ choice }: { choice: ImageChoice }) {
  return (
    <SelectItem value={choice.value} disabled={choice.disabled}>
      <ImageIcon
        src={choice.icon}
        fallback={choice.catalog ? RiCloudLine : undefined}
        className="size-4 rounded-sm"
      />
      {choice.label}
      {choice.disabled ? ` · ${t("images.unavailable")}` : ""}
    </SelectItem>
  )
}

/**
 * Chooses an icon: a picture from a file, scaled to fit 256 pixels a side and kept as a PNG data
 * URL. `undefined` means no icon.
 */
export function IconPicker({
  value,
  onChange,
  onError,
  id,
}: {
  value?: string
  onChange: (value: string | undefined) => void
  onError: (message: string | undefined) => void
  id?: string
}) {
  const input = React.useRef<HTMLInputElement>(null)
  const [busy, setBusy] = React.useState(false)
  const choose = () => input.current?.click()
  return (
    <div className="flex items-center gap-3">
      <button
        type="button"
        id={id}
        onClick={choose}
        disabled={busy}
        aria-label={value ? t("icons.change") : t("icons.choose")}
        className="flex size-16 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-dashed border-input bg-muted/40 transition-colors outline-none hover:bg-muted focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
      >
        {busy ? (
          <Spinner />
        ) : value ? (
          <img src={value} alt="" className="size-full object-contain" />
        ) : (
          <RiImageAddLine aria-hidden className="size-6 text-muted-foreground" />
        )}
      </button>
      <div className="flex min-w-0 flex-col items-start gap-1">
        <div className="flex flex-wrap gap-2">
          <Button type="button" variant="outline" size="sm" onClick={choose} disabled={busy}>
            {value ? t("icons.change") : t("icons.choose")}
          </Button>
          {value && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => {
                onChange(undefined)
                onError(undefined)
              }}
            >
              {t("icons.remove")}
            </Button>
          )}
        </div>
        <p className="text-xs text-muted-foreground">{t("icons.hint")}</p>
      </div>
      <input
        ref={input}
        type="file"
        accept="image/png,image/jpeg,image/webp,image/gif,image/svg+xml"
        hidden
        onChange={async (event) => {
          const file = event.target.files?.[0]
          event.target.value = ""
          if (!file) return
          setBusy(true)
          try {
            onChange(await iconFromBlob(file))
            onError(undefined)
          } catch (error) {
            onError((error as Error).message)
          } finally {
            setBusy(false)
          }
        }}
      />
    </div>
  )
}
