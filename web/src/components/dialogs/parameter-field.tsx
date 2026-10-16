import * as React from "react"
import { cn } from "cn"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Slider } from "@/components/ui/slider"
import { Spinner } from "@/components/ui/spinner"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import {
  displayOf,
  newVolumeName,
  pickedChoices,
  sliderValue,
  toggleChoice,
  volumeChoices,
  volumeNameValid,
} from "@/lib/compose"
import { rich, t } from "@/lib/i18n"
import { queryClient, useVolumes } from "@/lib/queries"
import type { ComposeParameter } from "@/lib/types"

// Values never contain NUL, so these cannot collide with an option or a volume.
const NONE = "\u0000none"
const OTHER = "\u0000other"
const CREATE = "\u0000create"

/** Displays that need the form's full width. */
const WIDE = new Set(["multiline", "slider", "radio", "checkboxes"])

/**
 * One compose parameter as a form field: the control its type and display ask for, holding the
 * value as the script will read it. `optional` lets a required parameter stay empty, as its
 * default may.
 */
export function ParameterField({
  parameter,
  value,
  onChange,
  label,
  optional,
  hints,
  createVolumes,
}: {
  parameter: ComposeParameter
  value: string
  onChange: (value: string) => void
  /** The field's label; the parameter's own label or name when left out. */
  label?: string
  optional?: boolean
  /** Show the description and, under a parameter that has a label, the variable name. */
  hints?: boolean
  /** Let a volume parameter create the volume it is given. */
  createVolumes?: boolean
}) {
  const id = React.useId()
  const title = label ?? (parameter.label || parameter.name)
  const display = displayOf(parameter)
  const required = !optional && parameter.required
  const help = hints && parameter.description && (
    <FieldDescription className="whitespace-pre-line">{parameter.description}</FieldDescription>
  )
  const name = hints && parameter.label && (
    <FieldDescription className="font-mono text-xs">{parameter.name}</FieldDescription>
  )

  if (parameter.type === "boolean") {
    const on = value === "true"
    const set = (checked: boolean) => onChange(checked ? "true" : "false")
    return (
      <Field orientation="horizontal" className="self-start">
        {display === "checkbox" ? (
          <Checkbox id={id} checked={on} onCheckedChange={(checked) => set(checked === true)} />
        ) : (
          <Switch id={id} checked={on} onCheckedChange={set} />
        )}
        <div className="flex flex-col gap-0.5">
          <FieldLabel htmlFor={id}>{title}</FieldLabel>
          {help}
          {name}
        </div>
      </Field>
    )
  }

  return (
    <Field className={cn(WIDE.has(display) && "sm:col-span-2")}>
      <FieldLabel id={`${id}-label`} htmlFor={id}>
        {title}
      </FieldLabel>
      {parameter.type === "volume" ? (
        <VolumeControl
          parameter={parameter}
          value={value}
          onChange={onChange}
          id={id}
          required={required}
          creates={createVolumes ?? false}
        />
      ) : (
        <Control
          parameter={parameter}
          value={value}
          onChange={onChange}
          id={id}
          title={title}
          required={required}
        />
      )}
      {help}
      {name}
    </Field>
  )
}

/**
 * A named volume for the parameter's path: an existing one, one created here, or none, which
 * leaves the path inside the container.
 */
