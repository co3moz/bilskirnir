import * as React from "react"
import NoVNC from "@novnc/novnc"
import {
  RiArrowLeftLine,
  RiPlayCircleLine,
  RiClipboardLine,
  RiFullscreenLine,
  RiMicLine,
  RiMicOffLine,
  RiFullscreenExitLine,
  RiRestartLine,
  RiDownloadLine,
  RiUploadLine,
  RiVolumeUpLine,
  RiVolumeMuteLine,
} from "@remixicon/react"
import { toast } from "sonner"
import { cn } from "cn"

import { HumanRequestStrip } from "@/components/human-requests"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty"
import { Separator } from "@/components/ui/separator"
import { Spinner } from "@/components/ui/spinner"
import { Textarea } from "@/components/ui/textarea"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useFileDrop } from "@/hooks/use-file-drop"
import { attempt, containerAction } from "@/lib/actions"
import { wsUrl } from "@/lib/api"
import { openDialog } from "@/lib/dialogs"
import { t } from "@/lib/i18n"
import { DesktopMedia, type MediaState } from "@/lib/media"
import { useContainers } from "@/lib/queries"
import { href } from "@/lib/route"

// noVNC 1.5 is CommonJS that sets `exports.default`. This package is `"type": "module"`, so the
// bundler applies Node's rules and hands over the exports object as the default import; take the
// class from it, and still accept the class itself should the interop change.
type RFB = NoVNC
const RFB: typeof NoVNC = (NoVNC as unknown as { default?: typeof NoVNC }).default ?? NoVNC

type Connection = "connecting" | "connected" | "disconnected" | "failed"
const connectionText = (connection: Connection) => t(`desktop.connection.${connection}`)

/** A compact toolbar button: an icon with its label on wide screens and a tooltip on narrow ones. */
function ToolButton({
  label,
  icon,
  className,
  ...props
}: React.ComponentProps<typeof Button> & { label: string; icon: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="sm" aria-label={label} className={className} {...props}>
          {icon}
          <span className="hidden xl:inline">{label}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent className="xl:hidden">{label}</TooltipContent>
    </Tooltip>
  )
}

function useMedia() {
  const [state, setState] = React.useState<MediaState>({
    output: false,
    input: false,
    outputPending: false,
    inputPending: false,
  })
  const [media] = React.useState(
    () =>
      new DesktopMedia({
        createContext: () => new AudioContext({ sampleRate: 48000 }),
        createNode: (ctx, name, options) => new AudioWorkletNode(ctx, name, options),
        createSocket: (url) => new WebSocket(url),
        url: wsUrl,
        // Browsers offer the microphone only on HTTPS or localhost.
        getUserMedia: navigator.mediaDevices?.getUserMedia?.bind(navigator.mediaDevices),
        onError: (message) => toast.error(message),
        onState: setState,
      })
  )
  React.useEffect(() => () => media.close(), [media])
  return { media, state }
}

