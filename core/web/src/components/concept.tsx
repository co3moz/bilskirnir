import type * as React from "react"
import {
  RiApps2Line,
  RiComputerLine,
  RiDatabase2Line,
  RiPlugLine,
  RiStackLine,
  type RemixiconComponentType,
} from "@remixicon/react"
import { cn } from "cn"

import { Badge } from "@/components/ui/badge"
import { EmptyMedia } from "@/components/ui/empty"
import { conceptColor, type Concept } from "@/lib/concepts"

/** Each concept's icon. Another icon may stand in for it, as a cloud does for a catalog image. */
export const CONCEPT_ICONS: Record<Concept, RemixiconComponentType> = {
  container: RiComputerLine,
  image: RiStackLine,
  template: RiApps2Line,
  volume: RiDatabase2Line,
  mcp: RiPlugLine,
}

/**
 * A concept's color on an element, and `--concept` for its classes (`bg-(--concept)/10`). It is
 * inline so a focused menu or select item, which recolors its children, keeps it.
 */
export const conceptStyle = (concept: Concept) =>
  ({ "--concept": conceptColor(concept), color: "var(--concept)" }) as React.CSSProperties

/** The icon beside a thing's name, in its concept's color. */
export function ConceptIcon({
  concept,
  icon: Icon = CONCEPT_ICONS[concept],
  style,
  ...props
}: React.ComponentProps<RemixiconComponentType> & {
  concept: Concept
  icon?: RemixiconComponentType
}) {
  return (
    <Icon
      aria-hidden
      data-concept={concept}
      style={{ ...conceptStyle(concept), ...style }}
      {...props}
    />
  )
}

/** A badge naming what kind of thing something is, such as an image's source. */
export function ConceptBadge({
  concept,
  className,
  style,
  ...props
}: Omit<React.ComponentProps<typeof Badge>, "variant"> & { concept: Concept }) {
  return (
    <Badge
      variant="outline"
      data-concept={concept}
      className={cn("border-(--concept)/40", className)}
      style={{ ...conceptStyle(concept), ...style }}
      {...props}
    />
  )
}

/** The large icon of an empty state, on a tint of its concept's color. */
export function ConceptMedia({
  concept,
  icon,
}: {
  concept: Concept
  icon?: RemixiconComponentType
}) {
  return (
    <EmptyMedia variant="icon" className="bg-(--concept)/10" style={conceptStyle(concept)}>
      <ConceptIcon concept={concept} icon={icon} />
    </EmptyMedia>
  )
}
