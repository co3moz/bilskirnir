import * as React from "react"
import { RiAddLine, RiCloseLine } from "@remixicon/react"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
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
  displayLabel,
  DISPLAYS,
  draftParameter,
  newOption,
  PARAMETER_NAME,
  typeLabel,
  withOptions,
  withType,
  type OptionDraft,
  type ParameterDraft,
} from "@/lib/compose"
import type { ParameterDisplay, ParameterType } from "@/lib/types"
import { formatNumber, t } from "@/lib/i18n"
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
          aria-label={t("parameter.name")}
          placeholder="UNITY_EMAIL"
          required
          maxLength={64}
          pattern={PARAMETER_NAME}
          title={t("parameter.namePattern")}
          value={parameter.name}
          onChange={(event) => set({ name: event.target.value.toUpperCase() })}
          className="font-mono"
        />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label={
            parameter.name
              ? t("parameter.removeOf", { name: parameter.name })
              : t("parameter.remove")
          }
          className="sm:col-start-3"
          onClick={onRemove}
        >
          <RiCloseLine />
        </Button>
        <Input
          aria-label={t("parameter.label")}
          placeholder={t("parameter.labelPlaceholder")}
          maxLength={100}
          value={parameter.label}
          onChange={(event) => set({ label: event.target.value })}
          className="col-span-2 sm:col-span-1 sm:col-start-2 sm:row-start-1"
        />
      </div>
      <Textarea
        aria-label={t("common.description")}
        placeholder={t("parameter.descriptionPlaceholder")}
        rows={2}
        maxLength={500}
        value={parameter.description}
        onChange={(event) => set({ description: event.target.value })}
        className="min-h-0"
      />
      <div className="grid grid-cols-2 gap-2">
        <Field className="gap-1.5">
          <FieldLabel htmlFor={`${id}-type`} className={small}>
            {t("parameter.type")}
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
                    {typeLabel(type)}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field className="gap-1.5">
          <FieldLabel htmlFor={`${id}-display`} className={small}>
            {t("parameter.display")}
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
                    {displayLabel(display)}
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
                {t(`parameter.${field}`)}
              </FieldLabel>
              <Input
                id={`${id}-${field}`}
                type="number"
                step="any"
                min={field === "step" ? 0 : undefined}
                required={parameter.display === "slider" && field !== "step"}
                placeholder={field === "step" ? t("parameter.any") : t("common.none")}
                value={parameter[field]}
                onChange={(event) => set({ [field]: event.target.value })}
              />
            </Field>
          ))}
        </div>
      )}
      {parameter.type === "volume" && (
        <Field className="gap-1.5">
          <FieldLabel htmlFor={`${id}-path`} className={small}>
            {t("parameter.path")}
          </FieldLabel>
          <Input
            id={`${id}-path`}
            required
            pattern="/[^,]+"
            title={t("parameter.pathPattern")}
            placeholder="/opt/unity"
            value={parameter.path}
            onChange={(event) => set({ path: event.target.value })}
            className="font-mono"
          />
          <FieldDescription className="text-xs">
            {t("parameter.pathHint", { name: parameter.name || "UNITY_HOME" })}
          </FieldDescription>
        </Field>
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
            {t("parameter.default")}
          </FieldLabel>
          <Input id={`${id}-default`} disabled placeholder={t("parameter.noSecretDefault")} />
        </Field>
      ) : (
        // A new type or display starts the preview over, with nothing typed as another value.
        <ParameterField
          key={`${parameter.type}-${parameter.display}`}
          parameter={draftParameter(parameter)}
          label={parameter.type === "boolean" ? t("parameter.onByDefault") : t("parameter.default")}
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
          {parameter.type === "boolean" ? t("parameter.mustBeOn") : t("parameter.required")}
        </Label>
        {parameter.type === "text" && (
          <Label className="flex items-center gap-2 text-xs font-normal">
            <Checkbox
              checked={parameter.secret}
              onCheckedChange={(value) => set({ secret: value === true })}
            />
            {t("compose.secret")}
          </Label>
        )}
        {parameter.type === "choice" && (
          <Label className="flex items-center gap-2 text-xs font-normal">
            <Checkbox
              checked={parameter.allow_custom}
              onCheckedChange={(value) => set({ allow_custom: value === true })}
            />
            {t("parameter.allowCustom")}
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
        <span className={small}>{t("parameter.options")}</span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={options.length >= 100}
          onClick={() => onChange([...options, newOption()])}
        >
          <RiAddLine data-icon="inline-start" />
          {t("parameter.option")}
        </Button>
      </div>
      {options.map((option, index) => (
        <div key={option.key} className="grid grid-cols-[1fr_1fr_auto] gap-2">
          <Input
            aria-label={t("parameter.optionValue", { number: formatNumber(index + 1) })}
            placeholder={t("parameter.value")}
            required
            maxLength={1024}
            value={option.value}
            onChange={(event) => setOption(option.key, { value: event.target.value })}
            className="font-mono"
          />
          <Input
            aria-label={t("parameter.optionLabel", { number: formatNumber(index + 1) })}
            placeholder={t("parameter.label")}
            maxLength={100}
            value={option.label}
            onChange={(event) => setOption(option.key, { label: event.target.value })}
          />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label={t("parameter.removeOption", {
              option: option.value || formatNumber(index + 1),
            })}
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
