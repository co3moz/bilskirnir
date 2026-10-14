import * as React from "react"
import {
  RiPlayCircleLine,
  RiCpuLine,
  RiGamepadLine,
  RiMicLine,
  RiComputerLine,
  RiResetLeftLine,
  RiUploadLine,
  RiVolumeUpLine,
} from "@remixicon/react"
import { cn } from "cn"

import { EnvironmentMenu } from "@/components/environment-menu"
import { EnvironmentPreview } from "@/components/environment-preview"
import { StatusBadge } from "@/components/status-badge"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useFileDrop } from "@/hooks/use-file-drop"
import { attempt, environmentAction, retrySetup, setInternet } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { environmentStatus, needsRetry, profileTitle } from "@/lib/format"
import { href } from "@/lib/route"
import type { EnvironmentItem, EnvironmentSpec, ProfileItem } from "@/lib/types"

/** The resources and devices a desktop was given, as compact badges. */
export function SpecBadges({ spec, className }: { spec: EnvironmentSpec; className?: string }) {
  return (
    <div className={cn("flex flex-wrap gap-1.5", className)}>
      <Badge variant="outline" className="text-muted-foreground">
        <RiCpuLine />
        {spec.gpu ? "GPU" : "CPU only"}
      </Badge>
      <Badge variant="outline" className="text-muted-foreground">
        {spec.cpus ? `${spec.cpus} CPU` : "Unlimited CPU"}
      </Badge>
      <Badge variant="outline" className="text-muted-foreground">
        {spec.memory_mb ? `${spec.memory_mb} MB` : "Unlimited RAM"}
      </Badge>
      {spec.audio && (
        <Badge variant="outline" className="text-muted-foreground">
          <RiVolumeUpLine />
          Audio
        </Badge>
      )}
      {spec.microphone && (
        <Badge variant="outline" className="text-muted-foreground">
          <RiMicLine />
          Microphone
        </Badge>
      )}
      {spec.gamepad && (
        <Badge variant="outline" className="text-muted-foreground">
          <RiGamepadLine />
          Gamepad
        </Badge>
      )}
    </div>
  )
}

/** The main action for an environment's state: connect, retry its setup or start it. */
export function PrimaryAction({ item, className }: { item: EnvironmentItem; className?: string }) {
  const { id } = item.environment
  const [busy, setBusy] = React.useState(false)
  const run = (action: () => Promise<unknown>) => async () => {
    setBusy(true)
    await attempt(action)
    setBusy(false)
  }
  if (item.state === "running")
    return (
      <Button asChild className={className}>
        <a href={href({ page: "desktop", id })}>
          <RiComputerLine data-icon="inline-start" />
          Connect
        </a>
      </Button>
    )
  if (needsRetry(item))
    return (
      <Button
        variant="outline"
        className={className}
        disabled={busy}
        onClick={run(() => retrySetup(id))}
      >
        <RiResetLeftLine data-icon="inline-start" />
        Retry setup
      </Button>
    )
  return (
    <Button
      variant="outline"
      className={className}
      disabled={busy}
      onClick={run(() => environmentAction(id, "start"))}
    >
      <RiPlayCircleLine data-icon="inline-start" />
      Start
    </Button>
  )
}

export function InternetSwitch({ item, id }: { item: EnvironmentItem; id?: string }) {
  const { environment } = item
  const [busy, setBusy] = React.useState(false)
  return (
    <Switch
      id={id}
      checked={environment.spec.internet}
      disabled={busy}
      aria-label={`${environment.spec.name} internet access`}
      onCheckedChange={async (enabled) => {
        setBusy(true)
        await attempt(() => setInternet(environment.id, enabled))
        setBusy(false)
      }}
    />
  )
}

export function EnvironmentCard({
  item,
  profiles,
}: {
  item: EnvironmentItem
  profiles: ProfileItem[]
}) {
  const { environment } = item
  const { id, spec } = environment
  const running = item.state === "running"
  const status = environmentStatus(item)
  const drop = useFileDrop((files) => openDialog({ kind: "upload", id, files }))
  const switchId = React.useId()

  return (
    <Card
      {...drop.props}
      className={cn(
        "gap-4 pt-3 transition-colors hover:ring-foreground/20",
        drop.over && "ring-2 ring-primary"
      )}
    >
      <CardContent className="px-3">
        <EnvironmentPreview item={item} />
      </CardContent>
      <CardHeader>
        <CardTitle className="truncate text-base">
          <button
            type="button"
            className="truncate text-left hover:underline focus-visible:underline focus-visible:outline-none"
            onClick={() => openDialog({ kind: "details", id })}
          >
            {spec.name}
          </button>
        </CardTitle>
        <CardDescription className="truncate">
          {profileTitle(spec, environment.desktop?.title, profiles)}
        </CardDescription>
        <CardAction>
          <StatusBadge state={status.key} tone={status.tone} />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <SpecBadges spec={spec} />
        <div className="flex items-center justify-between">
          <Label htmlFor={switchId} className="text-muted-foreground">
            Internet
          </Label>
          <InternetSwitch item={item} id={switchId} />
        </div>
        {environment.last_error && (
          <Alert variant="destructive">
            <AlertDescription className="line-clamp-3">{environment.last_error}</AlertDescription>
          </Alert>
        )}
      </CardContent>
      <CardFooter className="mt-auto gap-2">
        <PrimaryAction item={item} className="flex-1" />
        <Tooltip>
          <TooltipTrigger asChild>
            <span tabIndex={running ? -1 : 0}>
              <Button
                variant="outline"
                size="icon"
                disabled={!running}
                onClick={() => openDialog({ kind: "upload", id })}
                aria-label={`Upload files to ${spec.name}`}
              >
                <RiUploadLine />
              </Button>
            </span>
          </TooltipTrigger>
          <TooltipContent>
            {running ? "Upload files" : "Start the environment to upload files"}
          </TooltipContent>
        </Tooltip>
        <EnvironmentMenu item={item} />
      </CardFooter>
    </Card>
  )
}
