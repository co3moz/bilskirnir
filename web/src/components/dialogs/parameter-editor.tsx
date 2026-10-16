import * as React from "react"
import { RiAddLine, RiCloseLine } from "@remixicon/react"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  DISPLAY_LABELS,
  DISPLAYS,
  draftParameter,
  newOption,
  PARAMETER_NAME,
  TYPE_LABELS,
  withOptions,
  withType,
  type OptionDraft,
  type ParameterDraft,
} from "@/lib/compose"
import type { ParameterDisplay, ParameterType } from "@/lib/types"
import { ParameterField } from "./parameter-field"

const TYPES = Object.keys(DISPLAYS) as ParameterType[]
const small = "text-xs text-muted-foreground"

/** One parameter of the compose editor, with the fields its type has. */
export function ParameterEditor({
  parameter,
  onChange,
  onRemove,
}: {
  parameter: ParameterDraft
  onChange: (parameter: ParameterDraft) => void
  onRemove: () => void
}) {
  const id = React.useId()
  const set = (patch: Partial<ParameterDraft>) => onChange({ ...parameter, ...patch })
  const displays = DISPLAYS[parameter.type]
  const options = parameter.type === "choice" || parameter.type === "choices"

  return (
    <div className="flex flex-col gap-3 rounded-xl border p-3">
      <div className="grid grid-cols-[1fr_auto] gap-2 sm:grid-cols-[1fr_1fr_auto]">
        <Input
          aria-label="Variable name"
          placeholder="UNITY_EMAIL"
          required
          maxLength={64}
          pattern={PARAMETER_NAME}
          title="Capital letters, digits and underscores, starting with a letter"
          value={parameter.name}
          onChange={(event) => set({ name: event.target.value.toUpperCase() })}
          className="font-mono"
        />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label={`Remove ${parameter.name || "parameter"}`}
          className="sm:col-start-3"
          onClick={onRemove}
        >
          <RiCloseLine />
        </Button>
        <Input
          aria-label="Label"
          placeholder="Unity email"
          maxLength={100}
          value={parameter.label}
          onChange={(event) => set({ label: event.target.value })}
          className="col-span-2 sm:col-span-1 sm:col-start-2 sm:row-start-1"
        />
      </div>
      <Textarea
        aria-label="Description"
        placeholder="Help shown with the field, such as where to find the value"
        rows={2}
        maxLength={500}
        value={parameter.description}
        onChange={(event) => set({ description: event.target.value })}
        className="min-h-0"
      />
      <div className="grid grid-cols-2 gap-2">
        <Field className="gap-1.5">
          <FieldLabel htmlFor={`${id}-type`} className={small}>
            Type
          </FieldLabel>
          <Select
            value={parameter.type}
            onValueChange={(type) => onChange(withType(parameter, type as ParameterType))}
          >
            <SelectTrigger id={`${id}-type`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {TYPES.map((type) => (
                  <SelectItem key={type} value={type}>
                    {TYPE_LABELS[type]}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field className="gap-1.5">
          <FieldLabel htmlFor={`${id}-display`} className={small}>
            Shown as
          </FieldLabel>
          <Select
            value={parameter.display}
            disabled={displays.length === 1}
            onValueChange={(display) => set({ display: display as ParameterDisplay })}
          >
            <SelectTrigger id={`${id}-display`} className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {displays.map((display) => (
                  <SelectItem key={display} value={display}>
                    {DISPLAY_LABELS[display]}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
      </div>
      {parameter.type === "number" && (
        <div className="grid grid-cols-3 gap-2">
          {(["min", "max", "step"] as const).map((field) => (
            <Field key={field} className="gap-1.5">
              <FieldLabel htmlFor={`${id}-${field}`} className={small}>
                {{ min: "Minimum", max: "Maximum", step: "Step" }[field]}
              </FieldLabel>
              <Input
                id={`${id}-${field}`}
                type="number"
                step="any"
                min={field === "step" ? 0 : undefined}
                required={parameter.display === "slider" && field !== "step"}
                placeholder={field === "step" ? "Any" : "None"}
                value={parameter[field]}
                onChange={(event) => set({ [field]: event.target.value })}
              />
            </Field>
          ))}
        </div>
      )}
      {options && (
        <OptionsEditor
          options={parameter.options}
          onChange={(next) => onChange(withOptions(parameter, next))}
        />
      )}
      {parameter.secret ? (
        <Field className="gap-1.5">
          <FieldLabel htmlFor={`${id}-default`} className={small}>
            Default
          </FieldLabel>
          <Input id={`${id}-default`} disabled placeholder="Secrets have no default" />
        </Field>
      ) : (
        // A new type or display starts the preview over, with nothing typed as another value.
        <ParameterField
          key={`${parameter.type}-${parameter.display}`}
          parameter={draftParameter(parameter)}
          label={parameter.type === "boolean" ? "On by default" : "Default"}
          optional
          value={parameter.default}
          onChange={(value) => set({ default: value })}
        />
      )}
      <div className="flex flex-wrap items-center gap-4">
        <Label className="flex items-center gap-2 text-xs font-normal">
          <Checkbox
            checked={parameter.required}
            onCheckedChange={(value) => set({ required: value === true })}
          />
          {parameter.type === "boolean" ? "Must be turned on" : "Required"}
        </Label>
        {parameter.type === "text" && (
          <Label className="flex items-center gap-2 text-xs font-normal">
            <Checkbox
              checked={parameter.secret}
              onCheckedChange={(value) => set({ secret: value === true })}
            />
            Secret
          </Label>
        )}
        {parameter.type === "choice" && (
          <Label className="flex items-center gap-2 text-xs font-normal">
            <Checkbox
              checked={parameter.allow_custom}
              onCheckedChange={(value) => set({ allow_custom: value === true })}
            />
            Allow other values
          </Label>
        )}
      </div>
    </div>
  )
}

function OptionsEditor({
  options,
  onChange,
}: {
  options: OptionDraft[]
  onChange: (options: OptionDraft[]) => void
}) {
  const setOption = (key: number, patch: Partial<OptionDraft>) =>
    onChange(options.map((option) => (option.key === key ? { ...option, ...patch } : option)))
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <span className={small}>Options</span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={options.length >= 100}
          onClick={() => onChange([...options, newOption()])}
        >
          <RiAddLine data-icon="inline-start" />
          Option
        </Button>
      </div>
      {options.map((option, index) => (
        <div key={option.key} className="grid grid-cols-[1fr_1fr_auto] gap-2">
          <Input
            aria-label={`Option ${index + 1} value`}
            placeholder="Value"
            required
            maxLength={1024}
            value={option.value}
            onChange={(event) => setOption(option.key, { value: event.target.value })}
            className="font-mono"
          />
          <Input
            aria-label={`Option ${index + 1} label`}
            placeholder="Label"
            maxLength={100}
            value={option.label}
            onChange={(event) => setOption(option.key, { label: event.target.value })}
          />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label={`Remove option ${option.value || index + 1}`}
            disabled={options.length === 1}
            onClick={() => onChange(options.filter((item) => item.key !== option.key))}
          >
            <RiCloseLine />
          </Button>
        </div>
      ))}
    </div>
  )
}
