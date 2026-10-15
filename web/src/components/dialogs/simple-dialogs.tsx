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
import { refreshAll, useEnvironments, useImages, useProfiles, useSettings } from "@/lib/queries"
import { templateBody, templateDraft, type TemplateDraft } from "@/lib/templates"
import { useSubmit } from "./environment-dialogs"
import type { DialogProps } from "./side-drawer"
import { TemplateFields } from "./template-dialog"

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
          Cancel
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
  const item = useEnvironments().data?.find((entry) => entry.environment.id === id)
  const [name, setName] = React.useState("")
  const [description, setDescription] = React.useState("")
  const { busy, error, submit } = useSubmit()
  const field = React.useId()
  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-md">
        <form
          className="flex flex-col gap-4"
          onSubmit={submit(() => runJob(`environments/${id}/snapshot`, { name, description }))}
        >
          <DialogHeader>
            <DialogTitle>Create snapshot</DialogTitle>
            <DialogDescription>
              Saves the files of {item?.environment.spec.name ?? "this environment"} as an image you
              can start new environments from.
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={`${field}-name`}>Name</FieldLabel>
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
              <FieldLabel htmlFor={`${field}-description`}>Description</FieldLabel>
              <Textarea
                id={`${field}-description`}
                rows={3}
                maxLength={4096}
                value={description}
                onChange={(event) => setDescription(event.target.value)}
              />
              <FieldDescription>
                Briefly pauses the container. Volumes and running application state are excluded.
              </FieldDescription>
            </Field>
            <FieldError>{error}</FieldError>
          </FieldGroup>
          <Footer busy={busy} label="Create snapshot" />
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function DeleteDialog({ id, ...props }: DialogProps & { id: string }) {
  const item = useEnvironments().data?.find((entry) => entry.environment.id === id)
  const { busy, error, submit } = useSubmit()
  return (
    <AlertDialog {...props}>
      <AlertDialogContent>
        <form
          className="contents"
          onSubmit={submit(() => runJob(`environments/${id}`, undefined, "DELETE"))}
        >
          <AlertDialogHeader>
            <AlertDialogTitle>
              Delete {item?.environment.spec.name ?? "environment"}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              The desktop and its unsaved changes are deleted. Volumes and snapshots are kept.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <FieldError>{error}</FieldError>
          <AlertDialogFooter>
            <AlertDialogCancel type="button">Cancel</AlertDialogCancel>
            <Button type="submit" variant="destructive" disabled={busy}>
              {busy && <Spinner data-icon="inline-start" />}
              Delete
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
  const choices = imageChoices(
    useProfiles().data ?? [],
    useImages().data ?? [],
    settings?.base_image
  )
  const [name, setName] = React.useState("")
  const [base, setBase] = React.useState<string | undefined>()
  const [script, setScript] = React.useState(DEFAULT_SCRIPT)
  const [internet, setInternet] = React.useState(true)
  const [template, setTemplate] = React.useState<TemplateDraft | null>(null)
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
    setTemplate(config.template ? templateDraft(config.template) : null)
    const wanted = config.base_image
    const usable = choices.some((choice) => choice.value === wanted && !choice.disabled)
    if (usable) setBase(wanted)
    setMissingBase(usable ? undefined : wanted)
    setImporting(false)
  }

  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-2xl">
        {importing ? (
          <ImportConfiguration onLoad={load} onBack={() => setImporting(false)} />
        ) : (
          <form
            className="flex flex-col gap-4"
            onSubmit={submit(() =>
              runJob("images/build", {
                name,
                base_image: chosen,
                script,
                internet,
                ...(template ? { template: templateBody(template) } : {}),
              })
            )}
          >
            <DialogHeader>
              <DialogTitle>Build image</DialogTitle>
              <DialogDescription>
                Runs a setup script on top of an image and saves the result as a new image.
              </DialogDescription>
            </DialogHeader>
            <FieldGroup>
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <Field>
                  <FieldLabel htmlFor={`${field}-name`}>Name</FieldLabel>
                  <Input
                    id={`${field}-name`}
                    required
                    maxLength={100}
                    placeholder="Unity tools"
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    autoFocus
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${field}-base`}>Base image</FieldLabel>
                  <Select
                    value={chosen ?? ""}
                    onValueChange={(value) => {
                      setBase(value)
                      setMissingBase(undefined)
                    }}
                  >
                    <SelectTrigger id={`${field}-base`} className="w-full">
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
              </div>
              {missingBase && (
                <Alert>
                  <RiAlertLine className="text-warning" />
                  <AlertDescription>
                    The configuration builds on{" "}
                    <code className="font-mono break-all">{missingBase}</code>, which is not
                    available here. Choose a base image.
                  </AlertDescription>
                </Alert>
              )}
              <Tabs defaultValue="build">
                <TabsList>
                  <TabsTrigger value="build">Build script</TabsTrigger>
                  <TabsTrigger value="template">
                    Template
                    {template?.parameters.length ? ` · ${template.parameters.length}` : ""}
                  </TabsTrigger>
                </TabsList>
                <TabsContent value="build">
                  <FieldGroup>
                    <Field>
                      <FieldLabel htmlFor={`${field}-script`}>Setup script</FieldLabel>
                      <Textarea
                        id={`${field}-script`}
                        rows={12}
                        required
                        spellCheck={false}
                        value={script}
                        onChange={(event) => setScript(event.target.value)}
                        className="font-mono text-xs"
                      />
                      <FieldDescription>Runs as root during the build.</FieldDescription>
                    </Field>
                    <Field orientation="horizontal">
                      <Switch
                        id={`${field}-internet`}
                        checked={internet}
                        onCheckedChange={setInternet}
                      />
                      <FieldLabel htmlFor={`${field}-internet`}>Internet during build</FieldLabel>
                    </Field>
                  </FieldGroup>
                </TabsContent>
                <TabsContent
                  value="template"
                  className="-mx-4 max-h-[40vh] overflow-y-auto px-4 py-1 sm:max-h-[min(30rem,55vh)]"
                >
                  {template ? (
                    <div className="flex flex-col gap-4">
                      <TemplateFields draft={template} onChange={setTemplate} />
                      <Button
                        type="button"
                        variant="outline"
                        className="self-start text-destructive"
                        onClick={() => setTemplate(null)}
                      >
                        Remove template
                      </Button>
                    </div>
                  ) : (
                    <div className="flex flex-col items-start gap-3 py-2">
                      <p className="text-sm text-muted-foreground">
                        A template asks for values, such as an account, when an environment is
                        created from the image, and runs a setup script with them once the desktop
                        is ready.
                      </p>
                      <Button
                        type="button"
                        variant="outline"
                        onClick={() => setTemplate(templateDraft())}
                      >
                        Add template
                      </Button>
                    </div>
                  )}
                </TabsContent>
              </Tabs>
              <FieldError>{error}</FieldError>
            </FieldGroup>
            <Footer busy={busy} label="Build image" disabled={!chosen}>
              <Button
                type="button"
                variant="outline"
                className="sm:mr-auto"
                onClick={() => setImporting(true)}
              >
                <RiClipboardLine data-icon="inline-start" />
                Import configuration
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
      className="flex flex-col gap-4"
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
        <DialogTitle>Import configuration</DialogTitle>
        <DialogDescription>
          Paste the JSON that Export copied from a built image. It fills in the name, base image,
          script, internet setting and template, which you can still change before building.
        </DialogDescription>
      </DialogHeader>
      <FieldGroup>
        <Field data-invalid={!!error}>
          <FieldLabel htmlFor={`${field}-config`}>Configuration</FieldLabel>
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
            className="font-mono text-xs"
            autoFocus
          />
          <FieldError>{error}</FieldError>
        </Field>
      </FieldGroup>
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onBack}>
          Back
        </Button>
        <Button type="submit">Load</Button>
      </DialogFooter>
    </form>
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
            toast.success(`Created volume ${name}.`)
            void refreshAll()
          })}
        >
          <DialogHeader>
            <DialogTitle>New volume</DialogTitle>
            <DialogDescription>
              A named Docker volume you can mount into environments. Its files outlive the
              environments and are not part of snapshots.
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={`${field}-name`}>Name</FieldLabel>
              <Input
                id={`${field}-name`}
                required
                pattern="[a-zA-Z0-9][a-zA-Z0-9_.\-]+"
                title="Letters, digits, dots, dashes and underscores, starting with a letter or digit"
                placeholder="unity-projects"
                value={name}
                onChange={(event) => setName(event.target.value)}
                className="font-mono"
                autoFocus
              />
            </Field>
            <FieldError>{error}</FieldError>
          </FieldGroup>
          <Footer busy={busy} label="Create" />
        </form>
      </DialogContent>
    </Dialog>
  )
}
