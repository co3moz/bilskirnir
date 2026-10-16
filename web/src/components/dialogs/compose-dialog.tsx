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
  composeBody,
  composeDraft,
  type ParameterDraft,
  type ComposeDraft,
} from "@/lib/compose"
import type { Compose } from "@/lib/types"
import { useSubmit } from "./container-dialogs"
import type { DialogProps } from "./side-drawer"
import { t } from "@/lib/i18n"
import { ParameterEditor } from "./parameter-editor"
import { Footer } from "./simple-dialogs"

/** The compose editor shared by Build image and Edit compose. */
export function ComposeFields({
  draft,
  onChange,
}: {
  draft: ComposeDraft
  onChange: (draft: ComposeDraft) => void
}) {
  const id = React.useId()
  const set = (patch: Partial<ComposeDraft>) => onChange({ ...draft, ...patch })
  const setParameter = (key: number, next: ParameterDraft) =>
    set({
      parameters: draft.parameters.map((parameter) => (parameter.key === key ? next : parameter)),
    })

  return (
    <FieldGroup>
      <FieldSet>
        <div className="flex items-center justify-between">
          <FieldLegend variant="label" className="mb-0">
            {t("compose.parameters")}
          </FieldLegend>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={draft.parameters.length >= 32}
            onClick={() => set({ parameters: [...draft.parameters, newParameter()] })}
          >
            <RiAddLine data-icon="inline-start" />
            {t("compose.parameter")}
          </Button>
        </div>
        <FieldDescription className="text-xs">{t("compose.parametersHint")}</FieldDescription>
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
        <FieldLabel htmlFor={`${id}-script`}>{t("compose.script")}</FieldLabel>
        <Textarea
          id={`${id}-script`}
          rows={9}
          required
          spellCheck={false}
          value={draft.script}
          onChange={(event) => set({ script: event.target.value })}
          className="max-h-[500px] overflow-y-auto font-mono text-xs"
        />
        <FieldDescription>{t("compose.scriptHint")}</FieldDescription>
      </Field>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Field orientation="horizontal">
          <Switch
            id={`${id}-root`}
            checked={draft.root}
            onCheckedChange={(root) => set({ root })}
          />
          <FieldLabel htmlFor={`${id}-root`}>{t("compose.root")}</FieldLabel>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-timeout`}>{t("compose.timeout")}</FieldLabel>
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

/** Gives a saved image a compose, changes it or removes it. */
export function ComposeDialog({ id, ...props }: DialogProps & { id: string }) {
  const image = useImages().data?.find((record) => record.id === id)
  const exists = !!image?.compose
  // The list leaves the script out; the compose route has all of it.
  const saved = useQuery({
    queryKey: ["images", id, "compose"],
    queryFn: () => api<Compose>(`images/${id}/compose`),
    enabled: exists,
    staleTime: 0,
  })
  const [fresh] = React.useState(composeDraft)
  const [draft, setDraft] = React.useState<ComposeDraft>()
  const { busy, error, submit } = useSubmit()
  const current = draft ?? (exists ? saved.data && composeDraft(saved.data) : fresh)
  const save = async (remove: boolean) => {
    if (remove) await api(`images/${id}/compose`, undefined, "DELETE")
    else if (current) await api(`images/${id}/compose`, composeBody(current), "PUT")
    else return
    closeDialog()
    toast.success(remove ? t("compose.removed") : t("compose.saved", { name: image?.name ?? "" }))
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
            <DialogTitle>{exists ? t("compose.edit") : t("compose.add")}</DialogTitle>
            <DialogDescription>
              {image ? t("compose.descriptionOf", { name: image.name }) : t("compose.description")}
            </DialogDescription>
          </DialogHeader>
          <div className="-mx-4 flex max-h-[min(36rem,60vh)] flex-col gap-5 overflow-y-auto px-4 py-1">
            {current ? (
              <>
                <ComposeFields draft={current} onChange={setDraft} />
                {exists && (
                  <FieldDescription className="text-xs">{t("compose.pinned")}</FieldDescription>
                )}
              </>
            ) : saved.isError ? (
              <FieldError>{saved.error.message}</FieldError>
            ) : (
              <Skeleton className="h-72 rounded-xl" />
            )}
          </div>
          <FieldError>{error}</FieldError>
          <Footer busy={busy} label={t("compose.save")} disabled={!current}>
            {exists && (
              <Button
                type="submit"
                formNoValidate
                data-action="remove"
                variant="outline"
                className="text-destructive sm:mr-auto"
                disabled={busy}
              >
                {t("compose.remove")}
              </Button>
            )}
          </Footer>
        </form>
      </DialogContent>
    </Dialog>
  )
}
