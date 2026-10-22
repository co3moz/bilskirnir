import { RiArrowRightLine, RiPulseLine } from "@remixicon/react"

import { ActivityFeed } from "@/components/activity-feed"
import { Button } from "@/components/ui/button"
import {
  DrawerClose,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@/components/ui/drawer"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"
import { callerFilter } from "@/lib/activity"
import { useRecentActivity } from "@/lib/activity-queries"
import { t } from "@/lib/i18n"
import { useContainers } from "@/lib/queries"
import { navigate } from "@/lib/route"
import { SideDrawer, type DialogProps } from "./side-drawer"

/** The calls agents made on one MCP endpoint, the newest first, with a way to the whole log. */
export function ActivityDrawer({ endpoint, ...props }: DialogProps & { endpoint: string }) {
  const caller = callerFilter(endpoint)
  const recent = useRecentActivity({ caller }, 30)
  const containers = useContainers().data ?? []
  const names = new Map(containers.map((item) => [item.id, item.spec.name]))
  const items = recent.data?.items ?? []
  // The page's route ends this dialog as it opens.
  const open = (entry?: number) => navigate({ page: "activity", caller, entry })
  return (
    <SideDrawer {...props} wide>
      <DrawerHeader className="gap-1">
        <DrawerTitle>{t("activity.title")}</DrawerTitle>
        <DrawerDescription>
          {t("activity.endpoint.description", { path: `/mcp/${endpoint}` })}
        </DrawerDescription>
      </DrawerHeader>
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2">
        {recent.isPending ? (
          <div className="flex flex-col gap-2 px-2">
            <Skeleton className="h-12 w-full" />
            <Skeleton className="h-12 w-full" />
          </div>
        ) : recent.error ? (
          <p className="px-2 text-sm text-destructive">{recent.error.message}</p>
        ) : items.length ? (
          <ActivityFeed
            entries={items}
            containerNames={names}
            onSelect={(entry) => open(entry.id)}
          />
        ) : (
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <RiPulseLine />
              </EmptyMedia>
              <EmptyTitle>{t("activity.endpoint.none")}</EmptyTitle>
              <EmptyDescription>{t("activity.endpoint.noneHint")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </div>
      <DrawerFooter className="flex-row flex-wrap justify-end">
        <Button onClick={() => open()}>
          {t("activity.openPage")}
          <RiArrowRightLine data-icon="inline-end" />
        </Button>
        <DrawerClose asChild>
          <Button variant="outline">{t("common.done")}</Button>
        </DrawerClose>
      </DrawerFooter>
    </SideDrawer>
  )
}
