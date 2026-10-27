import * as React from "react"

import { ConceptIcon } from "@/components/concept"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field"
import { formatList, rich, t } from "@/lib/i18n"
import { optionKey, optionOn, type OptionGroup, type OptionValues } from "@/lib/template-options"

/**
 * A template's options, each one checkbox, under its template's name when several were merged.
 * Their values reach the build only as lines at the top of each script, which `hint` says.
 */
export function BuildOptions({
  groups,
  values,
  onChange,
  hint,
}: {
  groups: OptionGroup[]
  values: OptionValues
  /** One option turned on or off, by its `optionKey`. */
  onChange: (key: string, on: boolean) => void
  hint?: boolean
}) {
  const id = React.useId()
  const first = groups[0]
  const example = first?.options[0]
  return (
    <FieldSet className="gap-2">
      <FieldLegend variant="label" className="mb-0">
        {t("build.options")}
      </FieldLegend>
      {hint && example && (
        <FieldDescription className="text-xs">
          {rich("build.optionsHint", {
            line: (
              <code className="font-mono">{`${example.name}=${optionOn(example, values, first.part)}`}</code>
            ),
          })}
        </FieldDescription>
      )}
      {groups.map((group) => (
        <div
          key={group.part ?? ""}
          role="group"
          aria-labelledby={group.name ? `${id}-${group.part}` : undefined}
          className="flex flex-col gap-1.5"
        >
          {group.name && (
            <p id={`${id}-${group.part}`} className="text-xs font-medium text-muted-foreground">
              {group.name}
            </p>
          )}
          <ul className="flex flex-col divide-y rounded-xl border">
            {group.options.map((option) => {
              const key = optionKey(option.name, group.part)
              return (
                <li key={key} className="px-3 py-2.5">
                  <Field orientation="horizontal" className="items-start gap-3">
                    <Checkbox
                      id={`${id}-${key}`}
                      className="mt-0.5"
                      checked={optionOn(option, values, group.part)}
                      onCheckedChange={(checked) => onChange(key, checked === true)}
                    />
                    <FieldContent className="gap-1">
                      <FieldLabel htmlFor={`${id}-${key}`} className="block w-full leading-snug">
                        <span className="font-medium">{option.label}</span>
                        {option.description && (
                          <span className="font-normal text-muted-foreground">
                            {" "}
                            {option.description}
                          </span>
                        )}
                      </FieldLabel>
                      {option.mcp && (
                        <p className="flex items-start gap-1.5 text-xs text-muted-foreground">
                          <ConceptIcon concept="mcp" className="mt-px size-3.5 shrink-0" />
                          {t("images.mcpServers", { names: formatList(option.mcp) })}
                        </p>
                      )}
                    </FieldContent>
                  </Field>
                </li>
              )
            })}
          </ul>
        </div>
      ))}
    </FieldSet>
  )
}
