import {
  RiCameraLine,
  RiStopCircleLine,
  RiFileTextLine,
  RiRestartLine,
  RiSettings3Line,
  RiStethoscopeLine,
  RiDeleteBinLine,
  RiUploadLine,
} from "@remixicon/react"

import { PrimaryAction, SpecBadges } from "@/components/environment-card"
import { EnvironmentPreview } from "@/components/environment-preview"
import { StatusBadge } from "@/components/status-badge"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  DrawerClose,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@/components/ui/drawer"
import { Separator } from "@/components/ui/separator"
import { attempt, environmentAction, showLog } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { date, environmentStatus, profileTitle } from "@/lib/format"
import { useEnvironments, useProfiles } from "@/lib/queries"
import { SideDrawer, type DialogProps } from "./side-drawer"

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="break-words">{children}</dd>
    </div>
  )
}

const yes = (value: boolean) => (value ? "On" : "Off")

/** dashboard-01's row viewer for one environment: preview, configuration and every action. */
export function EnvironmentDrawer({ id, ...props }: DialogProps & { id: string }) {
  const item = useEnvironments().data?.find((entry) => entry.environment.id === id)
  const profiles = useProfiles().data ?? []
  if (!item)
    return (
      <SideDrawer {...props}>
        <DrawerHeader>
          <DrawerTitle>Environment</DrawerTitle>
          <DrawerDescription>This environment no longer exists.</DrawerDescription>
        </DrawerHeader>
      </SideDrawer>
    )
  const { environment } = item
  const { spec } = environment
  const running = item.state === "running"
  const status = environmentStatus(item)

  return (
    <SideDrawer {...props}>
      <DrawerHeader className="gap-1">
        <DrawerTitle>{spec.name}</DrawerTitle>
        <DrawerDescription>
          {profileTitle(spec, environment.desktop?.title, profiles)}
        </DrawerDescription>
      </DrawerHeader>
      <div className="flex flex-col gap-4 overflow-y-auto px-4 text-sm">
        <EnvironmentPreview item={item} />
        <div className="flex flex-wrap items-center gap-2">
          <StatusBadge state={status.key} tone={status.tone} />
          {item.status && <span className="text-xs text-muted-foreground">{item.status}</span>}
        </div>
        {environment.last_error && (
          <Alert variant="destructive">
            <AlertTitle>Setup did not finish</AlertTitle>
            <AlertDescription>{environment.last_error}</AlertDescription>
          </Alert>
        )}
        <div className="grid grid-cols-2 gap-2">
          <Button variant="outline" onClick={() => openDialog({ kind: "settings", id })}>
            <RiSettings3Line data-icon="inline-start" />
            Settings
          </Button>
          <Button variant="outline" onClick={() => openDialog({ kind: "snapshot", id })}>
            <RiCameraLine data-icon="inline-start" />
            Snapshot
          </Button>
          <Button
            variant="outline"
            disabled={!running}
            onClick={() => openDialog({ kind: "upload", id })}
          >
            <RiUploadLine data-icon="inline-start" />
            Upload files
          </Button>
          <Button variant="outline" onClick={() => attempt(() => environmentAction(id, "restart"))}>
            <RiRestartLine data-icon="inline-start" />
            Restart
          </Button>
          <Button
            variant="outline"
            onClick={() => attempt(() => showLog(`${spec.name} logs`, `environments/${id}/logs`))}
          >
            <RiFileTextLine data-icon="inline-start" />
            Logs
          </Button>
          <Button
            variant="outline"
            onClick={() =>
              attempt(() => showLog(`${spec.name} diagnostics`, `environments/${id}/diagnostics`))
            }
          >
            <RiStethoscopeLine data-icon="inline-start" />
            Diagnostics
          </Button>
          {running && (
            <Button variant="outline" onClick={() => attempt(() => environmentAction(id, "stop"))}>
              <RiStopCircleLine data-icon="inline-start" />
              Stop
            </Button>
          )}
          <Button variant="destructive" onClick={() => openDialog({ kind: "delete", id })}>
            <RiDeleteBinLine data-icon="inline-start" />
            Delete
          </Button>
        </div>
        <SpecBadges spec={spec} />
        <Separator />
        <dl className="grid grid-cols-2 gap-4">
          <Fact label="Resolution">{spec.resolution}</Fact>
          <Fact label="Shared memory">{spec.shm_mb} MB</Fact>
          <Fact label="Internet">{yes(spec.internet)}</Fact>
          <Fact label="Sudo">{yes(spec.sudo)}</Fact>
          <Fact label="Created">{date(environment.created_at)}</Fact>
          <Fact label="Container">
            <code className="font-mono text-xs">{environment.container}</code>
          </Fact>
          <div className="col-span-2">
            <Fact label="Image">
              <code className="font-mono text-xs">{spec.image}</code>
            </Fact>
          </div>
        </dl>
        <Separator />
        <div className="flex flex-col gap-2">
          <h3 className="text-xs text-muted-foreground">Volumes</h3>
          {spec.mounts.length ? (
            <ul className="flex flex-col gap-1">
              {spec.mounts.map((mount) => (
                <li key={mount.target} className="flex flex-wrap items-center gap-x-2 text-xs">
                  <code className="font-mono">{mount.volume}</code>
                  <span className="text-muted-foreground">at</span>
                  <code className="font-mono">{mount.target}</code>
                  {mount.readonly && <span className="text-muted-foreground">(read-only)</span>}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-xs text-muted-foreground">No volumes mounted.</p>
          )}
          <p className="text-xs text-muted-foreground">
            Mounted volumes are excluded from snapshots.
          </p>
        </div>
      </div>
      <DrawerFooter>
        <PrimaryAction item={item} />
        <DrawerClose asChild>
          <Button variant="outline">Done</Button>
        </DrawerClose>
      </DrawerFooter>
    </SideDrawer>
  )
}
