import * as React from "react"
import { RiFolderOpenLine, RiPlayLine } from "@remixicon/react"

import { ConceptIcon } from "@/components/concept"
import { FileExplorer } from "@/components/file-explorer"
import { DockerAlert, ListFailed, listFailed, PageBody, SectionHeading } from "@/components/page"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { attempt, containerAction } from "@/lib/actions"
import {
  HOME,
  listingProblem,
  quickFolders,
  recentFolders,
  uploadProblem,
} from "@/lib/desktop-paths"
import { t } from "@/lib/i18n"
import { useContainers } from "@/lib/queries"
import { href, useRoute } from "@/lib/route"
import type { ContainerItem } from "@/lib/types"

const byName = (a: ContainerItem, b: ContainerItem) => a.spec.name.localeCompare(b.spec.name)

/** A running container's files as the desktop user sees them, with its usual folders beside. */
function ContainerFiles({ container }: { container: ContainerItem }) {
  const [recent] = React.useState(() => recentFolders(container.id))
  return (
    <FileExplorer
      base={`containers/${encodeURIComponent(container.id)}`}
      queryKey={["containers", container.id, "files"]}
      rootName="/"
      start={HOME}
      places={quickFolders(container, recent)}
      writeProblem={(path) => uploadProblem(container, path)}
      describeFailure={listingProblem}
      paneClassName="h-[max(16rem,calc(100svh-22rem))]"
    />
  )
}

/**
 * The Files page: pick a running container, then browse, open and edit its files. The chosen
 * container is part of the address, so a link or a reload opens the same one.
 */
export function FilesPage() {
  const route = useRoute()
  const chosenId = route.page === "files" ? route.container : undefined
  const containers = useContainers()
  const all = [...(containers.data ?? [])].sort(byName)
  const running = all.filter((item) => item.state === "running")
  const chosen = all.find((item) => item.id === chosenId)

  // Without a choice in the address, show the first running container.
  React.useEffect(() => {
    if (!chosenId && running[0]) location.replace(href({ page: "files", container: running[0].id }))
  }, [chosenId, running])

  return (
    <PageBody>
      <DockerAlert />
      <SectionHeading title={t("filesPage.title")} description={t("filesPage.description")}>
        {running.length > 0 && (
          <Select
            value={chosen && chosen.state === "running" ? chosen.id : ""}
            onValueChange={(id) => location.assign(href({ page: "files", container: id }))}
          >
            <SelectTrigger aria-label={t("filesPage.container")} className="w-64 max-w-full">
              <SelectValue placeholder={t("filesPage.choose")} />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {running.map((item) => (
                  <SelectItem key={item.id} value={item.id}>
                    <ConceptIcon concept="container" />
                    {item.spec.name}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        )}
      </SectionHeading>
      <div className="px-4 lg:px-6">
        {containers.isPending ? (
          <Skeleton className="h-96 rounded-2xl" />
        ) : listFailed(containers) ? (
          <ListFailed query={containers} className="border" />
        ) : chosen && chosen.state === "running" ? (
          <Card>
            <CardContent>
              <ContainerFiles key={chosen.id} container={chosen} />
            </CardContent>
          </Card>
        ) : chosen ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <RiFolderOpenLine />
              </EmptyMedia>
              <EmptyTitle>{t("filesPage.notRunning", { name: chosen.spec.name })}</EmptyTitle>
              <EmptyDescription>{t("filesPage.notRunningHint")}</EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button size="sm" onClick={() => attempt(() => containerAction(chosen.id, "start"))}>
                <RiPlayLine data-icon="inline-start" />
                {t("containers.start")}
              </Button>
            </EmptyContent>
          </Empty>
        ) : (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <RiFolderOpenLine />
              </EmptyMedia>
              <EmptyTitle>
                {chosenId ? t("filesPage.missing") : t("filesPage.noneRunning")}
              </EmptyTitle>
              <EmptyDescription>{t("filesPage.noneRunningHint")}</EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button size="sm" variant="outline" asChild>
                <a href={href({ page: "containers" })}>{t("nav.containers")}</a>
              </Button>
            </EmptyContent>
          </Empty>
        )}
      </div>
    </PageBody>
  )
}
