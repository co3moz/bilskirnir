import * as React from "react"
import { RiAlertLine, RiClipboardLine, RiEqualizerLine } from "@remixicon/react"
import { toast } from "sonner"

import { IconPicker, ImageChoiceItem } from "@/components/image-icon"
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
import { Checkbox } from "@/components/ui/checkbox"
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
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Spinner } from "@/components/ui/spinner"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { CodeEditor } from "@/components/code-editor"
import { Textarea } from "@/components/ui/textarea"
import { runJob } from "@/lib/actions"
import { api } from "@/lib/api"
import { parseBuildConfig, type ImportedBuild } from "@/lib/build-config"
import { rowsHeight } from "@/lib/code-editor"
import { closeDialog } from "@/lib/dialogs"
import { deletedNotice, imageChoices, imageHolders, initialImage } from "@/lib/format"
import { formatList, formatNumber, rich, t } from "@/lib/i18n"
import { refreshAll, useContainers, useImages, useSettings, useVolumes } from "@/lib/queries"
import { composeBody, composeDraft, orderSteps, type ComposeDraft } from "@/lib/compose"
import type { BuildPart } from "@/lib/types"
import { ownedVolumes, volumeApi, volumeHolders } from "@/lib/volume-files"
import { BuildParts } from "./build-parts"
import { BuildServers } from "./build-servers"
import { BuildSummary } from "./build-summary"
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
  const owned = ownedVolumes(useVolumes().data ?? [], id)
  // Volumes stay unless this is turned on, and it offers only those created for the container.
  const [volumes, setVolumes] = React.useState(false)
  const removing = volumes && owned.length > 0
  const { busy, error, submit } = useSubmit()
  const field = React.useId()
  return (
    <AlertDialog {...props}>
      <AlertDialogContent>
        <form
          className="contents"
          onSubmit={submit(() =>
            runJob(`containers/${id}${removing ? "?delete_volumes=true" : ""}`, undefined, "DELETE")
          )}
        >
          <AlertDialogHeader>
            <AlertDialogTitle>
              {item ? t("delete.titleOf", { name: item.spec.name }) : t("delete.title")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {removing ? t("delete.descriptionVolumes") : t("delete.description")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {owned.length > 0 && (
            <Field orientation="horizontal" className="items-start">
              <Checkbox
                id={field}
                checked={volumes}
                onCheckedChange={(checked) => setVolumes(checked === true)}
              />
              <div className="flex min-w-0 flex-col gap-1">
                <FieldLabel htmlFor={field}>{t("delete.volumes")}</FieldLabel>
                <FieldDescription className="font-mono text-xs break-all">
                  {owned.join(", ")}
                </FieldDescription>
              </div>
            </Field>
          )}
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

/** The templates a configuration is built from: the one it names, or those merged into it. */
const templatesOf = (config?: ImportedBuild) =>
  config?.template_ids?.length
    ? config.template_ids
    : config?.template_id
      ? [config.template_id]
      : []

/**
 * Builds an image from a script, or from a configuration a template or a paste filled in. A
 * `simple` form asks only for a name and a base image and builds the rest as configured, until
 * Advanced editing shows everything else.
 */
export function BuildImageDialog({
  config,
  simple,
  ...props
}: DialogProps & { config?: ImportedBuild; simple?: boolean }) {
  const settings = useSettings().data
  const images = useImages()
  const choices = imageChoices(images.data ?? [], settings?.base_image)
  const [name, setName] = React.useState(config?.name ?? "")
  const [description, setDescription] = React.useState(config?.description ?? "")
  const [icon, setIcon] = React.useState(config?.icon)
  const [iconError, setIconError] = React.useState<string>()
  const [templateIds, setTemplateIds] = React.useState(() => templatesOf(config))
  const [warnings, setWarnings] = React.useState(config?.warnings ?? [])
  // The base the user chose, and the one a loaded configuration named.
  const [base, setBase] = React.useState<string | undefined>()
  const [wanted, setWanted] = React.useState(config?.base_image)
  const [script, setScript] = React.useState(config?.script ?? DEFAULT_SCRIPT)
  // Templates merged into one build run as parts instead of one script.
  const [parts, setParts] = React.useState(config?.parts ?? null)
  const [internet, setInternet] = React.useState(config?.internet ?? true)
  const [compose, setCompose] = React.useState<ComposeDraft | null>(
    config?.compose ? composeDraft(config.compose) : null
  )
  // Desktop MCP servers the configuration declares; they are shown, never typed in here.
  const [servers, setServers] = React.useState(config?.mcp ?? [])
  const [importing, setImporting] = React.useState(false)
  const [advanced, setAdvanced] = React.useState(!simple)
  const parameterCount =
    compose?.steps.reduce((count, step) => count + step.parameters.length, 0) ?? 0

  // A part's setup step goes with it: it moves along, and leaves when the part does.
  const moveParts = (next: BuildPart[]) => {
    setParts(next)
    setCompose(
      (current) =>
        current && {
          steps: orderSteps(
            current.steps,
            next.map((part) => part.id)
          ),
        }
    )
  }
  const removePart = (index: number) => {
    const removed = parts?.[index]
    if (!parts || !removed) return
    setParts(parts.filter((_, other) => other !== index))
    setTemplateIds((ids) => ids.filter((id) => id !== removed.id))
    setCompose((current) => {
      const steps = current?.steps.filter((step) => step.id !== removed.id) ?? []
      return steps.length ? { steps } : null
    })
  }
  const usable = choices.some((choice) => choice.value === wanted && !choice.disabled)
  const chosen = base ?? (usable ? wanted : initialImage(choices, settings?.base_image))
  // The base a loaded configuration named when this host cannot build on it.
  const missingBase = base === undefined && !usable && !images.isPending ? wanted : undefined
  const { busy, error, submit } = useSubmit()
  const field = React.useId()

  const load = (config: ImportedBuild) => {
    if (config.name !== undefined) setName(config.name)
    if (config.description !== undefined) setDescription(config.description)
    if (config.icon !== undefined) setIcon(config.icon)
    if (config.internet !== undefined) setInternet(config.internet)
    setTemplateIds(templatesOf(config))
    setWarnings(config.warnings ?? [])
    setParts(config.parts ?? null)
    if (config.script !== undefined) setScript(config.script)
    setCompose(config.compose ? composeDraft(config.compose) : null)
    setServers(config.mcp ?? [])
    if (config.base_image !== undefined) {
      setWanted(config.base_image)
      setBase(undefined)
    }
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
                ...(description.trim() ? { description } : {}),
                base_image: chosen,
                ...(parts ? { parts } : { script }),
                internet,
                ...(compose ? { compose: composeBody(compose) } : {}),
                ...(servers.length ? { mcp: servers } : {}),
                ...(icon ? { icon } : {}),
                ...(templateIds.length
                  ? parts
                    ? { template_ids: templateIds }
                    : { template_id: templateIds[0] }
                  : {}),
              })
            )}
          >
            <DialogHeader>
              <DialogTitle>{t("images.build")}</DialogTitle>
              <DialogDescription>
                {!advanced
                  ? parts
                    ? t("build.simpleMerged", {
                        names: formatList(parts.map((part) => part.name)),
                      })
                    : t("build.simple")
                  : parts
                    ? t("build.fromTemplates", {
                        names: formatList(parts.map((part) => part.name)),
                      })
                    : templateIds.length
                      ? t("build.fromTemplate")
                      : t("build.description")}
              </DialogDescription>
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
                  <Select value={chosen ?? ""} onValueChange={setBase}>
                    <SelectTrigger id={`${field}-base`} className="w-full">
                      <SelectValue placeholder={t("build.noBase")} />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        {choices.map((choice) => (
                          <ImageChoiceItem key={choice.value} choice={choice} />
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                </Field>
              </div>
              {advanced ? (
                <>
                  <Field>
                    <FieldLabel htmlFor={`${field}-description`}>
                      {t("common.description")}
                    </FieldLabel>
                    <Textarea
                      id={`${field}-description`}
                      rows={2}
                      maxLength={4096}
                      placeholder={t("common.optional")}
                      value={description}
                      onChange={(event) => setDescription(event.target.value)}
                      // Advanced editing replaces the button that had focus with this field.
                      autoFocus={simple}
                    />
                  </Field>
                  <Field data-invalid={!!iconError}>
                    <FieldLabel htmlFor={`${field}-icon`}>{t("icons.label")}</FieldLabel>
                    <IconPicker
                      id={`${field}-icon`}
                      value={icon}
                      onChange={setIcon}
                      onError={setIconError}
                    />
                    <FieldError>{iconError}</FieldError>
                  </Field>
                </>
              ) : (
                <BuildSummary description={description} icon={icon} servers={servers} />
              )}
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
              {warnings.map((warning) => (
                <Alert key={warning}>
                  <RiAlertLine className="text-warning" />
                  <AlertDescription>{warning}</AlertDescription>
                </Alert>
              ))}
              {advanced && servers.length > 0 && (
                <BuildServers
                  servers={servers}
                  onRemove={(name) => setServers(servers.filter((server) => server.name !== name))}
                />
              )}
              {advanced ? (
                <Tabs defaultValue="build">
                  <TabsList>
                    <TabsTrigger value="build">
                      {t("build.script")}
                      {parts ? ` · ${formatNumber(parts.length)}` : ""}
                    </TabsTrigger>
                    <TabsTrigger value="compose">
                      {t("compose.name")}
                      {parameterCount ? ` · ${formatNumber(parameterCount)}` : ""}
                    </TabsTrigger>
                  </TabsList>
                  <TabsContent value="build">
                    <FieldGroup>
                      {parts ? (
                        <BuildParts
                          parts={parts}
                          onChange={(index, next) =>
                            setParts(
                              parts.map((part, other) =>
                                other === index ? { ...part, script: next } : part
                              )
                            )
                          }
                          onMove={(index, by) => {
                            const next = [...parts]
                            ;[next[index], next[index + by]] = [next[index + by], next[index]]
                            moveParts(next)
                          }}
                          onRemove={removePart}
                        />
                      ) : (
                        <Field>
                          <FieldLabel htmlFor={`${field}-script`}>{t("compose.script")}</FieldLabel>
                          <CodeEditor
                            id={`${field}-script`}
                            language="bash"
                            label={t("compose.script")}
                            minHeight={rowsHeight(12)}
                            maxHeight={500}
                            required
                            value={script}
                            onChange={setScript}
                          />
                          <FieldDescription>{t("build.scriptHint")}</FieldDescription>
                        </Field>
                      )}
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
              ) : (
                <Button
                  type="button"
                  variant="outline"
                  className="self-start"
                  onClick={() => setAdvanced(true)}
                >
                  <RiEqualizerLine data-icon="inline-start" />
                  {t("build.advanced")}
                </Button>
              )}
              <FieldError>{error}</FieldError>
            </FieldGroup>
            <Footer busy={busy} label={t("images.build")} disabled={!chosen}>
              {advanced && (
                <Button
                  type="button"
                  variant="outline"
                  className="sm:mr-auto"
                  onClick={() => setImporting(true)}
                >
                  <RiClipboardLine data-icon="inline-start" />
                  {t("build.importConfig")}
                </Button>
              )}
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
          <CodeEditor
            id={`${field}-config`}
            language="json"
            label={t("build.config")}
            minHeight={rowsHeight(12)}
            maxHeight={500}
            required
            placeholder={'{\n  "name": "Unity tools",\n  "base_image": "…",\n  "script": "…"\n}'}
            value={text}
            onChange={(next) => {
              setText(next)
              setError(undefined)
            }}
            invalid={!!error}
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

/** Deletes a volume and its files once confirmed. A volume a container mounts stays, as the
 * manager refuses it too, so the dialog names those containers instead of offering Delete. */
export function VolumeDeleteDialog({ name, ...props }: DialogProps & { name: string }) {
  const holders = volumeHolders(name, useContainers().data ?? [])
  const { busy, error, submit } = useSubmit()
  return (
    <AlertDialog {...props}>
      <AlertDialogContent>
        <form
          className="contents"
          onSubmit={submit(async () => {
            await api(volumeApi(name), undefined, "DELETE")
            closeDialog()
            toast.success(t("volumes.deleted", { name }))
            void refreshAll()
          })}
        >
          <AlertDialogHeader>
            <AlertDialogTitle className="break-all">
              {t("volumes.deleteTitle", { name })}
            </AlertDialogTitle>
            <AlertDialogDescription>{t("volumes.deleteDescription")}</AlertDialogDescription>
          </AlertDialogHeader>
          {holders.length > 0 && (
            <Alert>
              <RiAlertLine className="text-warning" />
              <AlertDescription>
                {t("volumes.deleteMounted", { containers: formatList(holders) })}
              </AlertDescription>
            </Alert>
          )}
          <FieldError>{error}</FieldError>
          <AlertDialogFooter>
            <AlertDialogCancel type="button">{t("common.cancel")}</AlertDialogCancel>
            <Button type="submit" variant="destructive" disabled={busy || holders.length > 0}>
              {busy && <Spinner data-icon="inline-start" />}
              {t("common.delete")}
            </Button>
          </AlertDialogFooter>
        </form>
      </AlertDialogContent>
    </AlertDialog>
  )
}

/** Deletes an image once confirmed: a saved image with its record, an import's record alone, or a
 * catalog image's download. An image a container was created from stays, as the manager refuses
 * it too, so the dialog names those containers instead of offering Delete. */
export function ImageDeleteDialog({ id, ...props }: DialogProps & { id: string }) {
  const images = useImages()
  // The image as the row showed it, so the closing dialog keeps its name once the list drops it.
  const [image] = React.useState(() => images.data?.find((item) => item.id === id))
  const containers = useContainers().data ?? []
  const holders = image ? imageHolders(image, containers, images.data ?? []) : []
  const name = image?.name ?? id
  const { busy, error, submit } = useSubmit()
  return (
    <AlertDialog {...props}>
      <AlertDialogContent>
        <form
          className="contents"
          onSubmit={submit(async () => {
            const result = await api<{ image_removed: boolean }>(
              `images/${encodeURIComponent(id)}`,
              undefined,
              "DELETE"
            )
            closeDialog()
            const notice = image ? deletedNotice(image, result.image_removed) : "images.deleted"
            toast.success(t(notice, { name }))
            void refreshAll()
          })}
        >
          <AlertDialogHeader>
            <AlertDialogTitle className="break-all">
              {t("images.deleteTitle", { name })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {image?.kind === "catalog"
                ? t("images.deleteCatalog")
                : image?.kind === "import"
                  ? t("images.deleteImport")
                  : t("images.deleteSaved")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {holders.length > 0 && (
            <Alert>
              <RiAlertLine className="text-warning" />
              <AlertDescription>
                {t("images.deleteUsed", { containers: formatList(holders) })}
              </AlertDescription>
            </Alert>
          )}
          <FieldError>{error}</FieldError>
          <AlertDialogFooter>
            <AlertDialogCancel type="button">{t("common.cancel")}</AlertDialogCancel>
            <Button type="submit" variant="destructive" disabled={busy || holders.length > 0}>
              {busy && <Spinner data-icon="inline-start" />}
              {t("common.delete")}
            </Button>
          </AlertDialogFooter>
        </form>
      </AlertDialogContent>
    </AlertDialog>
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