function VolumeControl({
  parameter,
  value,
  onChange,
  id,
  required,
  creates,
}: {
  parameter: ComposeParameter
  value: string
  onChange: (value: string) => void
  id: string
  required: boolean
  creates: boolean
}) {
  const volumes = useVolumes().data ?? []
  const [creating, setCreating] = React.useState(false)
  const [name, setName] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState("")
  const path = parameter.path ?? ""

  const pick = (next: string) => {
    setError("")
    setCreating(next === CREATE)
    if (next === CREATE) setName(newVolumeName(parameter, volumes))
    else onChange(next === NONE ? "" : next)
  }
  const create = async () => {
    const volume = name.trim()
    if (!volumeNameValid(volume)) {
      setError(t("volumes.namePattern"))
      return
    }
    setBusy(true)
    setError("")
    try {
      await api("volumes", { name: volume })
      await queryClient.invalidateQueries({ queryKey: ["volumes"] })
      toast.success(t("volumes.created", { name: volume }))
      setCreating(false)
      onChange(volume)
    } catch (failure) {
      setError((failure as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const selected = creating ? CREATE : value === "" ? (required ? "" : NONE) : value

  return (
    <>
      <Select required={required} value={selected} onValueChange={pick}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue placeholder={t("parameter.choose")} />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {!required && <SelectItem value={NONE}>{t("parameter.volume.none")}</SelectItem>}
            {volumeChoices(volumes, value).map((volume) => (
              <SelectItem key={volume} value={volume} className="font-mono">
                {volume}
              </SelectItem>
            ))}
            {creates && <SelectItem value={CREATE}>{t("parameter.volume.create")}</SelectItem>}
          </SelectGroup>
        </SelectContent>
      </Select>
      {creating && (
        // Its own button, not the dialog's: Enter here creates the volume, not the container.
        <div className="flex gap-2">
          <Input
            aria-label={t("parameter.volume.newName")}
            aria-invalid={!!error}
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter") return
              event.preventDefault()
              void create()
            }}
            className="font-mono"
            autoFocus
          />
          <Button type="button" variant="outline" disabled={busy} onClick={() => void create()}>
            {busy && <Spinner data-icon="inline-start" />}
            {t("common.create")}
          </Button>
        </div>
      )}
      {creating && error && <FieldError>{error}</FieldError>}
      {path && (
        <FieldDescription className="text-xs">
          {rich(value && !creating ? "parameter.volume.mountedAt" : "parameter.volume.stays", {
            path: <code className="font-mono">{path}</code>,
          })}
        </FieldDescription>
      )}
    </>
  )
}

function Control({
  parameter,
  value,
  onChange,
  id,
  title,
  required,
}: {
  parameter: ComposeParameter
  value: string
  onChange: (value: string) => void
  id: string
  title: string
  required: boolean
}) {
  const display = displayOf(parameter)
  const options = (parameter.options ?? []).filter((option) => option.value)
  const listed = options.some((option) => option.value === value)
  // A value that is not an option was typed as another one.
  const [other, setOther] = React.useState(value !== "" && !listed)
  const custom = parameter.type === "choice" && parameter.allow_custom && other

  if (parameter.type === "number") {
    const { min, max, step } = parameter
    if (display === "slider" && typeof min === "number" && typeof max === "number")
      return (
        <div className="flex items-center gap-3">
          <Slider
            id={id}
            thumbLabel={title}
            min={min}
            max={max}
            step={step ?? 1}
            value={[value === "" ? min : Number(value)]}
            onValueChange={([position]) => onChange(sliderValue(position, step))}
          />
          <span className="min-w-14 text-right font-mono text-xs text-muted-foreground tabular-nums">
            {value === "" ? t("parameter.notSet") : value}
          </span>
        </div>
      )
    return (
      <Input
        id={id}
        type="number"
        inputMode="decimal"
        min={min ?? undefined}
        max={max ?? undefined}
        step={step ?? "any"}
        required={required}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    )
  }

  if (parameter.type === "choices") {
    const picked = new Set(pickedChoices(value))
    return (
      <div role="group" aria-labelledby={`${id}-label`} className="grid gap-2 sm:grid-cols-2">
        {options.map((option) => (
          <Label key={option.value} className="flex items-center gap-2 font-normal">
            <Checkbox
              aria-label={option.label || option.value}
              checked={picked.has(option.value)}
              onCheckedChange={(checked) =>
                onChange(toggleChoice(value, option.value, checked === true, options))
              }
            />
            {option.label || option.value}
          </Label>
        ))}
      </div>
    )
  }

  if (parameter.type === "choice") {
    const selected = custom ? OTHER : value === "" ? (required ? "" : NONE) : value
    const pick = (next: string) => {
      setOther(next === OTHER)
      onChange(next === OTHER || next === NONE ? "" : next)
    }
    const otherInput = custom && (
      <Input
        aria-label={t("parameter.otherOf", { name: title })}
        placeholder={t("parameter.otherValue")}
        required={required}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    )
    if (display === "radio")
      return (
        <>
          <RadioGroup
            id={id}
            aria-labelledby={`${id}-label`}
            required={required}
            value={selected}
            onValueChange={pick}
          >
            {!required && <RadioChoice value={NONE} label={t("common.none")} />}
            {options.map((option) => (
              <RadioChoice
                key={option.value}
                value={option.value}
                label={option.label || option.value}
              />
            ))}
            {parameter.allow_custom && (
              <RadioChoice value={OTHER} label={t("parameter.otherChoice")} />
            )}
          </RadioGroup>
          {otherInput}
        </>
      )
    return (
      <>
        <Select required={required} value={selected} onValueChange={pick}>
          <SelectTrigger id={id} className="w-full">
            <SelectValue placeholder={t("parameter.choose")} />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {!required && <SelectItem value={NONE}>{t("common.none")}</SelectItem>}
              {options.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label || option.value}
                </SelectItem>
              ))}
              {parameter.allow_custom && (
                <SelectItem value={OTHER}>{t("parameter.otherEllipsis")}</SelectItem>
              )}
            </SelectGroup>
          </SelectContent>
        </Select>
        {otherInput}
      </>
    )
  }

  if (display === "multiline")
    return (
      <Textarea
        id={id}
        rows={4}
        required={required}
        spellCheck={false}
        autoComplete="off"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="font-mono text-xs"
      />
    )
  return (
    <Input
      id={id}
      type={parameter.secret ? "password" : "text"}
      autoComplete={parameter.secret ? "new-password" : "off"}
      required={required}
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  )
}

function RadioChoice({ value, label }: { value: string; label: string }) {
  return (
    <Label className="flex items-center gap-2 font-normal">
      <RadioGroupItem value={value} aria-label={label} />
      {label}
    </Label>
  )
}
