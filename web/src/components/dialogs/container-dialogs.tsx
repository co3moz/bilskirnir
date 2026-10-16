import * as React from "react"
import { RiAlertLine } from "@remixicon/react"
import { toast } from "sonner"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
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
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Spinner } from "@/components/ui/spinner"
import { runJob } from "@/lib/actions"
import { api } from "@/lib/api"
import { closeDialog } from "@/lib/dialogs"
import { imageChoices, initialImage, restartNotice } from "@/lib/format"
import { useContainers, useImages, useProfiles, useSettings, useVolumes } from "@/lib/queries"
import { parameterBody, templateFor } from "@/lib/templates"
import type { ImageRecord, SettingsPreview } from "@/lib/types"
import type { DialogProps } from "./side-drawer"
import { ParameterField } from "./parameter-field"
import { specBody, specDraft, SpecFields, type SpecDraft } from "./spec-fields"

/** A dialog whose form submits once at a time and shows the manager's error in place. */
export function useSubmit() {
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState("")
  const submit = (action: () => Promise<unknown>) => async (event: React.FormEvent) => {
    event.preventDefault()
    setBusy(true)
    setError("")
    try {
      await action()
    } catch (failure) {
      setError((failure as Error).message)
    } finally {
      setBusy(false)
    }
  }
  return { busy, error, setError, submit }
}

const scrollBody = "-mx-4 flex max-h-[min(36rem,70vh)] flex-col gap-5 overflow-y-auto px-4 py-1"

/** The values an image's template asks for, filled in with its defaults. */
function TemplateValues({
  template,
  values,
  onChange,
}: {
  template: NonNullable<ImageRecord["template"]>
  values: Record<string, string>
  onChange: (values: Record<string, string>) => void
}) {
  if (!template.parameters.length)
    return (
      <FieldDescription>The image runs a setup script once the desktop is ready.</FieldDescription>
    )
  return (
    <FieldSet>
      <FieldLegend variant="label">Template</FieldLegend>
      <FieldDescription className="text-xs">
        The image's setup script receives these once the desktop is ready. Secret values are not
        shown again.
      </FieldDescription>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        {template.parameters.map((parameter) => (
          <ParameterField
            key={`${parameter.name}-${parameter.type}`}
            parameter={parameter}
            value={values[parameter.name] ?? parameter.default}
            onChange={(value) => onChange({ ...values, [parameter.name]: value })}
            hints
          />
        ))}
      </div>
    </FieldSet>
  )
}

