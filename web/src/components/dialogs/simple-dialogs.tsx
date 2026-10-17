import * as React from "react"
import { RiAlertLine, RiClipboardLine } from "@remixicon/react"
import { toast } from "sonner"

import { Alert, AlertDescription } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
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
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
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
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import { runJob } from "@/lib/actions"
import { api } from "@/lib/api"
import { parseBuildConfig, type ImportedBuild } from "@/lib/build-config"
import { closeDialog } from "@/lib/dialogs"
import { imageChoices, initialImage } from "@/lib/format"
import { formatNumber, rich, t } from "@/lib/i18n"
import { refreshAll, useContainers, useImages, useSettings } from "@/lib/queries"
import { composeBody, composeDraft, type ComposeDraft } from "@/lib/compose"
import { useSubmit } from "./container-dialogs"
import type { DialogProps } from "./side-drawer"
import { ComposeFields } from "./compose-dialog"

export function Footer({
  busy,
  label,
  disabled,
  children,
}: {
  busy: boolean
  label: string
  disabled?: boolean
  /** Secondary actions, shown before Cancel. */
  children?: React.ReactNode
}) {
  return (
    <DialogFooter>
      {children}
      <DialogClose asChild>
        <Button type="button" variant="outline">
          {t("common.cancel")}
        </Button>
      </DialogClose>
      <Button type="submit" disabled={busy || disabled}>
        {busy && <Spinner data-icon="inline-start" />}
        {label}
      </Button>
    </DialogFooter>
  )
}

