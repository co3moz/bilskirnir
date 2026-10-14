import {
  RiCameraLine,
  RiPlayCircleLine,
  RiStopCircleLine,
  RiMore2Line,
  RiFileTextLine,
  RiInformationLine,
  RiResetLeftLine,
  RiRestartLine,
  RiSettings3Line,
  RiStethoscopeLine,
  RiDeleteBinLine,
  RiUploadLine,
} from "@remixicon/react"

import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { attempt, environmentAction, retrySetup, showLog } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { needsRetry } from "@/lib/format"
import type { EnvironmentItem } from "@/lib/types"

/** Every action on one environment, as the old details dialog offered them. */
export function EnvironmentMenu({ item }: { item: EnvironmentItem }) {
  const { id, spec } = item.environment
  const running = item.state === "running"
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          className="flex size-8 text-muted-foreground data-[state=open]:bg-muted"
          size="icon"
        >
          <RiMore2Line />
          <span className="sr-only">{spec.name} actions</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuItem onSelect={() => openDialog({ kind: "details", id })}>
          <RiInformationLine />
          Details
        </DropdownMenuItem>
        {needsRetry(item) && (
          <DropdownMenuItem onSelect={() => attempt(() => retrySetup(id))}>
            <RiResetLeftLine />
            Retry setup
          </DropdownMenuItem>
        )}
        {!running && (
          <DropdownMenuItem onSelect={() => attempt(() => environmentAction(id, "start"))}>
            <RiPlayCircleLine />
            Start
          </DropdownMenuItem>
        )}
        {running && (
          <DropdownMenuItem onSelect={() => attempt(() => environmentAction(id, "stop"))}>
            <RiStopCircleLine />
            Stop
          </DropdownMenuItem>
        )}
        <DropdownMenuItem onSelect={() => attempt(() => environmentAction(id, "restart"))}>
          <RiRestartLine />
          Restart
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={!running} onSelect={() => openDialog({ kind: "upload", id })}>
          <RiUploadLine />
          Upload files
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => openDialog({ kind: "settings", id })}>
          <RiSettings3Line />
          Settings
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => openDialog({ kind: "snapshot", id })}>
          <RiCameraLine />
          Snapshot
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() => attempt(() => showLog(`${spec.name} logs`, `environments/${id}/logs`))}
        >
          <RiFileTextLine />
          Logs
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() =>
            attempt(() => showLog(`${spec.name} diagnostics`, `environments/${id}/diagnostics`))
          }
        >
          <RiStethoscopeLine />
          Diagnostics
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onSelect={() => openDialog({ kind: "delete", id })}>
          <RiDeleteBinLine />
          Delete environment
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
