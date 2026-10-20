import * as React from "react"
import { RiArrowDownLine, RiArrowUpLine, RiDeleteBinLine } from "@remixicon/react"

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { CodeTextarea } from "@/components/code-textarea"
import { formatNumber, t } from "@/lib/i18n"
import type { BuildPart } from "@/lib/types"

/**
 * The scripts of templates merged into one build, in the order they run: each in a section of
 * its own that can be read and changed, moved or left out before anything is built.
 */
export function BuildParts({
  parts,
  onChange,
  onMove,
  onRemove,
}: {
  parts: BuildPart[]
  onChange: (index: number, script: string) => void
  onMove: (index: number, by: -1 | 1) => void
  onRemove: (index: number) => void
}) {
  const id = React.useId()
  return (
    <div className="flex flex-col gap-2">
      <FieldDescription>{t("build.partsHint")}</FieldDescription>
      <Accordion type="multiple" className="rounded-2xl border px-4">
        {parts.map((part, index) => (
          <AccordionItem key={part.id} value={part.id}>
            <AccordionTrigger>
              <span className="flex min-w-0 items-center gap-2">
                <Badge variant="outline" className="tabular-nums">
                  {formatNumber(index + 1)}
                </Badge>
                <span className="truncate">{part.name}</span>
              </span>
            </AccordionTrigger>
            <AccordionContent className="flex flex-col gap-3 px-1">
              <Field>
                <FieldLabel htmlFor={`${id}-${part.id}`} className="sr-only">
                  {t("build.partScript", { name: part.name })}
                </FieldLabel>
                <CodeTextarea
                  id={`${id}-${part.id}`}
                  language="bash"
                  rows={10}
                  required
                  value={part.script}
                  onChange={(event) => onChange(index, event.target.value)}
                  className="max-h-[400px]"
                />
              </Field>
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={index === 0}
                  onClick={() => onMove(index, -1)}
                >
                  <RiArrowUpLine data-icon="inline-start" />
                  {t("build.partEarlier")}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={index === parts.length - 1}
                  onClick={() => onMove(index, 1)}
                >
                  <RiArrowDownLine data-icon="inline-start" />
                  {t("build.partLater")}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="text-destructive"
                  disabled={parts.length === 1}
                  onClick={() => onRemove(index)}
                >
                  <RiDeleteBinLine data-icon="inline-start" />
                  {t("build.partRemove")}
                </Button>
              </div>
            </AccordionContent>
          </AccordionItem>
        ))}
      </Accordion>
    </div>
  )
}
