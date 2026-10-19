import { t } from "./i18n"
import type { ImageItem, ImageStep } from "./types"

/** Steps that came from the same image, one after another in Docker's order, newest first. */
export type StepGroup = {
  /** The list ID of the image the steps came from; `null` for the image's own. */
  from: string | null
  /** Bytes the group's steps wrote. */
  size: number
  /** The steps shown. */
  steps: ImageStep[]
  /** Settings steps left out because they write no files. */
  hidden: number
  /** The other images that have the group's layers, besides the one they came from. */
  sharedWith: string[]
}

/** A step worth a row of its own: it wrote bytes, or a layer. A setting writes neither. */
export const writesFiles = (step: ImageStep) => step.size > 0 || step.layer

/** The steps in runs from the same image, settings steps only when asked for. */
export function stepGroups(steps: ImageStep[], settings: boolean): StepGroup[] {
  const groups: StepGroup[] = []
  for (const step of steps) {
    let group = groups.at(-1)
    if (!group || group.from !== step.from) {
      group = { from: step.from, size: 0, steps: [], hidden: 0, sharedWith: [] }
      groups.push(group)
    }
    group.size += step.size
    if (step.layer)
      for (const id of step.shared_with)
        if (id !== step.from && !group.sharedWith.includes(id)) group.sharedWith.push(id)
    if (settings || writesFiles(step)) group.steps.push(step)
    else group.hidden += 1
  }
  return groups.filter((group) => group.steps.length > 0)
}

/** What a step did, in a word or two. */
export function stepKind(kind: ImageStep["kind"]) {
  switch (kind) {
    case "run":
      return t("layers.kind.run")
    case "copy":
      return t("layers.kind.copy")
    case "add":
      return t("layers.kind.add")
    case "config":
      return t("layers.kind.config")
    case "commit":
      return t("layers.kind.commit")
  }
}

/** An image's name from the list, or its ID when the list does not have it. */
export const imageName = (images: Pick<ImageItem, "id" | "name">[], id: string) =>
  images.find((image) => image.id === id)?.name ?? id

/**
 * The part of Docker's figure beyond the unpacked layers: the compressed copy the containerd image
 * store keeps of each layer. Zero with the classic store, where the two are the same.
 */
export const compressedCopies = (image: { size: number; layers_size: number }) =>
  Math.max(0, image.size - image.layers_size)
