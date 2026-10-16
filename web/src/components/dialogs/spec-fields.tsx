import * as React from "react"
import { RiAddLine, RiCloseLine } from "@remixicon/react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Field,
  FieldContent,
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
import { Switch } from "@/components/ui/switch"
import type { ContainerSpec, Mount, Settings, Volume } from "@/lib/types"

const RESOLUTIONS = ["1920x1080", "1600x900", "1280x720", "2560x1440", "3840x2160"]
const FLAGS = ["gpu", "internet", "sudo", "audio", "microphone", "gamepad"] as const
type Flag = (typeof FLAGS)[number]

export type MountDraft = Mount & { key: number }
export type SpecDraft = {
  resolution: string
  shm_mb: string
  cpus: string
  memory_mb: string
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
    mounts: draft.mounts.map(({ volume, target, readonly }) => ({ volume, target, readonly })),
  }
}

const flagCopy: Record<Flag, { label: string; description: string }> = {
  gpu: { label: "GPU", description: "Hardware rendering on the host GPU." },
  internet: { label: "Internet", description: "Outbound network access." },
  sudo: { label: "Sudo", description: "Administrator rights for the desktop user." },
  audio: { label: "Audio", description: "Desktop sound in the browser." },
  microphone: { label: "Microphone", description: "The browser microphone, inside the desktop." },
  gamepad: { label: "Gamepad", description: "A virtual controller device." },
}

/**
 * The settings shared by the create and settings dialogs. `spec` is the container being changed, if
 * any: a feature the installation switched off cannot be turned on, but a container that has it can
 * still turn it off.
 */
export function SpecFields({
  draft,
  onChange,
  settings,
  volumes,
  spec,
}: {
  draft: SpecDraft
  onChange: (draft: SpecDraft) => void
  settings?: Settings
  volumes: Volume[]
  spec?: ContainerSpec
}) {
  const id = React.useId()
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
  const set = (patch: Partial<SpecDraft>) => onChange({ ...draft, ...patch })
  const toggle = (flag: Flag, value: boolean) => {
    const patch: Partial<SpecDraft> = { [flag]: value }
    // The microphone travels over the audio connection.
    if (flag === "microphone" && value) patch.audio = true
    if (flag === "audio" && !value) patch.microphone = false
    set(patch)
  }
  const setMount = (key: number, patch: Partial<Mount>) =>
    set({
      mounts: draft.mounts.map((mount) => (mount.key === key ? { ...mount, ...patch } : mount)),
    })

  return (
    <FieldGroup>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${id}-resolution`}>Resolution</FieldLabel>
          <Select value={draft.resolution} onValueChange={(resolution) => set({ resolution })}>
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
          <FieldLabel htmlFor={`${id}-shm`}>Shared memory (MB)</FieldLabel>
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
          <FieldLabel htmlFor={`${id}-cpus`}>CPU limit</FieldLabel>
          <Input
            id={`${id}-cpus`}
            type="number"
            min={0.1}
            step={0.1}
            placeholder="Unlimited"
            value={draft.cpus}
            onChange={(event) => set({ cpus: event.target.value })}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-memory`}>Memory limit (MB)</FieldLabel>
          <Input
            id={`${id}-memory`}
            type="number"
            min={256}
            placeholder="Unlimited"
            value={draft.memory_mb}
            onChange={(event) => set({ memory_mb: event.target.value })}
          />
        </Field>
      </div>
      <FieldSet>
        <FieldLegend variant="label">Features</FieldLegend>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {FLAGS.map((flag) => {
            const disabled = !enabled[flag] && !spec?.[flag]
            return (
              <FieldLabel key={flag} htmlFor={`${id}-${flag}`}>
                <Field orientation="horizontal" data-disabled={disabled}>
                  <FieldContent>
                    <span className="font-medium">{flagCopy[flag].label}</span>
                    <FieldDescription className="text-xs">
                      {disabled ? "Turned off for this installation." : flagCopy[flag].description}
                    </FieldDescription>
                  </FieldContent>
                  <Switch
                    id={`${id}-${flag}`}
                    checked={draft[flag]}
                    disabled={disabled}
                    onCheckedChange={(value) => toggle(flag, value)}
                  />
                </Field>
              </FieldLabel>
            )
          })}
        </div>
      </FieldSet>
      <FieldSet>
        <div className="flex items-center justify-between">
          <FieldLegend variant="label" className="mb-0">
            Volumes
          </FieldLegend>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => {
              if (!volumes.length) {
                toast.error("Create a volume first.")
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
            Mount
          </Button>
        </div>
        {draft.mounts.map((mount) => {
          const names = volumes.map((volume) => volume.Name)
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
                <SelectTrigger aria-label="Volume" className="w-full">
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
                aria-label="Mount path"
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
                Read-only
              </Label>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label="Remove mount"
                onClick={() =>
                  set({ mounts: draft.mounts.filter((item) => item.key !== mount.key) })
                }
              >
                <RiCloseLine />
              </Button>
            </div>
          )
        })}
        <FieldDescription className="text-xs">
          Mounted volumes are excluded from snapshots.
        </FieldDescription>
      </FieldSet>
    </FieldGroup>
  )
}
