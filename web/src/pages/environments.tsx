import * as React from "react"
import { RiComputerLine, RiAddLine, RiSearchLine } from "@remixicon/react"

import { ViewTabs } from "@/components/data-table"
import { EnvironmentCard } from "@/components/environment-card"
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
import { useEnvironments, useProfiles } from "@/lib/queries"
import type { EnvironmentItem } from "@/lib/types"

const views = {
  all: () => true,
  running: (item: EnvironmentItem) => item.state === "running",
  stopped: (item: EnvironmentItem) => item.state !== "running",
  attention: needsAttention,
} satisfies Record<string, (item: EnvironmentItem) => boolean>
type View = keyof typeof views

/** Every desktop as a card with its live preview. */
export function EnvironmentsPage() {
  const environments = useEnvironments()
  const profiles = useProfiles().data ?? []
  const [view, setView] = React.useState<View>("all")
  const [search, setSearch] = React.useState("")
  const all = environments.data ?? []
  const query = search.trim().toLowerCase()
  const shown = all
    .filter(views[view])
    .filter((item) => !query || item.environment.spec.name.toLowerCase().includes(query))
    .sort((a, b) => a.environment.spec.name.localeCompare(b.environment.spec.name))
  const count = (key: View) => all.filter(views[key]).length

  return (
    <PageBody>
      <DockerAlert />
      {environments.isPending ? (
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
            <EmptyTitle>No environments</EmptyTitle>
            <EmptyDescription>
              Create a desktop from a profile; it downloads the first time you use it.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => openDialog({ kind: "create" })}>
              <RiAddLine data-icon="inline-start" />
              New environment
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
                { value: "all", label: "All" },
                { value: "running", label: "Running", count: count("running") },
                { value: "stopped", label: "Stopped", count: count("stopped") },
                { value: "attention", label: "Needs attention", count: count("attention") },
              ]}
            />
            <div className="relative">
              <RiSearchLine className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                type="search"
                aria-label="Filter environments"
                placeholder="Filter by name"
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
                  <EnvironmentCard key={item.environment.id} item={item} profiles={profiles} />
                ))}
              </div>
            ) : (
              <p className="px-4 py-12 text-center text-sm text-muted-foreground lg:px-6">
                No environments in this view.
              </p>
            )}
          </TabsContent>
        </Tabs>
      )}
    </PageBody>
  )
}
