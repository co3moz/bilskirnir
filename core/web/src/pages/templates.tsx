import * as React from "react"
import {
  RiAlertLine,
  RiApps2Line,
  RiCheckLine,
  RiGitMergeLine,
  RiHammerLine,
  RiRefreshLine,
  RiSearchLine,
  RiTerminalBoxLine,
} from "@remixicon/react"
import { cn } from "cn"
import { toast } from "sonner"

import { ConceptIcon, ConceptMedia } from "@/components/concept"
import { ImageIcon } from "@/components/image-icon"
import { PageBody, SectionHeading } from "@/components/page"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
import { formatList, formatNumber, plural, t } from "@/lib/i18n"
import { useImages, useSettings, useTemplates } from "@/lib/queries"
import {
  allTags,
  buildable,
  groupByKind,
  loadTemplate,
  matches,
  mergeTemplates,
  templateFile,
  type BrokenTemplate,
  type Template,
  type TemplateKind,
} from "@/lib/templates"

const KIND_ICONS = { application: RiApps2Line, environment: RiTerminalBoxLine }

/**
 * One template: its icon, what it is, whether an image was built from it, and Install. While
 * templates are chosen for a merge, the whole card chooses it and shows where it comes in.
 */
function TemplateCard({
  list,
  template,
  version,
  installed,
  choosing,
  order,
  onChoose,
}: {
  list: string
  template: Template
  version: string
  installed: number
  choosing: boolean
  /** Where it comes in the merge, from 1; 0 when it is not chosen. */
  order: number
  onChoose: () => void
}) {
  const [busy, setBusy] = React.useState(false)
  const servers = template.mcp ?? []
  const supported = buildable(template, version)
  const install = async () => {
    setBusy(true)
    try {
      // The build form asks only for a name and a base image; its advanced editing shows
      // everything the template runs before anything is built.
      openDialog({ kind: "build", config: await loadTemplate(list, template), simple: true })
    } catch (error) {
      toast.error((error as Error).message)
    } finally {
      setBusy(false)
    }
  }
  const choosable = choosing && supported
  return (
    <Card
      size="sm"
      data-chosen={order > 0 || undefined}
      onClick={choosable ? onChoose : undefined}
      className={cn(
        choosable && "cursor-pointer transition-shadow hover:ring-foreground/25",
        choosing && !supported && "opacity-60",
        order > 0 && "ring-2 ring-primary hover:ring-primary"
      )}
    >
      <CardHeader
        className={cn(
          "items-center gap-x-3",
          choosing ? "grid-cols-[auto_1fr_auto]" : "grid-cols-[auto_1fr]"
        )}
      >
        <ImageIcon
          src={templateFile(list, template.id, "png")}
          concept="template"
          fallback={KIND_ICONS[template.kind]}
          className="row-span-2 size-14 rounded-xl"
        />
        <CardTitle className="break-words">{template.name}</CardTitle>
        {choosing && (
          <Checkbox
            checked={order > 0}
            disabled={!supported}
            onCheckedChange={onChoose}
            // The card chooses it too; one click must not choose it twice.
            onClick={(event) => event.stopPropagation()}
            aria-label={t("templates.merge.choose", { name: template.name })}
            className="row-span-2 self-start"
          />
        )}
        <div className="flex flex-wrap gap-1">
          {template.tags.map((tag) => (
            <Badge key={tag} variant="outline" className="text-muted-foreground">
              {tag}
            </Badge>
          ))}
        </div>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-2">
        <CardDescription className="line-clamp-4 whitespace-pre-line">
          {template.description || t("templates.noDescription")}
        </CardDescription>
        {servers.length > 0 && (
          <p className="flex items-start gap-1.5 text-xs text-muted-foreground">
            <ConceptIcon concept="mcp" className="mt-px size-3.5 shrink-0" />
            {t("images.mcpServers", { names: formatList(servers) })}
          </p>
        )}
      </CardContent>
      <CardFooter className="flex-wrap justify-between gap-2">
        {choosing ? (
          <>
            {order > 0 ? (
              <Badge variant="secondary" className="tabular-nums">
                {t("templates.merge.order", { number: formatNumber(order) })}
              </Badge>
            ) : (
              <span />
            )}
            {!supported && (
              <Badge variant="outline" className="border-warning/40 text-warning">
                {t("templates.requires", { version: template.requires ?? "" })}
              </Badge>
            )}
          </>
        ) : installed ? (
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
        {choosing ? null : supported ? (
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

/**
 * The cards of one kind. A list with more than one kind gives each its heading, and says what an
 * environment is for, since one is meant to be merged with an application.
 */
function TemplateSection({
  kind,
  heading,
  children,
}: {
  kind: TemplateKind
  heading: boolean
  children: React.ReactNode
}) {
  const Icon = KIND_ICONS[kind]
  const id = `templates-${kind}`
  return (
    <section aria-labelledby={heading ? id : undefined} className="flex flex-col gap-3">
      {heading && (
        <div className="flex flex-col gap-0.5">
          <h3 id={id} className="flex items-center gap-2 text-sm font-medium">
            <ConceptIcon concept="template" icon={Icon} className="size-4 shrink-0" />
            {t(`templates.kind.${kind}`)}
          </h3>
          {kind === "environment" && (
            <p className="text-sm text-muted-foreground">{t("templates.kind.environmentHint")}</p>
          )}
        </div>
      )}
      <div className="grid grid-cols-1 gap-4 @xl/main:grid-cols-2 @5xl/main:grid-cols-3 @[100rem]/main:grid-cols-4">
        {children}
      </div>
    </section>
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
  // Search and tags narrow every section alike; a list of one kind shows no headings.
  const groups = groupByKind(shown)
  const sectioned = React.useMemo(() => groupByKind(all).length > 1, [all])
  const installed = (id: string) =>
    images.filter((image) => image.template_ids?.includes(id)).length
  const source = list ? new URL(list).host : ""
  const version = settings?.version ?? "0.0.0"
  const mergeable = all.filter((template) => buildable(template, version)).length >= 2
  // Choosing templates to merge: their ids in the order they were chosen, which is the order
  // their scripts run in.
  const [choosing, setChoosing] = React.useState(false)
  const [chosen, setChosen] = React.useState<string[]>([])
  const [merging, setMerging] = React.useState(false)
  const choose = (id: string) =>
    setChosen((ids) => (ids.includes(id) ? ids.filter((other) => other !== id) : [...ids, id]))
  const stopChoosing = () => {
    setChoosing(false)
    setChosen([])
  }
  const merge = async () => {
    if (!list) return
    setMerging(true)
    try {
      const templates = chosen.flatMap((id) => all.filter((template) => template.id === id))
      const loaded = await Promise.all(
        templates.map(async (template) => ({
          template,
          config: await loadTemplate(list, template),
        }))
      )
      // As for one template; advanced editing shows every script the merge runs.
      openDialog({ kind: "build", config: mergeTemplates(loaded), simple: true })
      stopChoosing()
    } catch (error) {
      toast.error((error as Error).message)
    } finally {
      setMerging(false)
    }
  }

  return (
    <PageBody>
      <SectionHeading
        title={t("templates.title")}
        description={list ? t("templates.description", { source }) : undefined}
      >
        {list &&
          (choosing ? (
            <>
              <Button variant="outline" size="sm" onClick={stopChoosing}>
                {t("common.cancel")}
              </Button>
              <Button size="sm" disabled={chosen.length < 2 || merging} onClick={merge}>
                {merging ? (
                  <Spinner data-icon="inline-start" />
                ) : (
                  <RiGitMergeLine data-icon="inline-start" />
                )}
                {t("templates.merge.go")}
              </Button>
            </>
          ) : (
            <>
              {mergeable && (
                <Button variant="outline" size="sm" onClick={() => setChoosing(true)}>
                  <RiGitMergeLine data-icon="inline-start" />
                  {t("templates.merge.start")}
                </Button>
              )}
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
            </>
          ))}
      </SectionHeading>
      {!list ? (
        <Empty className="mx-4 border lg:mx-6">
          <EmptyHeader>
            <ConceptMedia concept="template" />
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
          {choosing && (
            <Alert>
              <RiGitMergeLine />
              <AlertTitle>
                {plural(chosen.length, {
                  one: "templates.merge.chosen.one",
                  other: "templates.merge.chosen.other",
                })}
              </AlertTitle>
              <AlertDescription>{t("templates.merge.hint")}</AlertDescription>
            </Alert>
          )}
          {all.length === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <ConceptMedia concept="template" />
                <EmptyTitle>{t("templates.none")}</EmptyTitle>
                <EmptyDescription>{t("templates.noneHint")}</EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : groups.length ? (
            <div className="flex flex-col gap-6">
              {groups.map((group) => (
                <TemplateSection key={group.kind} kind={group.kind} heading={sectioned}>
                  {group.templates.map((template) => (
                    <TemplateCard
                      key={template.id}
                      list={list}
                      template={template}
                      version={version}
                      installed={installed(template.id)}
                      choosing={choosing}
                      order={chosen.indexOf(template.id) + 1}
                      onChoose={() => choose(template.id)}
                    />
                  ))}
                </TemplateSection>
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
