import * as React from "react"
import {
  RiAlertLine,
  RiArrowUpCircleLine,
  RiExternalLinkLine,
  RiLockLine,
  RiRefreshLine,
} from "@remixicon/react"
import { toast } from "sonner"

import { PageBody, SectionHeading } from "@/components/page"
import { StatusBadge } from "@/components/status-badge"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { shortDate } from "@/lib/format"
import { has, t } from "@/lib/i18n"
import { queryClient, useConfiguration, useUpdate } from "@/lib/queries"
import { releaseState, sections, shownValue } from "@/lib/settings"
import type { ConfigVariable, UpdateStatus } from "@/lib/types"

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

function Value({ variable }: { variable: ConfigVariable }) {
  const shown = shownValue(variable)
  if (shown.kind === "secret")
    return (
      <span className="inline-flex items-center gap-1.5 text-muted-foreground">
        <RiLockLine className="size-3.5 shrink-0" />
        {shown.set ? t("settings.secretSet") : t("settings.notSet")}
      </span>
    )
  if (shown.kind === "empty")
    return <span className="text-muted-foreground">{t("settings.notSet")}</span>
  return <span className="font-mono text-xs break-all">{shown.text}</span>
}

/** One section of the configuration: each setting's name, what it does and its value. */
function SectionCard({ title, variables }: { title: string; variables: ConfigVariable[] }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="text-sm">{title}</CardTitle>
      </CardHeader>
      <CardContent className="@container/settings">
        <dl className="flex flex-col divide-y">
          {variables.map((variable) => {
            const about = `settings.about.${variable.name}`
            return (
              <div
                key={variable.name}
                className="grid gap-1 py-2.5 first:pt-0 last:pb-0 @md/settings:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] @md/settings:gap-4"
              >
                <dt className="flex min-w-0 flex-col gap-0.5">
                  <code className="font-mono text-xs font-medium break-all">{variable.name}</code>
                  {has(about) && <span className="text-xs text-muted-foreground">{t(about)}</span>}
                </dt>
                <dd className="min-w-0 text-sm @md/settings:text-right">
                  <Value variable={variable} />
                </dd>
              </div>
            )
          })}
        </dl>
      </CardContent>
    </Card>
  )
}

export function SettingsPage() {
  const configuration = useConfiguration()
  return (
    <PageBody>
      <SectionHeading title={t("settings.version")} />
      <div className="px-4 lg:px-6">
        <VersionCard />
      </div>
      <SectionHeading
        title={t("settings.configuration")}
        description={t("settings.configurationHint")}
      />
      <div className="grid grid-cols-1 items-start gap-4 px-4 lg:px-6 @5xl/main:grid-cols-2">
        {configuration.error ? (
          <Alert variant="destructive">
            <RiAlertLine />
            <AlertTitle>{t("settings.configurationFailed")}</AlertTitle>
            <AlertDescription>{configuration.error.message}</AlertDescription>
          </Alert>
        ) : !configuration.data ? (
          <Skeleton className="h-72 rounded-2xl" />
        ) : (
          sections(configuration.data.variables).map(({ section, variables }) => (
            <SectionCard
              key={section}
              title={t(`settings.section.${section}`)}
              variables={variables}
            />
          ))
        )}
      </div>
    </PageBody>
  )
}
