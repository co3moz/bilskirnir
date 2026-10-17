import * as React from "react"
import { flushSync } from "react-dom"
import {
  RiAddLine,
  RiArrowDownSLine,
  RiCloseLine,
  RiCpuLine,
  RiGamepadLine,
  RiGlobalLine,
  RiMicLine,
  RiShieldUserLine,
  RiVolumeUpLine,
  type RemixiconComponentType,
} from "@remixicon/react"
import { cn } from "cn"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Toggle } from "@/components/ui/toggle"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { formatList, formatNumber, plural, t } from "@/lib/i18n"
import { formatPorts, parsePorts, PORTS_PATTERN } from "@/lib/ports"
import type { ContainerSpec, Mount, Settings, Volume } from "@/lib/types"
import { mountableVolumes } from "@/lib/volume-files"

const RESOLUTIONS = ["1920x1080", "1600x900", "1280x720", "2560x1440", "3840x2160"]
const FLAGS = ["gpu", "internet", "sudo", "audio", "microphone", "gamepad"] as const
type Flag = (typeof FLAGS)[number]

const FLAG_ICONS: Record<Flag, RemixiconComponentType> = {
  gpu: RiCpuLine,
  internet: RiGlobalLine,
  sudo: RiShieldUserLine,
  audio: RiVolumeUpLine,
  microphone: RiMicLine,
  gamepad: RiGamepadLine,
}

export type MountDraft = Mount & { key: number }
export type SpecDraft = {
  resolution: string
  shm_mb: string
  cpus: string
  memory_mb: string
  ports: string
  mounts: MountDraft[]
} & Record<Flag, boolean>

let mountKey = 0

export function specDraft(spec: Partial<ContainerSpec>, settings?: Settings): SpecDraft {
  return {
    resolution: spec.resolution || RESOLUTIONS[0],
    shm_mb: String(spec.shm_mb ?? 512),
    cpus: spec.cpus ? String(spec.cpus) : "",
    memory_mb: spec.memory_mb ? String(spec.memory_mb) : "",
    gpu: spec.gpu ?? settings?.gpu_enabled !== false,
    internet: spec.internet ?? true,
    sudo: spec.sudo ?? true,
    audio: spec.audio ?? false,
    microphone: spec.microphone ?? false,
    gamepad: spec.gamepad ?? false,
    ports: formatPorts(spec.ports),
    mounts: (spec.mounts ?? []).map((mount) => ({ ...mount, key: ++mountKey })),
  }
}

/** The request body for the settings shared by creation and later changes. */
export function specBody(draft: SpecDraft) {
  const number = (value: string) => (value ? Number(value) : null)
  return {
    resolution: draft.resolution,
    shm_mb: number(draft.shm_mb),
    cpus: number(draft.cpus),
    memory_mb: number(draft.memory_mb),
    ...Object.fromEntries(FLAGS.map((flag) => [flag, draft[flag]])),
    ports: parsePorts(draft.ports),
    mounts: draft.mounts.map(({ volume, target, readonly }) => ({ volume, target, readonly })),
  }
}

const flagLabel = (flag: Flag) => t(`spec.flag.${flag}`)
const flagDescription = (flag: Flag) => t(`spec.flag.${flag}.description`)

/** One line for the closed advanced section, so its values are known without opening it. */
function advancedSummary(draft: SpecDraft) {
  const parts = [
    draft.resolution,
    draft.cpus
      ? t("spec.cpus", { count: formatNumber(Number(draft.cpus)) })
      : t("spec.unlimitedCpu"),
    draft.memory_mb
      ? t("spec.memory", { count: formatNumber(Number(draft.memory_mb)) })
      : t("spec.unlimitedMemory"),
  ]
  const ports = parsePorts(draft.ports).length
  if (ports) parts.push(plural(ports, { one: "spec.ports.one", other: "spec.ports.other" }))
  if (draft.mounts.length)
    parts.push(plural(draft.mounts.length, { one: "spec.mounts.one", other: "spec.mounts.other" }))
  return parts.join(" · ")
}

/**
 * The settings shared by the create and settings dialogs: the features as toggle buttons, then
 * resources and volumes in a section that starts closed unless `advancedOpen`. `spec` is the
 * container being changed, if any: a feature the installation switched off cannot be turned on, but
 * a container that has it can still turn it off.
 */
