import {
  CameraIcon,
  CirclePlayIcon,
  CircleStopIcon,
  EllipsisVerticalIcon,
  FileTextIcon,
  InfoIcon,
  RotateCcwIcon,
  RotateCwIcon,
  Settings2Icon,
  StethoscopeIcon,
  Trash2Icon,
  UploadIcon,
} from "lucide-react"

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
          <EllipsisVerticalIcon />
          <span className="sr-only">{spec.name} actions</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuItem onSelect={() => openDialog({ kind: "details", id })}>
          <InfoIcon />
          Details
        </DropdownMenuItem>
        {needsRetry(item) && (
          <DropdownMenuItem onSelect={() => attempt(() => retrySetup(id))}>
            <RotateCcwIcon />
            Retry setup
          </DropdownMenuItem>
        )}
        {!running && (
          <DropdownMenuItem onSelect={() => attempt(() => environmentAction(id, "start"))}>
            <CirclePlayIcon />
            Start
          </DropdownMenuItem>
        )}
        {running && (
          <DropdownMenuItem onSelect={() => attempt(() => environmentAction(id, "stop"))}>
            <CircleStopIcon />
            Stop
          </DropdownMenuItem>
        )}
        <DropdownMenuItem onSelect={() => attempt(() => environmentAction(id, "restart"))}>
          <RotateCwIcon />
          Restart
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={!running} onSelect={() => openDialog({ kind: "upload", id })}>
          <UploadIcon />
          Upload files
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => openDialog({ kind: "settings", id })}>
          <Settings2Icon />
          Settings
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => openDialog({ kind: "snapshot", id })}>
          <CameraIcon />
          Snapshot
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() => attempt(() => showLog(`${spec.name} logs`, `environments/${id}/logs`))}
        >
          <FileTextIcon />
          Logs
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() =>
            attempt(() => showLog(`${spec.name} diagnostics`, `environments/${id}/diagnostics`))
          }
        >
          <StethoscopeIcon />
          Diagnostics
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onSelect={() => openDialog({ kind: "delete", id })}>
          <Trash2Icon />
          Delete environment
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