export function SnapshotDialog({ id, ...props }: DialogProps & { id: string }) {
  const item = useContainers().data?.find((entry) => entry.id === id)
  const [name, setName] = React.useState("")
  const [description, setDescription] = React.useState("")
  const { busy, error, submit } = useSubmit()
  const field = React.useId()
  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-md">
        <form
          className="flex flex-col gap-4"
          onSubmit={submit(() => runJob(`containers/${id}/snapshot`, { name, description }))}
        >
          <DialogHeader>
            <DialogTitle>{t("snapshot.title")}</DialogTitle>
            <DialogDescription>
              {item
                ? t("snapshot.descriptionOf", { name: item.spec.name })
                : t("snapshot.description")}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={`${field}-name`}>{t("table.name")}</FieldLabel>
              <Input
                id={`${field}-name`}
                required
                maxLength={100}
                value={name}
                onChange={(event) => setName(event.target.value)}
                autoFocus
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={`${field}-description`}>{t("common.description")}</FieldLabel>
              <Textarea
                id={`${field}-description`}
                rows={3}
                maxLength={4096}
                value={description}
                onChange={(event) => setDescription(event.target.value)}
              />
              <FieldDescription>{t("snapshot.hint")}</FieldDescription>
            </Field>
            <FieldError>{error}</FieldError>
          </FieldGroup>
          <Footer busy={busy} label={t("snapshot.title")} />
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function DeleteDialog({ id, ...props }: DialogProps & { id: string }) {
  const item = useContainers().data?.find((entry) => entry.id === id)
  const { busy, error, submit } = useSubmit()
  return (
    <AlertDialog {...props}>
      <AlertDialogContent>
        <form
          className="contents"
          onSubmit={submit(() => runJob(`containers/${id}`, undefined, "DELETE"))}
        >
          <AlertDialogHeader>
            <AlertDialogTitle>
              {item ? t("delete.titleOf", { name: item.spec.name }) : t("delete.title")}
            </AlertDialogTitle>
            <AlertDialogDescription>{t("delete.description")}</AlertDialogDescription>
          </AlertDialogHeader>
          <FieldError>{error}</FieldError>
          <AlertDialogFooter>
            <AlertDialogCancel type="button">{t("common.cancel")}</AlertDialogCancel>
            <Button type="submit" variant="destructive" disabled={busy}>
              {busy && <Spinner data-icon="inline-start" />}
              {t("common.delete")}
            </Button>
          </AlertDialogFooter>
        </form>
      </AlertDialogContent>
    </AlertDialog>
  )
}

const DEFAULT_SCRIPT = `#!/usr/bin/env bash
set -euo pipefail
apt-get update
apt-get install -y git build-essential
rm -rf /var/lib/apt/lists/*`

export function BuildImageDialog(props: DialogProps) {
  const settings = useSettings().data
  const choices = imageChoices(useImages().data ?? [], settings?.base_image)
  const [name, setName] = React.useState("")
  const [base, setBase] = React.useState<string | undefined>()
  const [script, setScript] = React.useState(DEFAULT_SCRIPT)
  const [internet, setInternet] = React.useState(true)
  const [compose, setCompose] = React.useState<ComposeDraft | null>(null)
  const [importing, setImporting] = React.useState(false)
  // The base image a loaded configuration named when this host cannot build on it.
  const [missingBase, setMissingBase] = React.useState<string>()
  const chosen = base ?? initialImage(choices, settings?.base_image)
  const { busy, error, submit } = useSubmit()
  const field = React.useId()

  const load = (config: ImportedBuild) => {
    if (config.name !== undefined) setName(config.name)
    if (config.internet !== undefined) setInternet(config.internet)
    setScript(config.script)
    setCompose(config.compose ? composeDraft(config.compose) : null)
    const wanted = config.base_image
    const usable = choices.some((choice) => choice.value === wanted && !choice.disabled)
    if (usable) setBase(wanted)
    setMissingBase(usable ? undefined : wanted)
    setImporting(false)
  }

  return (
    <Dialog {...props}>
      {/* The dialog stays inside the window: its body scrolls and the buttons stay in view. */}
      <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col sm:max-w-2xl">
        {importing ? (
          <ImportConfiguration onLoad={load} onBack={() => setImporting(false)} />
        ) : (
          <form
            className="flex min-h-0 flex-col gap-4"
            onSubmit={submit(() =>
              runJob("images/build", {
                name,
                base_image: chosen,
                script,
                internet,
                ...(compose ? { compose: composeBody(compose) } : {}),
              })
            )}
          >
            <DialogHeader>
              <DialogTitle>{t("images.build")}</DialogTitle>
              <DialogDescription>{t("build.description")}</DialogDescription>
            </DialogHeader>
            <FieldGroup className="-mx-4 min-h-0 w-auto overflow-y-auto px-4 py-1">
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <Field>
                  <FieldLabel htmlFor={`${field}-name`}>{t("table.name")}</FieldLabel>
                  <Input
                    id={`${field}-name`}
                    required
                    maxLength={100}
                    placeholder={t("build.namePlaceholder")}
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    autoFocus
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${field}-base`}>{t("build.base")}</FieldLabel>
                  <Select
                    value={chosen ?? ""}
                    onValueChange={(value) => {
                      setBase(value)
                      setMissingBase(undefined)
                    }}
                  >
                    <SelectTrigger id={`${field}-base`} className="w-full">
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
              </div>
              {missingBase && (
                <Alert>
                  <RiAlertLine className="text-warning" />
                  <AlertDescription>
                    {rich("build.missingBase", {
                      image: <code className="font-mono break-all">{missingBase}</code>,
                    })}
                  </AlertDescription>
                </Alert>
              )}
              <Tabs defaultValue="build">
                <TabsList>
                  <TabsTrigger value="build">{t("build.script")}</TabsTrigger>
                  <TabsTrigger value="compose">
                    {t("compose.name")}
                    {compose?.parameters.length
                      ? ` · ${formatNumber(compose.parameters.length)}`
                      : ""}
                  </TabsTrigger>
                </TabsList>
                <TabsContent value="build">
                  <FieldGroup>
                    <Field>
                      <FieldLabel htmlFor={`${field}-script`}>{t("compose.script")}</FieldLabel>
                      <Textarea
                        id={`${field}-script`}
                        rows={12}
                        required
                        spellCheck={false}
                        value={script}
                        onChange={(event) => setScript(event.target.value)}
                        className="max-h-[500px] overflow-y-auto font-mono text-xs"
                      />
                      <FieldDescription>{t("build.scriptHint")}</FieldDescription>
                    </Field>
                    <Field orientation="horizontal">
                      <Switch
                        id={`${field}-internet`}
                        checked={internet}
                        onCheckedChange={setInternet}
                      />
                      <FieldLabel htmlFor={`${field}-internet`}>{t("build.internet")}</FieldLabel>
                    </Field>
                  </FieldGroup>
                </TabsContent>
                <TabsContent value="compose">
                  {compose ? (
                    <div className="flex flex-col gap-4">
                      <ComposeFields draft={compose} onChange={setCompose} />
                      <Button
                        type="button"
                        variant="outline"
                        className="self-start text-destructive"
                        onClick={() => setCompose(null)}
                      >
                        {t("compose.remove")}
                      </Button>
                    </div>
                  ) : (
                    <div className="flex flex-col items-start gap-3 py-2">
                      <p className="text-sm text-muted-foreground">{t("compose.about")}</p>
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() => setCompose(composeDraft())}
                      >
                        {t("compose.add")}
                      </Button>
                    </div>
                  )}
                </TabsContent>
              </Tabs>
              <FieldError>{error}</FieldError>
            </FieldGroup>
            <Footer busy={busy} label={t("images.build")} disabled={!chosen}>
              <Button
                type="button"
                variant="outline"
                className="sm:mr-auto"
                onClick={() => setImporting(true)}
              >
                <RiClipboardLine data-icon="inline-start" />
                {t("build.importConfig")}
              </Button>
            </Footer>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}

/** The build dialog's paste view: a configuration copied from a built image fills the form. */
function ImportConfiguration({
  onLoad,
  onBack,
}: {
  onLoad: (config: ImportedBuild) => void
  onBack: () => void
}) {
  const [text, setText] = React.useState("")
  const [error, setError] = React.useState<string>()
  const field = React.useId()
  return (
    <form
      className="flex min-h-0 flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault()
        try {
          onLoad(parseBuildConfig(text))
        } catch (reason) {
          setError((reason as Error).message)
        }
      }}
    >
      <DialogHeader>
        <DialogTitle>{t("build.importConfig")}</DialogTitle>
        <DialogDescription>{t("build.importConfigHint")}</DialogDescription>
      </DialogHeader>
      <FieldGroup className="-mx-4 min-h-0 w-auto overflow-y-auto px-4 py-1">
        <Field data-invalid={!!error}>
          <FieldLabel htmlFor={`${field}-config`}>{t("build.config")}</FieldLabel>
          <Textarea
            id={`${field}-config`}
            rows={12}
            required
            spellCheck={false}
            placeholder={'{\n  "name": "Unity tools",\n  "base_image": "…",\n  "script": "…"\n}'}
            value={text}
            onChange={(event) => {
              setText(event.target.value)
              setError(undefined)
            }}
            aria-invalid={!!error}
            className="max-h-[500px] overflow-y-auto font-mono text-xs"
            autoFocus
          />
          <FieldError>{error}</FieldError>
        </Field>
      </FieldGroup>
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onBack}>
          {t("common.back")}
        </Button>
        <Button type="submit">{t("build.load")}</Button>
      </DialogFooter>
    </form>
  )
}

/** Copies a volume's files into a new volume; the copy's job opens once it is queued. */
export function VolumeCopyDialog({ name: source, ...props }: DialogProps & { name: string }) {
  const [name, setName] = React.useState(`${source}-copy`)
  const { busy, error, submit } = useSubmit()
  const field = React.useId()
  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-md">
        <form
          className="flex flex-col gap-4"
          onSubmit={submit(() => runJob(`volumes/${encodeURIComponent(source)}/copy`, { name }))}
        >
          <DialogHeader>
            <DialogTitle className="break-all">
              {t("volumes.copyTitle", { name: source })}
            </DialogTitle>
            <DialogDescription>{t("volumes.copyDescription")}</DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={`${field}-name`}>{t("volumes.copyName")}</FieldLabel>
              <Input
                id={`${field}-name`}
                required
                pattern="[a-zA-Z0-9][a-zA-Z0-9_.\-]+"
                title={t("volumes.namePattern")}
                value={name}
                onChange={(event) => setName(event.target.value)}
                className="font-mono"
                autoFocus
              />
            </Field>
            <FieldError>{error}</FieldError>
          </FieldGroup>
          <Footer busy={busy} label={t("volumes.copy")} />
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function VolumeDialog(props: DialogProps) {
  const [name, setName] = React.useState("")
  const { busy, error, submit } = useSubmit()
  const field = React.useId()
  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-md">
        <form
          className="flex flex-col gap-4"
          onSubmit={submit(async () => {
            await api("volumes", { name })
            closeDialog()
            toast.success(t("volumes.created", { name }))
            void refreshAll()
          })}
        >
          <DialogHeader>
            <DialogTitle>{t("volumes.new")}</DialogTitle>
            <DialogDescription>{t("volumes.newHint")}</DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={`${field}-name`}>{t("table.name")}</FieldLabel>
              <Input
                id={`${field}-name`}
                required
                pattern="[a-zA-Z0-9][a-zA-Z0-9_.\-]+"
                title={t("volumes.namePattern")}
                placeholder="unity-projects"
                value={name}
                onChange={(event) => setName(event.target.value)}
                className="font-mono"
                autoFocus
              />
            </Field>
            <FieldError>{error}</FieldError>
          </FieldGroup>
          <Footer busy={busy} label={t("common.create")} />
        </form>
      </DialogContent>
    </Dialog>
  )
}
