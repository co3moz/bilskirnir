import * as React from "react"
import { RiAddLine } from "@remixicon/react"
import { useQuery } from "@tanstack/react-query"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { closeDialog } from "@/lib/dialogs"
import { refreshAll, useImages } from "@/lib/queries"
import {
  newParameter,
  templateBody,
  templateDraft,
  type ParameterDraft,
  type TemplateDraft,
} from "@/lib/templates"
import type { Template } from "@/lib/types"
import { useSubmit } from "./environment-dialogs"
import type { DialogProps } from "./side-drawer"
import { ParameterEditor } from "./parameter-editor"
import { Footer } from "./simple-dialogs"

/** The template editor shared by Build image and Edit template. */
export function TemplateFields({
  draft,
  onChange,
}: {
  draft: TemplateDraft
  onChange: (draft: TemplateDraft) => void
}) {
  const id = React.useId()
  const set = (patch: Partial<TemplateDraft>) => onChange({ ...draft, ...patch })
  const setParameter = (key: number, next: ParameterDraft) =>
    set({
      parameters: draft.parameters.map((parameter) => (parameter.key === key ? next : parameter)),
    })

  return (
    <FieldGroup>
      <FieldSet>
        <div className="flex items-center justify-between">
          <FieldLegend variant="label" className="mb-0">
            Parameters
          </FieldLegend>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={draft.parameters.length >= 32}
            onClick={() => set({ parameters: [...draft.parameters, newParameter()] })}
          >
            <RiAddLine data-icon="inline-start" />
            Parameter
          </Button>
        </div>
        <FieldDescription className="text-xs">
          New environments ask for these. Each one reaches the script as an environment variable.
        </FieldDescription>
        {draft.parameters.map((parameter) => (
          <ParameterEditor
            key={parameter.key}
            parameter={parameter}
            onChange={(next) => setParameter(parameter.key, next)}
            onRemove={() =>
              set({ parameters: draft.parameters.filter((item) => item.key !== parameter.key) })
            }
          />
        ))}
      </FieldSet>
      <Field>
        <FieldLabel htmlFor={`${id}-script`}>Setup script</FieldLabel>
        <Textarea
          id={`${id}-script`}
          rows={9}
          required
          spellCheck={false}
          value={draft.script}
          onChange={(event) => set({ script: event.target.value })}
          className="font-mono text-xs"
        />
        <FieldDescription>
          Runs once the new desktop is ready, and again after a reset. Secret values are masked in
          the job log.
        </FieldDescription>
      </Field>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Field orientation="horizontal">
          <Switch
            id={`${id}-root`}
            checked={draft.root}
            onCheckedChange={(root) => set({ root })}
          />
          <FieldLabel htmlFor={`${id}-root`}>Run as root</FieldLabel>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-timeout`}>Time limit (seconds)</FieldLabel>
          <Input
            id={`${id}-timeout`}
            type="number"
            min={1}
            max={3500}
            required
            value={draft.timeout_seconds}
            onChange={(event) => set({ timeout_seconds: event.target.value })}
          />
        </Field>
      </div>
    </FieldGroup>
  )
}

/** Gives a saved image a template, changes it or removes it. */
export function TemplateDialog({ id, ...props }: DialogProps & { id: string }) {
  const image = useImages().data?.find((record) => record.id === id)
  const exists = !!image?.template
  // The list leaves the script out; the template route has all of it.
  const saved = useQuery({
    queryKey: ["images", id, "template"],
    queryFn: () => api<Template>(`images/${id}/template`),
    enabled: exists,
    staleTime: 0,
  })
  const [fresh] = React.useState(templateDraft)
  const [draft, setDraft] = React.useState<TemplateDraft>()
  const { busy, error, submit } = useSubmit()
  const current = draft ?? (exists ? saved.data && templateDraft(saved.data) : fresh)
  const save = async (remove: boolean) => {
    if (remove) await api(`images/${id}/template`, undefined, "DELETE")
    else if (current) await api(`images/${id}/template`, templateBody(current), "PUT")
    else return
    closeDialog()
    toast.success(remove ? "Removed the template." : `Saved the template of ${image?.name}.`)
    void refreshAll()
  }

  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-2xl">
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            // Remove submits the form too, without the browser checking its fields.
            const submitter = (event.nativeEvent as SubmitEvent).submitter
            return submit(() => save(submitter?.dataset.action === "remove"))(event)
          }}
        >
          <DialogHeader>
            <DialogTitle>{exists ? "Edit template" : "Add template"}</DialogTitle>
            <DialogDescription>
              What a new environment from {image?.name ?? "this image"} asks for, and the script
              that receives it.
            </DialogDescription>
          </DialogHeader>
          <div className="-mx-4 flex max-h-[min(36rem,60vh)] flex-col gap-5 overflow-y-auto px-4 py-1">
            {current ? (
              <>
                <TemplateFields draft={current} onChange={setDraft} />
                {exists && (
                  <FieldDescription className="text-xs">
                    Environments created earlier keep the template they were created with.
                  </FieldDescription>
                )}
              </>
            ) : saved.isError ? (
              <FieldError>{saved.error.message}</FieldError>
            ) : (
              <Skeleton className="h-72 rounded-xl" />
            )}
          </div>
          <FieldError>{error}</FieldError>
          <Footer busy={busy} label="Save template" disabled={!current}>
            {exists && (
              <Button
                type="submit"
                formNoValidate
                data-action="remove"
                variant="outline"
                className="text-destructive sm:mr-auto"
                disabled={busy}
              >
                Remove template
              </Button>
            )}
          </Footer>
        </form>
      </DialogContent>
    </Dialog>
  )
}
