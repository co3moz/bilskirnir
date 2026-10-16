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
import { t } from "@/lib/i18n"
import { useContainers, useImages, useSettings, useVolumes } from "@/lib/queries"
import { parameterBody, composeFor } from "@/lib/compose"
import type { ImageItem, SettingsPreview } from "@/lib/types"
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

/** The values an image's compose asks for, filled in with its defaults. */
function ComposeValues({
  compose,
  values,
  onChange,
}: {
  compose: NonNullable<ImageItem["compose"]>
  values: Record<string, string>
  onChange: (values: Record<string, string>) => void
}) {
  if (!compose.parameters.length)
    return <FieldDescription>{t("compose.runsScript")}</FieldDescription>
  return (
    <FieldSet>
      <FieldLegend variant="label">{t("compose.name")}</FieldLegend>
      <FieldDescription className="text-xs">{t("compose.valuesHint")}</FieldDescription>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        {compose.parameters.map((parameter) => (
          <ParameterField
            key={`${parameter.name}-${parameter.type}`}
            parameter={parameter}
            value={values[parameter.name] ?? parameter.default}
            onChange={(value) => onChange({ ...values, [parameter.name]: value })}
            hints
            createVolumes
          />
        ))}
      </div>
    </FieldSet>
  )
}

export function CreateContainerDialog({ image, ...props }: DialogProps & { image?: string }) {
  const settings = useSettings().data
  const images = useImages().data ?? []
  const volumes = useVolumes().data ?? []
  const choices = imageChoices(images, settings?.base_image, image)
  const [name, setName] = React.useState("")
  const [selected, setSelected] = React.useState<string | undefined>()
  const [draft, setDraft] = React.useState<SpecDraft>(() => specDraft({}, settings))
  const chosen = selected ?? initialImage(choices, image ?? settings?.base_image)
  const compose = composeFor(images, chosen)
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
              ...(compose ? { parameters: parameterBody(compose, values) } : {}),
            })
          )}
        >
          <DialogHeader>
            <DialogTitle>{t("containers.new")}</DialogTitle>
            <DialogDescription>{t("create.description")}</DialogDescription>
          </DialogHeader>
          <div className={scrollBody}>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor={`${id}-name`}>{t("table.name")}</FieldLabel>
                <Input
                  id={`${id}-name`}
                  required
                  maxLength={100}
                  placeholder={t("create.namePlaceholder")}
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  autoFocus
                />
              </Field>
              <Field>
                <FieldLabel htmlFor={`${id}-image`}>{t("create.image")}</FieldLabel>
                <Select value={chosen ?? ""} onValueChange={setSelected}>
                  <SelectTrigger id={`${id}-image`} className="w-full">
                    <SelectValue placeholder={t("build.noBase")} />
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
                          {choice.disabled ? ` · ${t("images.unavailable")}` : ""}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              {compose && <ComposeValues compose={compose} values={values} onChange={setValues} />}
            </FieldGroup>
            <SpecFields draft={draft} onChange={setDraft} settings={settings} volumes={volumes} />
          </div>
          {/* Outside the scrolling fields, so a long form cannot hide the outcome. */}
          <FieldError>{error}</FieldError>
          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="outline">
                {t("common.cancel")}
              </Button>
            </DialogClose>
            <Button type="submit" disabled={busy || !chosen}>
              {busy && <Spinner data-icon="inline-start" />}
              {t("common.create")}
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
        toast(t("settings.unchanged"))
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
            <DialogTitle>{t("settings.title")}</DialogTitle>
            <DialogDescription>{t("settings.description")}</DialogDescription>
          </DialogHeader>
          <div className={scrollBody}>
            <Field>
              <FieldLabel htmlFor={`${formId}-name`}>{t("table.name")}</FieldLabel>
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
              // These settings are what this dialog is for, so they start in view.
              advancedOpen
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
                {t("common.cancel")}
              </Button>
            </DialogClose>
            <Button type="submit" disabled={busy}>
              {busy && <Spinner data-icon="inline-start" />}
              {notice && running ? t("settings.saveRestart") : t("common.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
