import * as React from "react"

import { cn } from "cn"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  DrawerClose,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@/components/ui/drawer"
import { Field, FieldLabel } from "@/components/ui/field"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { imageKind, shortDate } from "@/lib/format"
import { formatList, plural, t } from "@/lib/i18n"
import {
  compressedCopies,
  imageName,
  stepGroups,
  stepKind,
  writesFiles,
  type StepGroup,
} from "@/lib/layers"
import { useImageLayers, useImages } from "@/lib/queries"
import type { ImageItem, ImageStep } from "@/lib/types"
import { fileSize } from "@/lib/uploads"
import { SideDrawer, type DialogProps } from "./side-drawer"

/** An instruction longer than this starts folded to three lines. */
const LONG_INSTRUCTION = 160

/** A figure with its label and hint; on a phone the value sits beside them to save height. */
function Figure({ label, value, hint }: { label: string; value: string; hint: string }) {
  return (
    <div className="grid grid-cols-[1fr_auto] gap-x-3 gap-y-0.5 rounded-xl border p-3 sm:flex sm:flex-col">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="row-span-2 self-center text-base font-medium tabular-nums sm:self-auto">
        {value}
      </dd>
      <dd className="text-xs text-muted-foreground">{hint}</dd>
    </div>
  )
}

/** One step: what it did, the bytes it wrote against the largest step, and its instruction. */
function StepRow({ step, largest, own }: { step: ImageStep; largest: number; own: boolean }) {
  const [unfolded, setUnfolded] = React.useState(false)
  // A commit records the container's command, which says nothing; its message names the snapshot.
  const text = step.kind === "commit" ? step.comment || t("layers.commitHint") : step.instruction
  const long = text.length > LONG_INSTRUCTION
  return (
    <li className="flex flex-col gap-1.5 py-2">
      <div className="flex items-baseline justify-between gap-2 text-xs">
        <span className="font-medium">{stepKind(step.kind)}</span>
        <span
          className={cn(
            "whitespace-nowrap tabular-nums",
            step.size === 0 && "text-muted-foreground"
          )}
        >
          {fileSize(step.size)}
        </span>
      </div>
      {step.size > 0 && largest > 0 && (
        // The track makes a sliver of a small step read as part of a scale.
        <div className="h-1.5 rounded-r-[4px] bg-muted" aria-hidden>
          <div
            className={cn("h-full rounded-r-[4px]", own ? "bg-violet" : "bg-muted-foreground/40")}
            style={{ width: `max(2px, ${(step.size / largest) * 100}%)` }}
          />
        </div>
      )}
      <code
        className={cn(
          "font-mono text-[0.7rem] break-all whitespace-pre-wrap text-muted-foreground",
          long && !unfolded && "line-clamp-3"
        )}
      >
        {text}
      </code>
      {step.kind !== "commit" && step.comment && (
        <p className="text-xs text-muted-foreground">{step.comment}</p>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2">
        {step.created !== null && (
          <span className="text-xs text-muted-foreground">{shortDate(step.created)}</span>
        )}
        {long && (
          <Button
            variant="link"
            size="sm"
            className="ml-auto h-auto p-0 text-xs"
            aria-expanded={unfolded}
            onClick={() => setUnfolded(!unfolded)}
          >
            {unfolded ? t("layers.showLess") : t("layers.showMore")}
          </Button>
        )}
      </div>
    </li>
  )
}

/** The other images that have a group's layers, a few by name. */
function sharedText(ids: string[], images: ImageItem[]) {
  const names = ids.map((id) => imageName(images, id))
  const shown =
    names.length > 3 ? [...names.slice(0, 2), t("layers.more", { count: names.length - 2 })] : names
  return t("layers.alsoIn", { names: formatList(shown) })
}

function Group({
  group,
  images,
  largest,
}: {
  group: StepGroup
  images: ImageItem[]
  largest: number
}) {
  const own = group.from === null
  return (
    <section className="flex flex-col">
      <div className="flex flex-wrap items-baseline justify-between gap-x-2">
        <h4 className="flex items-center gap-2 text-sm font-medium">
          <span
            className={cn("size-2 rounded-full", own ? "bg-violet" : "bg-muted-foreground/40")}
            aria-hidden
          />
          {own ? t("layers.own") : t("layers.from", { name: imageName(images, group.from!) })}
        </h4>
        <span className="text-xs tabular-nums">{fileSize(group.size)}</span>
      </div>
      {group.sharedWith.length > 0 && (
        <p
          className="text-xs text-muted-foreground"
          title={group.sharedWith.map((id) => imageName(images, id)).join("\n")}
        >
          {sharedText(group.sharedWith, images)}
        </p>
      )}
      <ol className="flex flex-col divide-y">
        {group.steps.map((step, index) => (
          <StepRow key={index} step={step} largest={largest} own={own} />
        ))}
      </ol>
    </section>
  )
}

/** dashboard-01's row viewer for one image: what its disk space is made of, step by step. */
export function ImageDrawer({ id, ...props }: DialogProps & { id: string }) {
  const images = useImages().data ?? []
  const image = images.find((entry) => entry.id === id)
  const layers = useImageLayers(id)
  const [settings, setSettings] = React.useState(false)
  const data = layers.data
  const groups = React.useMemo(
    () => (data ? stepGroups(data.steps, settings) : []),
    [data, settings]
  )
  const settingSteps = data?.steps.filter((step) => !writesFiles(step)).length ?? 0
  const largest = data ? Math.max(0, ...data.steps.map((step) => step.size)) : 0
  const compressed = data ? compressedCopies(data) : 0

  return (
    <SideDrawer {...props} wide>
      <DrawerHeader className="gap-1">
        <DrawerTitle>{image?.name ?? t("layers.title")}</DrawerTitle>
        <DrawerDescription asChild>
          {/* Centered like the header on a phone, where the drawer opens from the bottom. */}
          <div className="flex flex-wrap items-center justify-center gap-2 md:justify-start">
            {image ? (
              <>
                <Badge variant="outline" className="border-violet/40 text-violet">
                  {imageKind(image.kind)}
                </Badge>
                <code className="font-mono text-[0.7rem] break-all">{image.reference}</code>
              </>
            ) : (
              <span>{t("images.gone")}</span>
            )}
          </div>
        </DrawerDescription>
      </DrawerHeader>
      <div className="flex flex-col gap-4 overflow-y-auto px-4 text-sm">
        <p className="text-xs text-muted-foreground">{t("layers.description")}</p>
        {layers.error ? (
          <p className="text-destructive">{layers.error.message}</p>
        ) : !data ? (
          <>
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-48 w-full" />
          </>
        ) : (
          <>
            <dl className="grid gap-2 sm:grid-cols-3">
              <Figure
                label={t("layers.onDisk")}
                value={fileSize(data.size)}
                hint={
                  compressed > 0
                    ? t("layers.compressedHint", { size: fileSize(compressed) })
                    : t("layers.onDiskHint")
                }
              />
              <Figure
                label={t("layers.ownSize")}
                value={fileSize(data.own_size)}
                hint={t("layers.ownHint")}
              />
              <Figure
                label={t("layers.sharedSize")}
                value={fileSize(data.layers_size - data.own_size)}
                hint={t("layers.sharedHint")}
              />
            </dl>
            <Separator />
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h3 className="text-xs text-muted-foreground">{t("layers.steps")}</h3>
              {settingSteps > 0 && (
                <Field orientation="horizontal" className="w-auto">
                  <Switch id={`${id}-settings`} checked={settings} onCheckedChange={setSettings} />
                  <FieldLabel htmlFor={`${id}-settings`} className="text-xs font-normal">
                    {plural(settingSteps, {
                      one: "layers.showSettings.one",
                      other: "layers.showSettings.other",
                    })}
                  </FieldLabel>
                </Field>
              )}
            </div>
            <div className="flex flex-col gap-4 pb-2">
              {groups.map((group, index) => (
                <Group key={index} group={group} images={images} largest={largest} />
              ))}
            </div>
          </>
        )}
      </div>
      <DrawerFooter>
        <DrawerClose asChild>
          <Button variant="outline">{t("common.done")}</Button>
        </DrawerClose>
      </DrawerFooter>
    </SideDrawer>
  )
}
