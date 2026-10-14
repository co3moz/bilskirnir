import * as React from "react"
import { RiComputerLine } from "@remixicon/react"
import { cn } from "cn"

import { Badge } from "@/components/ui/badge"
import { useEnvironments } from "@/lib/queries"
import type { EnvironmentItem } from "@/lib/types"

/** The live desktop, letterboxed rather than cropped so the whole screen stays visible. */
export function EnvironmentPreview({
  item,
  className,
}: {
  item: EnvironmentItem
  className?: string
}) {
  const { id, spec } = item.environment
  const running = item.state === "running"
  // The picture refreshes with the list, at most every ten seconds; the browser keeps the
  // previous picture on screen until the new one has loaded.
  const stamp = Math.floor(useEnvironments().dataUpdatedAt / 10000)
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
        {running ? "Loading preview" : "Desktop stopped"}
      </div>
      {shown && (
        <img
          alt={`${spec.name} desktop preview`}
          src={`/api/environments/${encodeURIComponent(id)}/screenshot?max_width=640&t=${stamp}`}
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
