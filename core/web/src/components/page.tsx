import { RiAlertLine, RiErrorWarningLine } from "@remixicon/react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Spinner } from "@/components/ui/spinner"
import { t } from "@/lib/i18n"
import { useSystem } from "@/lib/queries"

/** The content column of dashboard-01, sized by its `@container/main` queries. */
export function PageBody({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col">
      <div className="@container/main flex flex-1 flex-col gap-2">
        <div className="flex flex-col gap-4 py-4 md:gap-6 md:py-6">{children}</div>
      </div>
    </div>
  )
}

/** A section heading with its actions, inside the page's side padding. */
export function SectionHeading({
  title,
  description,
  children,
}: {
  title: string
  description?: React.ReactNode
  children?: React.ReactNode
}) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-2 px-4 lg:px-6">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-base font-medium">{title}</h2>
        {description && <p className="text-sm text-muted-foreground">{description}</p>}
      </div>
      {children && <div className="flex items-center gap-2">{children}</div>}
    </div>
  )
}

export function DockerAlert() {
  const system = useSystem().data
  if (!system || system.connected) return null
  return (
    <div className="px-4 lg:px-6">
      <Alert variant="destructive">
        <RiAlertLine />
        <AlertTitle>{t("docker.unreachable")}</AlertTitle>
        <AlertDescription>{system.error || t("docker.checkDaemon")}</AlertDescription>
      </Alert>
    </div>
  )
}

/** A list query as `ListFailed` reads it. */
type ListQuery = { data: unknown; error: Error | null; isFetching: boolean; refetch: () => unknown }

/** Whether a list could not be read and has nothing to show, which is not the same as empty. */
export const listFailed = (query: ListQuery) => !!query.error && query.data === undefined

/**
 * A list that could not be read, in place of the empty state that would say there is nothing:
 * the manager's reason and **Try again**. `row` fits it into a table's empty row.
 */
export function ListFailed({
  query,
  row,
  className = "mx-4 w-auto border lg:mx-6",
}: {
  query: ListQuery
  row?: boolean
  className?: string
}) {
  const retry = (
    <Button variant="outline" size="sm" disabled={query.isFetching} onClick={() => query.refetch()}>
      {query.isFetching && <Spinner data-icon="inline-start" />}
      {t("errors.retry")}
    </Button>
  )
  if (row)
    return (
      <span className="flex flex-col items-center gap-1" role="alert">
        <span className="font-medium text-destructive">{t("list.failed")}</span>
        <span className="break-words">{query.error?.message}</span>
        <span className="mt-1">{retry}</span>
      </span>
    )
  return (
    <Empty className={className} role="alert">
      <EmptyHeader>
        <EmptyMedia variant="icon" className="text-destructive">
          <RiErrorWarningLine />
        </EmptyMedia>
        <EmptyTitle>{t("list.failed")}</EmptyTitle>
        <EmptyDescription className="break-words">{query.error?.message}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>{retry}</EmptyContent>
    </Empty>
  )
}
