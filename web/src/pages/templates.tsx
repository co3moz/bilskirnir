import * as React from "react"
import {
  RiAlertLine,
  RiApps2Line,
  RiCheckLine,
  RiHammerLine,
  RiRefreshLine,
  RiSearchLine,
} from "@remixicon/react"
import { toast } from "sonner"

import { ImageIcon } from "@/components/image-icon"
import { PageBody, SectionHeading } from "@/components/page"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import { openDialog } from "@/lib/dialogs"
import { plural, t } from "@/lib/i18n"
import { useImages, useSettings, useTemplates } from "@/lib/queries"
import {
  allTags,
  buildable,
  loadTemplate,
  matches,
  templateFile,
  type BrokenTemplate,
  type Template,
} from "@/lib/templates"

/** One template: its icon, what it is, whether an image was built from it, and Install. */
function TemplateCard({
  list,
  template,
  version,
  installed,
}: {
  list: string
  template: Template
  version: string
  installed: number
}) {
  const [busy, setBusy] = React.useState(false)
  const supported = buildable(template, version)
  const install = async () => {
    setBusy(true)
    try {
      // The build form shows everything the template runs before anything is built.
      openDialog({ kind: "build", config: await loadTemplate(list, template) })
    } catch (error) {
      toast.error((error as Error).message)
    } finally {
      setBusy(false)
    }
  }
  return (
    <Card size="sm">
      <CardHeader className="grid-cols-[auto_1fr] items-center gap-x-3">
        <ImageIcon
          src={templateFile(list, template.id, "png")}
          fallback={RiApps2Line}
          className="row-span-2 size-14 rounded-xl"
        />
        <CardTitle className="break-words">{template.name}</CardTitle>
        <div className="flex flex-wrap gap-1">
          {template.tags.map((tag) => (
            <Badge key={tag} variant="outline" className="text-muted-foreground">
              {tag}
            </Badge>
          ))}
        </div>
      </CardHeader>
      <CardContent className="flex-1">
        <CardDescription className="line-clamp-4 whitespace-pre-line">
          {template.description || t("templates.noDescription")}
        </CardDescription>
      </CardContent>
      <CardFooter className="flex-wrap justify-between gap-2">
        {installed ? (
          <Badge variant="outline" className="border-success/40 text-success">
            <RiCheckLine data-icon="inline-start" />
            {plural(installed, {
              one: "templates.installed.one",
              other: "templates.installed.other",
            })}
          </Badge>
        ) : (
          <span />
        )}
        {supported ? (
          <Button
            size="sm"
            variant={installed ? "outline" : "default"}
            disabled={busy}
            onClick={install}
            aria-label={t("templates.installOf", { name: template.name })}
          >
            {busy ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <RiHammerLine data-icon="inline-start" />
            )}
            {installed ? t("templates.installAgain") : t("templates.install")}
          </Button>
        ) : (
          <Badge variant="outline" className="border-warning/40 text-warning">
            {t("templates.requires", { version: template.requires ?? "" })}
          </Badge>
        )}
      </CardFooter>
    </Card>
  )
}

/** Entries the list names but the page cannot offer, so whoever maintains the list sees why. */
function BrokenTemplates({ broken }: { broken: BrokenTemplate[] }) {
  if (!broken.length) return null
  return (
    <Alert>
      <RiAlertLine className="text-warning" />
      <AlertTitle>
        {plural(broken.length, { one: "templates.broken.one", other: "templates.broken.other" })}
      </AlertTitle>
      <AlertDescription>
        <ul className="flex flex-col gap-1">
          {broken.map((entry) => (
            <li key={entry.id}>
              <code className="font-mono">{entry.id}</code>: {entry.reason}
            </li>
          ))}
        </ul>
      </AlertDescription>
    </Alert>
  )
}