export function CreateContainerDialog({ image, ...props }: DialogProps & { image?: string }) {
  const settings = useSettings().data
  const profiles = useProfiles().data ?? []
  const images = useImages().data ?? []
  const volumes = useVolumes().data ?? []
  const choices = imageChoices(profiles, images, settings?.base_image, image)
  const [name, setName] = React.useState("")
  const [selected, setSelected] = React.useState<string | undefined>()
  const [draft, setDraft] = React.useState<SpecDraft>(() => specDraft({}, settings))
  const chosen = selected ?? initialImage(choices, image ?? settings?.base_image)
  const template = templateFor(images, chosen)
  // Values typed for one image are kept while another is chosen, and sent only for its own fields.
  const [values, setValues] = React.useState<Record<string, string>>({})
  const { busy, error, submit } = useSubmit()
  const id = React.useId()

  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-2xl">
        <form
          className="flex flex-col gap-4"
          onSubmit={submit(() =>
            runJob("containers", {
              name,
              image: chosen,
              ...specBody(draft),
              ...(template ? { parameters: parameterBody(template, values) } : {}),
            })
          )}
        >
          <DialogHeader>
            <DialogTitle>New container</DialogTitle>
            <DialogDescription>
              A container from a profile, snapshot or custom image. A profile that is not downloaded
              yet downloads first.
            </DialogDescription>
          </DialogHeader>
          <div className={scrollBody}>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor={`${id}-name`}>Name</FieldLabel>
                <Input
                  id={`${id}-name`}
                  required
                  maxLength={100}
                  placeholder="Unity workspace"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  autoFocus
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-image`}>Image or snapshot</FieldLabel>
                <Select value={chosen ?? ""} onValueChange={setSelected}>
                  <SelectTrigger id={`${id}-image`} className="w-full">
                    <SelectValue placeholder="Build an image first" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {choices.map((choice) => (
                        <SelectItem
                          key={choice.value}
                          value={choice.value}
                          disabled={choice.disabled}
                        >
                          {choice.label}
                          {choice.disabled ? " · unavailable" : ""}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              {template && (
                <TemplateValues template={template} values={values} onChange={setValues} />
              )}
            </FieldGroup>
            <SpecFields draft={draft} onChange={setDraft} settings={settings} volumes={volumes} />
          </div>
          {/* Outside the scrolling fields, so a long form cannot hide the outcome. */}
          <FieldError>{error}</FieldError>
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                Cancel
              </Button>
            </DialogClose>
            <Button type="submit" disabled={busy || !chosen}>
              {busy && <Spinner data-icon="inline-start" />}
              Create
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function SettingsDialog({ id, ...props }: DialogProps & { id: string }) {
  const item = useContainers().data?.find((entry) => entry.id === id)
  const settings = useSettings().data
  const volumes = useVolumes().data ?? []
  const spec = item?.spec
  const [name, setName] = React.useState(spec?.name ?? "")
  const [draft, setDraftState] = React.useState<SpecDraft>(() => specDraft(spec ?? {}, settings))
  // A restart warning is confirmed for one set of values; editing the form asks again.
  const [notice, setNotice] = React.useState("")
  const { busy, error, submit } = useSubmit()
  const formId = React.useId()
  const running = item?.state === "running"

  const setDraft = (next: SpecDraft) => {
    setNotice("")
    setDraftState(next)
  }

  const save = async () => {
    const body = { name, ...specBody(draft) }
    // The server decides what restarts the container; ask it first and warn once before applying.
    if (!notice) {
      const preview = await api<SettingsPreview>(`containers/${id}/settings`, {
        ...body,
        dry_run: true,
      })
      if (!preview.live.length && !preview.restart.length) {
        closeDialog()
        toast("No settings changed.")
        return
      }
      if (preview.restart.length) {
        setNotice(restartNotice(preview.restart, running))
        return
      }
    }
    await runJob(`containers/${id}/settings`, body)
  }

  if (!spec) return null
  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-2xl">
        <form className="flex flex-col gap-4" onSubmit={submit(save)}>
          <DialogHeader>
            <DialogTitle>Container settings</DialogTitle>
            <DialogDescription>
              Name, internet and resource limits apply to the running container. Other changes need
              a new container; its files are kept.
            </DialogDescription>
          </DialogHeader>
          <div className={scrollBody}>
            <Field>
              <FieldLabel htmlFor={`${formId}-name`}>Name</FieldLabel>
              <Input
                id={`${formId}-name`}
                required
                maxLength={100}
                value={name}
                onChange={(event) => {
                  setNotice("")
                  setName(event.target.value)
                }}
              />
            </Field>
            <SpecFields
              draft={draft}
              onChange={setDraft}
              settings={settings}
              volumes={volumes}
              spec={spec}
            />
          </div>
          {/* Outside the scrolling fields, next to the button whose label it changes. */}
          {notice && (
            <Alert>
              <RiAlertLine className="text-warning" />
              <AlertDescription>{notice}</AlertDescription>
            </Alert>
          )}
          <FieldError>{error}</FieldError>
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                Cancel
              </Button>
            </DialogClose>
            <Button type="submit" disabled={busy}>
              {busy && <Spinner data-icon="inline-start" />}
              {notice && running ? "Save and restart" : "Save"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
