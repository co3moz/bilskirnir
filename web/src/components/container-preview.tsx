import * as React from "react"
import { RiComputerLine } from "@remixicon/react"
import { cn } from "cn"

import { Badge } from "@/components/ui/badge"
import { t } from "@/lib/i18n"
import { useContainers } from "@/lib/queries"
import type { ContainerItem } from "@/lib/types"

/** The live desktop, letterboxed rather than cropped so the whole screen stays visible. */
export function ContainerPreview({ item, className }: { item: ContainerItem; className?: string }) {
  const { id, spec } = item
  const running = item.state === "running"
  // The picture refreshes with the list, at most every ten seconds; the browser keeps the
  // previous picture on screen until the new one has loaded.
  const stamp = Math.floor(useContainers().dataUpdatedAt / 10000)
  const [failed, setFailed] = React.useState<number | null>(null)
  const shown = running && failed !== stamp

  return (
    <div
      className={cn(
        "relative flex aspect-video shrink-0 items-center justify-center overflow-hidden rounded-md border bg-muted/40",
        className
      )}
    >
      <div className="flex flex-col items-center gap-2 text-xs text-muted-foreground">
        <RiComputerLine className="size-6 opacity-60" />
        {running ? t("containers.loadingPreview") : t("containers.stopped")}
      </div>
      {shown && (
        <img
          alt={`${spec.name} desktop preview`}
          src={`/api/containers/${encodeURIComponent(id)}/screenshot?max_width=640&t=${stamp}`}
          loading="lazy"
          onError={() => setFailed(stamp)}
          className="absolute inset-0 size-full object-contain"
        />
      )}
      <Badge
        variant="secondary"
        className="absolute top-2 right-2 rounded-sm bg-background/80 font-mono text-[0.7rem] backdrop-blur-sm"
      >
        {spec.resolution}
      </Badge>
    </div>
  )
}
