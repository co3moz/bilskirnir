import * as React from "react"
import { RiComputerLine, RiAddLine, RiSearchLine } from "@remixicon/react"

import { ViewTabs } from "@/components/data-table"
import { ContainerCard } from "@/components/container-card"
import { DockerAlert, PageBody } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsContent } from "@/components/ui/tabs"
import { openDialog } from "@/lib/dialogs"
import { needsAttention } from "@/lib/format"
import { t } from "@/lib/i18n"
import { useContainers, useImages } from "@/lib/queries"
import type { ContainerItem } from "@/lib/types"

const views = {
  all: () => true,
  running: (item: ContainerItem) => item.state === "running",
  stopped: (item: ContainerItem) => item.state !== "running",
  attention: needsAttention,
} satisfies Record<string, (item: ContainerItem) => boolean>
type View = keyof typeof views

/** Every container as a card with its live preview. */
export function ContainersPage() {
  const containers = useContainers()
  const images = useImages().data ?? []
  const [view, setView] = React.useState<View>("all")
  const [search, setSearch] = React.useState("")
  const all = containers.data ?? []
  const query = search.trim().toLowerCase()
  const shown = all
    .filter(views[view])
    .filter((item) => !query || item.spec.name.toLowerCase().includes(query))
    .sort((a, b) => a.spec.name.localeCompare(b.spec.name))
  const count = (key: View) => all.filter(views[key]).length

  return (
    <PageBody>
      <DockerAlert />
      {containers.isPending ? (
        <div className="grid grid-cols-1 gap-4 px-4 lg:px-6 @xl/main:grid-cols-2 @5xl/main:grid-cols-3">
          {[0, 1, 2].map((index) => (
            <Card key={index} className="gap-4 p-3">
              <Skeleton className="aspect-video w-full" />
              <Skeleton className="h-5 w-1/2" />
              <Skeleton className="h-4 w-3/4" />
            </Card>
          ))}
        </div>
      ) : all.length === 0 ? (
        <Empty className="mx-4 border lg:mx-6">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <RiComputerLine />
            </EmptyMedia>
            <EmptyTitle>{t("containers.none")}</EmptyTitle>
            <EmptyDescription>{t("containers.noneHint")}</EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => openDialog({ kind: "create" })}>
              <RiAddLine data-icon="inline-start" />
              {t("containers.new")}
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <Tabs value={view} onValueChange={(value) => setView(value as View)} className="gap-4">
          <div className="flex flex-wrap items-center justify-between gap-2 px-4 lg:px-6">
            <ViewTabs
              value={view}
              onChange={(value) => setView(value as View)}
              views={[
                { value: "all", label: t("common.all") },
                { value: "running", label: t("state.running"), count: count("running") },
                { value: "stopped", label: t("state.exited"), count: count("stopped") },
                {
                  value: "attention",
                  label: t("containers.view.attention"),
                  count: count("attention"),
                },
              ]}
            />
            <div className="relative">
              <RiSearchLine className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                type="search"
                aria-label={t("containers.filter")}
                placeholder={t("containers.filterPlaceholder")}
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                className="h-8 w-48 pl-8 sm:w-56"
              />
            </div>
          </div>
          <TabsContent value={view}>
            {shown.length ? (
              <div className="grid grid-cols-1 gap-4 px-4 lg:px-6 @xl/main:grid-cols-2 @5xl/main:grid-cols-3 @[100rem]/main:grid-cols-4">
                {shown.map((item) => (
                  <ContainerCard key={item.id} item={item} images={images} />
                ))}
              </div>
            ) : (
              <p className="px-4 py-12 text-center text-sm text-muted-foreground lg:px-6">
                {t("containers.noneInView")}
              </p>
            )}
          </TabsContent>
        </Tabs>
      )}
    </PageBody>
  )
}