/** The remote desktop over VNC, with audio, microphone, clipboard and uploads. */
export function DesktopPage({ id }: { id: string }) {
  const containers = useContainers(false)
  const item = containers.data?.find((entry) => entry.id === id)
  const running = item?.state === "running"
  // A stopped desktop is watched until it runs, so starting it here connects.
  useContainers(!!item && !running)

  const view = React.useRef<HTMLElement>(null)
  const screen = React.useRef<HTMLDivElement>(null)
  const rfb = React.useRef<RFB | null>(null)
  const [generation, setGeneration] = React.useState(0)
  // Each connection attempt reports under its own key; a new attempt starts out connecting.
  const attemptKey = `${id}:${generation}:${running}`
  const [status, setStatus] = React.useState<{ key: string; value: Connection } | null>(null)
  const connection = status?.key === attemptKey ? status.value : "connecting"
  const [remoteClipboard, setRemoteClipboard] = React.useState("")
  const [clipboard, setClipboard] = React.useState<string | null>(null)
  const [fullscreen, setFullscreen] = React.useState(false)
  const { media, state: audio } = useMedia()
  const drop = useFileDrop((files) => openDialog({ kind: "upload", id, files }))

  React.useEffect(() => {
    if (!running || !screen.current) return
    media.select(id)
    const report = (value: Connection) => setStatus({ key: attemptKey, value })
    let current: RFB
    try {
      current = new RFB(screen.current, wsUrl(`containers/${encodeURIComponent(id)}/vnc`))
    } catch (error) {
      toast.error((error as Error).message)
      // Reported like the connection's own events, after the effect has run.
      queueMicrotask(() => report("failed"))
      return
    }
    rfb.current = current
    current.scaleViewport = true
    current.resizeSession = true
    current.showDotCursor = true
    current.focusOnClick = true
    // Events from a connection that was replaced or closed must not touch the current one.
    const live = () => rfb.current === current
    current.addEventListener("connect", () => live() && report("connected"))
    current.addEventListener("disconnect", (event) => {
      if (!live()) return
      media.close()
      report((event as CustomEvent<{ clean: boolean }>).detail.clean ? "disconnected" : "failed")
    })
    current.addEventListener("clipboard", (event) => {
      if (!live()) return
      const text = (event as CustomEvent<{ text: string }>).detail.text
      setRemoteClipboard(text)
      setClipboard((open) => (open === null ? null : text))
    })
    return () => {
      if (rfb.current === current) rfb.current = null
      current.disconnect()
      media.close()
    }
  }, [id, running, attemptKey, media])

  React.useEffect(() => {
    const update = () => setFullscreen(!!document.fullscreenElement)
    document.addEventListener("fullscreenchange", update)
    return () => document.removeEventListener("fullscreenchange", update)
  }, [])

  const name = item?.spec.name ?? t("nav.desktop")
  const spec = item?.spec

  return (
    <section ref={view} className="flex h-dvh flex-col bg-background">
      <header className="flex h-12 shrink-0 items-center gap-1 border-b bg-sidebar px-2 sm:gap-2 sm:px-3">
        <Button variant="ghost" size="sm" asChild>
          <a href={href({ page: "containers" })}>
            <RiArrowLeftLine data-icon="inline-start" />
            <span className="hidden md:inline">{t("nav.containers")}</span>
            <span className="sr-only md:hidden">{t("desktop.back")}</span>
          </a>
        </Button>
        <Separator
          orientation="vertical"
          className="mx-1 data-vertical:h-4 data-vertical:self-center"
        />
        <h1 className="min-w-0 truncate text-sm font-medium">{name}</h1>
        {running && (
          <span
            className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground"
            role="status"
          >
            <span
              aria-hidden
              className={cn(
                "size-2 rounded-full",
                connection === "connected"
                  ? "bg-success"
                  : connection === "connecting"
                    ? "animate-pulse bg-warning"
                    : "bg-muted-foreground"
              )}
            />
            <span className="hidden sm:inline">{connectionText(connection)}</span>
          </span>
        )}
        <div className="ml-auto flex items-center gap-0.5">
          {running && (
            <>
              <ToolButton
                label={t("uploads.title")}
                icon={<RiUploadLine />}
                onClick={() => openDialog({ kind: "upload", id })}
              />
              <ToolButton
                label={t("downloads.title")}
                icon={<RiDownloadLine />}
                onClick={() => openDialog({ kind: "download", id })}
              />
              <ToolButton
                label={t("desktop.clipboard")}
                icon={<RiClipboardLine />}
                onClick={() => setClipboard(remoteClipboard)}
              />
              {spec?.audio && (
                <ToolButton
                  label={
                    audio.output
                      ? t("desktop.mute")
                      : audio.outputPending
                        ? t("desktop.startingAudio")
                        : t("spec.audio")
                  }
                  icon={
                    audio.output ? (
                      <RiVolumeUpLine className="text-primary" />
                    ) : (
                      <RiVolumeMuteLine />
                    )
                  }
                  aria-pressed={audio.output}
                  onClick={() => attempt(() => media.toggleOutput())}
                />
              )}
              {spec?.microphone && (
                <ToolButton
                  label={
                    audio.input
                      ? t("desktop.micOff")
                      : audio.inputPending
                        ? t("desktop.micPermission")
                        : t("spec.microphone")
                  }
                  icon={audio.input ? <RiMicLine className="text-primary" /> : <RiMicOffLine />}
                  aria-pressed={audio.input}
                  onClick={() => attempt(() => media.toggleMic())}
                />
              )}
              <ToolButton
                label={t("desktop.reconnect")}
                icon={<RiRestartLine />}
                onClick={() => setGeneration((value) => value + 1)}
              />
            </>
          )}
          <ToolButton
            label={fullscreen ? t("desktop.exitFullscreen") : t("desktop.fullscreen")}
            icon={fullscreen ? <RiFullscreenExitLine /> : <RiFullscreenLine />}
            onClick={() =>
              attempt(() =>
                document.fullscreenElement
                  ? document.exitFullscreen()
                  : view.current!.requestFullscreen()
              )
            }
          />
        </div>
      </header>
      <HumanRequestStrip container={id} />
      <div
        {...drop.props}
        className={cn(
          "relative min-h-0 flex-1 bg-[#0a0a0a]",
          drop.over && "outline-3 -outline-offset-3 outline-primary"
        )}
      >
        {running ? (
          <div ref={screen} className="vnc-screen size-full overflow-hidden" />
        ) : (
          <Empty className="h-full text-white/80">
            <EmptyHeader>
              <EmptyTitle className="text-base">
                {containers.isPending
                  ? t("desktop.loading")
                  : item
                    ? t("desktop.notRunning", { name })
                    : t("containers.gone")}
              </EmptyTitle>
              {item && <EmptyDescription>{t("desktop.startToConnect")}</EmptyDescription>}
            </EmptyHeader>
            <EmptyContent>
              {containers.isPending ? (
                <Spinner />
              ) : item ? (
                <Button onClick={() => attempt(() => containerAction(id, "start"))}>
                  <RiPlayCircleLine data-icon="inline-start" />
                  {t("containers.start")}
                </Button>
              ) : (
                <Button variant="outline" asChild>
                  <a href={href({ page: "containers" })}>{t("desktop.back")}</a>
                </Button>
              )}
            </EmptyContent>
          </Empty>
        )}
      </div>
      <Dialog open={clipboard !== null} onOpenChange={(open) => !open && setClipboard(null)}>
        <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("desktop.clipboard")}</DialogTitle>
            <DialogDescription>{t("desktop.clipboardHint")}</DialogDescription>
          </DialogHeader>
          <Textarea
            rows={8}
            aria-label={t("desktop.clipboardText")}
            placeholder={t("desktop.clipboardPlaceholder")}
            value={clipboard ?? ""}
            onChange={(event) => setClipboard(event.target.value)}
            className="max-h-[500px] overflow-y-auto"
          />
          <DialogFooter>
            <Button
              onClick={() => {
                rfb.current?.clipboardPasteFrom(clipboard ?? "")
                setClipboard(null)
              }}
            >
              {t("desktop.send")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  )
}
