import * as React from "react"
import { RiArrowUpCircleLine, RiExternalLinkLine } from "@remixicon/react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Spinner } from "@/components/ui/spinner"
import { openDialog } from "@/lib/dialogs"
import { t } from "@/lib/i18n"
import { useUpdate } from "@/lib/queries"
import { followUpdate, updatedFrom } from "@/lib/update"

/**
 * A newer release in the sidebar: what it is, its notes and, where the WSL package takes the
 * request, the update. While one runs, the page follows it to the new manager.
 */
export function UpdateNotice() {
  const update = useUpdate().data
  const current = update?.current
  const job = update?.job_id

  React.useEffect(() => {
    if (!current) return
    const from = updatedFrom()
    if (from && from !== current) toast.success(t("update.done", { version: current }))
  }, [current])
  React.useEffect(() => {
    if (job && current) followUpdate(current)
  }, [job, current])

  if (!update?.enabled) return null
  if (job)
    return (
      <Card size="sm" className="shadow-none">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-sm">
            <Spinner className="size-4" />
            {t("update.running")}
          </CardTitle>
          <CardDescription className="text-xs">
            {/* Once the new manager answers, it follows the rest, such as the desktop images. */}
            {update.available ? t("update.runningDetail") : t("update.finishing")}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button
            size="sm"
            variant="outline"
            className="w-full"
            onClick={() => openDialog({ kind: "job", id: job })}
          >
            {t("update.showLog")}
          </Button>
        </CardContent>
      </Card>
    )
  if (!update.available || !update.newest) return null
  return (
    <Card size="sm" className="shadow-none">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-sm">
          <RiArrowUpCircleLine className="size-4 shrink-0 text-info" />
          {t("update.available", { version: update.newest })}
        </CardTitle>
        <CardDescription className="text-xs">
          {update.installable
            ? t("update.current", { version: update.current })
            : t("update.manual", { version: update.current })}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        {update.installable && (
          <Button size="sm" onClick={() => openDialog({ kind: "update" })}>
            {t("update.install")}
          </Button>
        )}
        {update.release_url && (
          <Button size="sm" variant="outline" asChild>
            <a href={update.release_url} target="_blank" rel="noreferrer">
              {t("update.notes")}
              <RiExternalLinkLine data-icon="inline-end" />
            </a>
          </Button>
        )}
      </CardContent>
    </Card>
  )
}