export function SpecFields({
  draft,
  onChange,
  settings,
  volumes,
  spec,
  advancedOpen = false,
}: {
  draft: SpecDraft
  onChange: (draft: SpecDraft) => void
  settings?: Settings
  volumes: Volume[]
  spec?: ContainerSpec
  advancedOpen?: boolean
}) {
  const id = React.useId()
  const [open, setOpen] = React.useState(advancedOpen)
  const resolutions = RESOLUTIONS.includes(draft.resolution)
    ? RESOLUTIONS
    : [...RESOLUTIONS, draft.resolution]
  const enabled: Record<Flag, boolean> = {
    gpu: settings?.gpu_enabled ?? true,
    audio: settings?.audio_enabled ?? true,
    microphone: settings?.microphone_enabled ?? true,
    internet: true,
    sudo: true,
    gamepad: true,
  }
  const blocked = FLAGS.filter((flag) => !enabled[flag] && !spec?.[flag])
  const set = (patch: Partial<SpecDraft>) => onChange({ ...draft, ...patch })
  const toggle = (flag: Flag, value: boolean) => {
    const patch: Partial<SpecDraft> = { [flag]: value }
    // The microphone travels over the audio connection.
    if (flag === "microphone" && value) patch.audio = true
    if (flag === "audio" && !value) patch.microphone = false
    set(patch)
  }

  return (
    <FieldGroup>
      <FieldSet className="gap-3">
        <FieldLegend variant="label">{t("spec.features")}</FieldLegend>
        <div className="flex flex-wrap gap-2">
          {FLAGS.map((flag) => {
            const Icon = FLAG_ICONS[flag]
            const disabled = blocked.includes(flag)
            const button = (
              <Toggle
                key={flag}
                variant="outline"
                pressed={draft[flag]}
                disabled={disabled}
                aria-describedby={disabled ? `${id}-blocked` : undefined}
                onPressedChange={(value) => toggle(flag, value)}
                // The tooltip trigger overrides data-state, so aria-pressed carries the style.
                className="text-muted-foreground aria-pressed:border-primary aria-pressed:text-foreground"
              >
                <Icon data-icon="inline-start" />
                {flagLabel(flag)}
              </Toggle>
            )
            // A disabled button gets no hover or focus, so the line below says why instead.
            if (disabled) return button
            return (
              <Tooltip key={flag}>
                <TooltipTrigger asChild>{button}</TooltipTrigger>
                <TooltipContent>{flagDescription(flag)}</TooltipContent>
              </Tooltip>
            )
          })}
        </div>
        {blocked.length > 0 && (
          <FieldDescription id={`${id}-blocked`} className="text-xs">
            {t("spec.turnedOffList", { features: formatList(blocked.map(flagLabel)) })}
          </FieldDescription>
        )}
      </FieldSet>
      {/* Not a Radix collapsible: that unmounts its content when closed, and the browser could
          then no longer check the fields inside before submitting. */}
      <div
        className="rounded-2xl border"
        // The browser points at a field it refuses only when the field is visible, so a closed
        // section opens before the browser looks for the field to focus.
        onInvalidCapture={() => {
          if (!open) flushSync(() => setOpen(true))
        }}
      >
        <button
          type="button"
          aria-expanded={open}
          aria-controls={`${id}-advanced`}
          onClick={() => setOpen(!open)}
          className="group flex w-full items-center gap-3 rounded-2xl px-4 py-3 text-left outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
        >
          <span className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="text-sm font-medium group-hover:underline">{t("spec.advanced")}</span>
            {!open && (
              <span className="text-xs text-muted-foreground">{advancedSummary(draft)}</span>
            )}
          </span>
          <RiArrowDownSLine
            className={cn(
              "size-4 shrink-0 text-muted-foreground transition-transform",
              open && "rotate-180"
            )}
          />
        </button>
        <div id={`${id}-advanced`} hidden={!open} className="border-t p-4">
          <FieldGroup>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor={`${id}-resolution`}>{t("spec.resolution")}</FieldLabel>
                <Select
                  value={draft.resolution}
                  onValueChange={(resolution) => set({ resolution })}
                >
                  <SelectTrigger id={`${id}-resolution`} className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {resolutions.map((value) => (
                        <SelectItem key={value} value={value}>
                          {value}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-shm`}>{t("spec.sharedMemory")}</FieldLabel>
                <Input
                  id={`${id}-shm`}
                  type="number"
                  min={64}
                  required
                  value={draft.shm_mb}
                  onChange={(event) => set({ shm_mb: event.target.value })}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-cpus`}>{t("spec.cpuLimit")}</FieldLabel>
                <Input
                  id={`${id}-cpus`}
                  type="number"
                  min={0.1}
                  step={0.1}
                  placeholder={t("spec.unlimited")}
                  value={draft.cpus}
                  onChange={(event) => set({ cpus: event.target.value })}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-memory`}>{t("spec.memoryLimit")}</FieldLabel>
                <Input
                  id={`${id}-memory`}
                  type="number"
                  min={256}
                  placeholder={t("spec.unlimited")}
                  value={draft.memory_mb}
                  onChange={(event) => set({ memory_mb: event.target.value })}
                />
              </Field>
              <Field className="sm:col-span-2">
                <FieldLabel htmlFor={`${id}-ports`}>{t("spec.proxiedPorts")}</FieldLabel>
                <Input
                  id={`${id}-ports`}
                  inputMode="numeric"
                  pattern={PORTS_PATTERN}
                  placeholder="8090, 6400"
                  aria-describedby={`${id}-ports-hint`}
                  value={draft.ports}
                  onChange={(event) => set({ ports: event.target.value })}
                  className="font-mono"
                />
                <FieldDescription id={`${id}-ports-hint`} className="text-xs">
                  {t("spec.proxiedPortsHint")}
                </FieldDescription>
              </Field>
            </div>
            <Mounts draft={draft} set={set} volumes={mountableVolumes(volumes)} />
          </FieldGroup>
        </div>
      </div>
    </FieldGroup>
  )
}

/**
 * The volumes mounted into the container, each with its path and access. `volumes` are the ones a
 * container may mount.
 */
function Mounts({
  draft,
  set,
  volumes,
}: {
  draft: SpecDraft
  set: (patch: Partial<SpecDraft>) => void
  volumes: Volume[]
}) {
  const setMount = (key: number, patch: Partial<Mount>) =>
    set({
      mounts: draft.mounts.map((mount) => (mount.key === key ? { ...mount, ...patch } : mount)),
    })

  return (
    <FieldSet>
      <div className="flex items-center justify-between">
        <FieldLegend variant="label" className="mb-0">
          {t("nav.volumes")}
        </FieldLegend>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => {
            if (!volumes.length) {
              toast.error(t("spec.volumeFirst"))
              return
            }
            set({
              mounts: [
                ...draft.mounts,
                { key: ++mountKey, volume: volumes[0].Name, target: "", readonly: false },
              ],
            })
          }}
        >
          <RiAddLine data-icon="inline-start" />
          {t("spec.mount")}
        </Button>
      </div>
      {draft.mounts.map((mount) => {
        const names = volumes.map((volume) => volume.Name)
        // A container's existing mount stays visible, so it can be removed or changed.
        if (!names.includes(mount.volume)) names.push(mount.volume)
        return (
          <div
            key={mount.key}
            className="grid grid-cols-[1fr_auto_auto] items-center gap-2 sm:grid-cols-[10rem_1fr_auto_auto]"
          >
            <Select
              value={mount.volume}
              onValueChange={(volume) => setMount(mount.key, { volume })}
            >
              <SelectTrigger aria-label={t("spec.volume")} className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {names.map((name) => (
                    <SelectItem key={name} value={name}>
                      {name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            <Input
              aria-label={t("spec.mountPath")}
              placeholder="/workspace/project"
              required
              value={mount.target}
              onChange={(event) => setMount(mount.key, { target: event.target.value })}
              className="col-span-3 row-start-2 font-mono sm:col-span-1 sm:row-start-auto"
            />
            <Label className="flex items-center gap-2 text-xs font-normal">
              <Checkbox
                checked={mount.readonly}
                onCheckedChange={(value) => setMount(mount.key, { readonly: !!value })}
              />
              {t("spec.readOnly")}
            </Label>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t("spec.removeMount")}
              onClick={() => set({ mounts: draft.mounts.filter((item) => item.key !== mount.key) })}
            >
              <RiCloseLine />
            </Button>
          </div>
        )
      })}
      <FieldDescription className="text-xs">{t("spec.mountsHint")}</FieldDescription>
    </FieldSet>
  )
}