/** Ready-made images from the template list; installing one opens the build form filled in. */
export function TemplatesPage() {
  const settings = useSettings().data
  const list = settings?.templates_url ?? null
  const templates = useTemplates(list)
  const images = useImages().data ?? []
  const [search, setSearch] = React.useState("")
  const [tag, setTag] = React.useState<string>()
  const all = React.useMemo(() => templates.data?.templates ?? [], [templates.data])
  const tags = React.useMemo(() => allTags(all), [all])
  const shown = all.filter(
    (template) => matches(template, search) && (!tag || template.tags.includes(tag))
  )
  const installed = (id: string) => images.filter((image) => image.template_id === id).length
  const source = list ? new URL(list).host : ""

  return (
    <PageBody>
      <SectionHeading
        title={t("templates.title")}
        description={list ? t("templates.description", { source }) : undefined}
      >
        {list && (
          <Button
            variant="outline"
            size="sm"
            disabled={templates.isFetching}
            onClick={() => void templates.refetch()}
          >
            {templates.isFetching ? (
              <Spinner data-icon="inline-start" />
            ) : (
              <RiRefreshLine data-icon="inline-start" />
            )}
            {t("templates.refresh")}
          </Button>
        )}
      </SectionHeading>
      {!list ? (
        <Empty className="mx-4 border lg:mx-6">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <RiApps2Line />
            </EmptyMedia>
            <EmptyTitle>{t("templates.off")}</EmptyTitle>
            <EmptyDescription>{t("templates.offHint")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : templates.isPending ? (
        <p className="flex items-center justify-center gap-2 px-4 py-12 text-sm text-muted-foreground lg:px-6">
          <Spinner />
          {t("templates.loading")}
        </p>
      ) : templates.error ? (
        <Empty className="mx-4 border lg:mx-6">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <RiAlertLine />
            </EmptyMedia>
            <EmptyTitle>{t("templates.failed")}</EmptyTitle>
            <EmptyDescription>{templates.error.message}</EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button variant="outline" onClick={() => void templates.refetch()}>
              <RiRefreshLine data-icon="inline-start" />
              {t("templates.retry")}
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <div className="flex flex-col gap-4 px-4 lg:px-6">
          {all.length > 0 && (
            <div className="flex flex-wrap items-center gap-2">
              <div className="relative">
                <RiSearchLine className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  type="search"
                  aria-label={t("templates.filter")}
                  placeholder={t("templates.filterPlaceholder")}
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  className="h-8 w-48 pl-8 sm:w-56"
                />
              </div>
              {tags.length > 0 && (
                <div className="flex flex-wrap gap-1" role="group" aria-label={t("templates.tags")}>
                  <Button
                    size="xs"
                    variant={tag ? "ghost" : "secondary"}
                    aria-pressed={!tag}
                    onClick={() => setTag(undefined)}
                  >
                    {t("common.all")}
                  </Button>
                  {tags.map((name) => (
                    <Button
                      key={name}
                      size="xs"
                      variant={tag === name ? "secondary" : "ghost"}
                      aria-pressed={tag === name}
                      onClick={() => setTag(tag === name ? undefined : name)}
                    >
                      {name}
                    </Button>
                  ))}
                </div>
              )}
            </div>
          )}
          {all.length === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <RiApps2Line />
                </EmptyMedia>
                <EmptyTitle>{t("templates.none")}</EmptyTitle>
                <EmptyDescription>{t("templates.noneHint")}</EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : shown.length ? (
            <div className="grid grid-cols-1 gap-4 @xl/main:grid-cols-2 @5xl/main:grid-cols-3 @[100rem]/main:grid-cols-4">
              {shown.map((template) => (
                <TemplateCard
                  key={template.id}
                  list={list}
                  template={template}
                  version={settings?.version ?? "0.0.0"}
                  installed={installed(template.id)}
                />
              ))}
            </div>
          ) : (
            <p className="py-12 text-center text-sm text-muted-foreground">
              {t("templates.noneMatch")}
            </p>
          )}
          <BrokenTemplates broken={templates.data.broken} />
        </div>
      )}
    </PageBody>
  )
}
