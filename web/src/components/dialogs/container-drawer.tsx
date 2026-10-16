import {
  RiCameraLine,
  RiStopCircleLine,
  RiFileTextLine,
  RiResetLeftLine,
  RiRestartLine,
  RiSettings3Line,
  RiStethoscopeLine,
  RiDeleteBinLine,
  RiUploadLine,
} from "@remixicon/react"

import { PrimaryAction, SpecBadges } from "@/components/container-card"
import { ContainerPreview } from "@/components/container-preview"
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
import { attempt, containerAction, retrySetup, showLog } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { date, containerStatus, profileTitle } from "@/lib/format"
import { useContainers, useProfiles } from "@/lib/queries"
import { setupLabel } from "@/lib/templates"
import type { ContainerSetup } from "@/lib/types"
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

const setupTone = {
  pending: "warning",
  running: "info",
  completed: "success",
  failed: "danger",
} as const satisfies Record<ContainerSetup["status"], string>

/** dashboard-01's row viewer for one container: preview, configuration and every action. */
export function ContainerDrawer({ id, ...props }: DialogProps & { id: string }) {
  const item = useContainers().data?.find((entry) => entry.id === id)
  const profiles = useProfiles().data ?? []
  if (!item)
    return (
      <SideDrawer {...props}>
        <DrawerHeader>
          <DrawerTitle>Container</DrawerTitle>
          <DrawerDescription>This container no longer exists.</DrawerDescription>
        </DrawerHeader>
      </SideDrawer>
    )
  const { spec, setup } = item
  const running = item.state === "running"
  const status = containerStatus(item)
  // A setup still waiting after an error did not get to run, which reads as a failure.
  const setupStatus = setup?.status === "pending" && item.last_error ? "failed" : setup?.status

  return (
    <SideDrawer {...props}>
      <DrawerHeader className="gap-1">
        <DrawerTitle>{spec.name}</DrawerTitle>
        <DrawerDescription>{profileTitle(spec, item.desktop?.title, profiles)}</DrawerDescription>
      </DrawerHeader>
      <div className="flex flex-col gap-4 overflow-y-auto px-4 text-sm">
        <ContainerPreview item={item} />
        <div className="flex flex-wrap items-center gap-2">
          <StatusBadge state={status.key} tone={status.tone} />
          {item.status && <span className="text-xs text-muted-foreground">{item.status}</span>}
        </div>
        {item.last_error && (
          <Alert variant="destructive">
            <AlertTitle>Setup did not finish</AlertTitle>
            <AlertDescription>{item.last_error}</AlertDescription>
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
          <Button variant="outline" onClick={() => attempt(() => containerAction(id, "restart"))}>
            <RiRestartLine data-icon="inline-start" />
            Restart
          </Button>
          <Button
            variant="outline"
            onClick={() => attempt(() => showLog(`${spec.name} logs`, `containers/${id}/logs`))}
          >
            <RiFileTextLine data-icon="inline-start" />
            Logs
          </Button>
          <Button
            variant="outline"
            onClick={() =>
              attempt(() => showLog(`${spec.name} diagnostics`, `containers/${id}/diagnostics`))
            }
          >
            <RiStethoscopeLine data-icon="inline-start" />
            Diagnostics
          </Button>
          {running && (
            <Button variant="outline" onClick={() => attempt(() => containerAction(id, "stop"))}>
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
          <Fact label="Created">{date(item.created_at)}</Fact>
          <Fact label="Docker name">
            <code className="font-mono text-xs">{item.container}</code>
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
        {setup && setupStatus && (
          <>
            <Separator />
            <div className="flex flex-col gap-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-xs text-muted-foreground">Template setup</h3>
                <StatusBadge
                  state={setupStatus}
                  tone={setupTone[setupStatus]}
                  label={setupLabel({ status: setupStatus })}
                />
              </div>
              {Object.keys(setup.parameters).length + setup.secrets.length > 0 && (
                <dl className="grid grid-cols-2 gap-4">
                  {Object.entries(setup.parameters).map(([name, value]) => (
                    <Fact key={name} label={name}>
                      {/* Several choices arrive one per line. */}
                      <span className="whitespace-pre-wrap">
                        {value || <span className="text-muted-foreground">Empty</span>}
                      </span>
                    </Fact>
                  ))}
                  {setup.secrets.map((name) => (
                    <Fact key={name} label={name}>
                      <span className="text-muted-foreground">Secret</span>
                    </Fact>
                  ))}
                </dl>
              )}
              {running && setupStatus === "failed" && (
                <Button
                  variant="outline"
                  className="self-start"
                  onClick={() => attempt(() => retrySetup(id))}
                >
                  <RiResetLeftLine data-icon="inline-start" />
                  Retry setup
                </Button>
              )}
            </div>
          </>
        )}
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
