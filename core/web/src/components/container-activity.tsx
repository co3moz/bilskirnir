import { RiArrowRightLine } from "@remixicon/react"

import { ActivityFeed } from "@/components/activity-feed"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { useRecentActivity } from "@/lib/activity-queries"
import { t } from "@/lib/i18n"
import { navigate } from "@/lib/route"

/** The latest calls agents made on a container, for its details panel. */
export function ContainerActivity({ id }: { id: string }) {
  const recent = useRecentActivity({ container: id }, 5)
  const items = recent.data?.items ?? []
  // The Activity page's route ends the panel as it opens.
  const open = (entry?: number) => navigate({ page: "activity", container: id, entry })
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-xs text-muted-foreground">{t("activity.title")}</h3>
        {items.length > 0 && (
          <Button variant="link" size="sm" className="h-auto p-0 text-xs" onClick={() => open()}>
            {t("activity.section.showAll")}
            <RiArrowRightLine data-icon="inline-end" />
          </Button>
        )}
      </div>
      {recent.isPending ? (
        <Skeleton className="h-16 w-full" />
      ) : recent.error ? (
        <p className="text-xs text-destructive">{recent.error.message}</p>
      ) : items.length ? (
        <div className="-mx-3">
          <ActivityFeed
            entries={items}
            grouped={false}
            showContainer={false}
            onSelect={(entry) => open(entry.id)}
          />
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">{t("activity.section.none")}</p>
      )}
    </div>
  )
}
