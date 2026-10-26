import * as React from "react"
import {
  RiArrowRightSLine,
  RiArrowUpCircleLine,
  RiExternalLinkLine,
  RiListCheck3,
  RiNodeTree,
  RiRefreshLine,
} from "@remixicon/react"
import { toast } from "sonner"

import { pageTitle } from "@/components/app-sidebar"
import { BackupCard } from "@/components/backup-card"
import { ConfigurationEditor } from "@/components/configuration-editor"
import { PageBody, SectionHeading } from "@/components/page"
import { StatusBadge } from "@/components/status-badge"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Spinner } from "@/components/ui/spinner"
import { api } from "@/lib/api"
import { openDialog } from "@/lib/dialogs"
import { jobActive, shortDate } from "@/lib/format"
import { plural, t } from "@/lib/i18n"
import { queryClient, useJobs, useUpdate } from "@/lib/queries"
import { href, type Page } from "@/lib/route"
import { releaseState } from "@/lib/settings"
import type { UpdateStatus } from "@/lib/types"

function ReleaseBadge({ state }: { state: ReturnType<typeof releaseState> }) {
  const label = t(`settings.state.${state}`)
  if (state === "available")
    return (
      <Badge variant="outline" className="px-1.5 text-muted-foreground">
        <RiArrowUpCircleLine className="text-info" />
        {label}
      </Badge>
    )
  const tone = {
    current: "success",
    updating: "info",
    failed: "danger",
    off: "muted",
    unchecked: "muted",
  } as const
  return <StatusBadge state={state} tone={tone[state]} label={label} />
}

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="font-medium">{children}</dd>
    </div>
  )
}

/** This release against the newest one, with the manual check and the update. */
function VersionCard() {
  const update = useUpdate().data
  const [checking, setChecking] = React.useState(false)
  if (!update) return <Skeleton className="h-48 rounded-2xl" />
  const state = releaseState(update)
  const job = update.job_id

  const check = async () => {
    setChecking(true)
    try {
      const status = await api<UpdateStatus>("update/check", {})
      queryClient.setQueryData(["update"], status)
      if (status.available && status.newest)
        toast.info(t("update.available", { version: status.newest }))
      else if (status.error) toast.error(t("settings.checkFailed", { error: status.error }))
      else toast.success(t("settings.upToDate", { version: status.current }))
    } catch (error) {
      toast.error((error as Error).message)
    } finally {
      setChecking(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("settings.release", { version: update.current })}</CardTitle>
        <CardDescription>
          {state === "off" ? t("settings.checksOff") : t("settings.versionHint")}
        </CardDescription>
        <CardAction>
          <ReleaseBadge state={state} />
        </CardAction>
      </CardHeader>
      {update.enabled && (
        <CardContent className="flex flex-col gap-4">
          <dl className="grid grid-cols-2 gap-4 sm:max-w-md">
            <Fact label={t("settings.newest")}>
              {update.newest ? (
                <span className="font-mono">{update.newest}</span>
              ) : (
                <span className="text-muted-foreground">{t("common.unknown")}</span>
              )}
            </Fact>
            <Fact label={t("settings.checkedAt")}>
              {update.checked_at ? (
                shortDate(update.checked_at)
              ) : (
                <span className="text-muted-foreground">{t("settings.never")}</span>
              )}
            </Fact>
          </dl>
          {update.error && (
            <p className="text-sm text-destructive">
              {t("settings.checkFailed", { error: update.error })}
            </p>
          )}
          {state === "updating" && (
            <p className="text-sm text-muted-foreground">
              {update.available ? t("update.runningDetail") : t("update.finishing")}
            </p>
          )}
          {state === "available" && !update.installable && (
            <p className="text-sm text-muted-foreground">
              {t("update.manual", { version: update.current })}
            </p>
          )}
        </CardContent>
      )}
      {update.enabled && (
        <CardFooter className="flex flex-wrap gap-2">
          {job ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => openDialog({ kind: "job", id: job })}
            >
              <Spinner data-icon="inline-start" />
              {t("update.showLog")}
            </Button>
          ) : (
            <Button variant="outline" size="sm" disabled={checking} onClick={() => void check()}>
              {checking ? <Spinner data-icon="inline-start" /> : <RiRefreshLine />}
              {t("settings.check")}
            </Button>
          )}
          {state === "available" && update.installable && update.newest && (
            <Button size="sm" onClick={() => openDialog({ kind: "update" })}>
              <RiArrowUpCircleLine />
              {t("settings.updateTo", { version: update.newest })}
            </Button>
          )}
          {update.available && update.release_url && (
            <Button variant="ghost" size="sm" asChild>
              <a href={update.release_url} target="_blank" rel="noreferrer">
                {t("update.notes")}
                <RiExternalLinkLine data-icon="inline-end" />
              </a>
            </Button>
          )}
        </CardFooter>
      )}
    </Card>
  )
}

function PageLink({
  page,
  icon,
  description,
  badge,
}: {
  page: Page
  icon: React.ReactNode
  description: string
  badge?: string
}) {
  return (
    <a
      href={href({ page })}
      className="flex items-center gap-3 rounded-2xl bg-card p-4 text-sm ring-1 ring-foreground/10 outline-none hover:bg-accent/50 focus-visible:ring-3 focus-visible:ring-ring/50"
    >
      <span className="flex size-9 shrink-0 items-center justify-center rounded-xl bg-muted text-muted-foreground [&_svg]:size-4.5">
        {icon}
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex flex-wrap items-center gap-2 font-medium">
          {pageTitle(page)}
          {badge && (
            <Badge variant="outline" className="px-1.5 font-normal text-muted-foreground">
              {badge}
            </Badge>
          )}
        </span>
        <span className="text-muted-foreground">{description}</span>
      </span>
      <RiArrowRightSLine className="size-4 shrink-0 text-muted-foreground" />
    </a>
  )
}

/** The pages that live under Settings rather than in the sidebar. */
function PageLinks() {
  const active = useJobs().data?.filter(jobActive).length ?? 0
  return (
    <div className="grid gap-4 px-4 lg:px-6 @xl/main:grid-cols-2">
      <PageLink
        page="jobs"
        icon={<RiListCheck3 />}
        description={t("settings.jobsHint")}
        badge={
          active
            ? plural(active, { one: "jobs.inProgress.one", other: "jobs.inProgress.other" })
            : undefined
        }
      />
      <PageLink
        page="dependencies"
        icon={<RiNodeTree />}
        description={t("settings.dependenciesHint")}
      />
    </div>
  )
}

export function SettingsPage() {
  return (
    <PageBody>
      <SectionHeading title={t("settings.version")} />
      <div className="px-4 lg:px-6">
        <VersionCard />
      </div>
      <SectionHeading title={t("settings.more")} />
      <PageLinks />
      <SectionHeading title={t("backup.title")} description={t("backup.description")} />
      <div className="px-4 lg:px-6">
        <BackupCard />
      </div>
      <SectionHeading
        title={t("settings.configuration")}
        description={t("settings.configurationHint")}
      />
      <ConfigurationEditor />
    </PageBody>
  )
}
