import * as React from "react"
import { RiAddLine, RiArrowLeftLine, RiGroupLine, RiLinkUnlinkM } from "@remixicon/react"

import { ConceptIcon, ConceptMedia } from "@/components/concept"
import { AliasBadges, ContainerCard } from "@/components/container-card"
import { DockerAlert, ListFailed, listFailed, PageBody, SectionHeading } from "@/components/page"
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
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { attempt, runJob } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { groupNetwork, knownGroups, type KnownGroup } from "@/lib/groups"
import { formatNumber, rich, t } from "@/lib/i18n"
import { useContainers, useImages } from "@/lib/queries"
import { href, useRoute } from "@/lib/route"
import type { ContainerItem, ImageItem } from "@/lib/types"

const grid =
  "grid grid-cols-1 gap-4 px-4 lg:px-6 @xl/main:grid-cols-2 @5xl/main:grid-cols-3 @[100rem]/main:grid-cols-4"

/** The header's action on the network groups page. */
export function NewGroupButton() {
  return (
    <Button size="sm" onClick={() => openDialog({ kind: "group-connect" })}>
      <RiAddLine data-icon="inline-start" />
      <span className="hidden sm:inline">{t("groups.new")}</span>
      <span className="sr-only sm:hidden">{t("groups.new")}</span>
    </Button>
  )
}

/** A group as a card: its network, how many members run, and each member with its aliases. */
function GroupCard({ group }: { group: KnownGroup }) {
  const link = href({ page: "groups", group: group.name })
  const running = group.members.filter((item) => item.state === "running").length
  return (
    <Card className="gap-4 transition-colors hover:ring-foreground/20">
      <CardHeader>
        <CardTitle className="flex min-w-0 items-center gap-2 text-base">
          <ConceptIcon concept="container" icon={RiGroupLine} className="size-4 shrink-0" />
          <a
            href={link}
            className="truncate font-mono hover:underline focus-visible:underline focus-visible:outline-none"
          >
            {group.name}
          </a>
        </CardTitle>
        <CardDescription className="truncate font-mono text-xs">
          {groupNetwork(group.name)}
        </CardDescription>
        <CardAction>
          <Badge variant="outline" className="text-muted-foreground">
            {t("groups.running", {
              running: formatNumber(running),
              total: formatNumber(group.members.length),
            })}
          </Badge>
        </CardAction>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col gap-2 text-sm">
          {group.members.map((item) => (
            <li key={item.id} className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
              <ConceptIcon concept="container" className="size-4 shrink-0" />
              <span className="min-w-0 truncate">{item.spec.name}</span>
              <AliasBadges aliases={item.spec.aliases} />
            </li>
          ))}
        </ul>
      </CardContent>
      <CardFooter className="mt-auto gap-2">
        <Button asChild variant="outline" className="flex-1">
          <a href={link}>{t("groups.open")}</a>
        </Button>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              size="icon"
              aria-label={t("groups.connectTo", { name: group.name })}
              onClick={() => openDialog({ kind: "group-connect", group: group.name })}
            >
              <RiAddLine />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t("groups.connect")}</TooltipContent>
        </Tooltip>
      </CardFooter>
    </Card>
  )
}

/** Takes a container out of a group, which a running one leaves at once. */
function DisconnectButton({ item, group }: { item: ContainerItem; group: string }) {
  const [busy, setBusy] = React.useState(false)
  const label = t("groups.disconnect", { name: group })
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="outline"
          size="icon"
          disabled={busy}
          aria-label={`${label}: ${item.spec.name}`}
          onClick={async () => {
            setBusy(true)
            await attempt(() =>
              runJob(`containers/${item.id}/settings`, {
                groups: (item.spec.groups ?? []).filter((name) => name !== group),
              })
            )
            setBusy(false)
          }}
        >
          <RiLinkUnlinkM />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

/** One group's members as container cards, with their aliases beside their names. */
function GroupMembers({
  name,
  members,
  images,
}: {
  name: string
  members: ContainerItem[]
  images: ImageItem[]
}) {
  return (
    <>
      <SectionHeading
        title={name}
        description={rich("groups.detailHint", {
          network: <code className="font-mono">{groupNetwork(name)}</code>,
        })}
      >
        <Button asChild variant="outline" size="sm">
          <a href={href({ page: "groups" })}>
            <RiArrowLeftLine data-icon="inline-start" />
            {t("groups.all")}
          </a>
        </Button>
        <Button size="sm" onClick={() => openDialog({ kind: "group-connect", group: name })}>
          <RiAddLine data-icon="inline-start" />
          {t("groups.connect")}
        </Button>
      </SectionHeading>
      {members.length ? (
        <div className={grid}>
          {[...members]
            .sort((a, b) => a.spec.name.localeCompare(b.spec.name))
            .map((item) => (
              <ContainerCard
                key={item.id}
                item={item}
                images={images}
                aliases
                actions={<DisconnectButton item={item} group={name} />}
              />
            ))}
        </div>
      ) : (
        <Empty className="mx-4 w-auto border lg:mx-6">
          <EmptyHeader>
            <ConceptMedia concept="container" icon={RiGroupLine} />
            <EmptyTitle>{t("groups.empty")}</EmptyTitle>
            <EmptyDescription>{t("groups.emptyHint")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </>
  )
}

/**
 * The network groups the containers list, each as a card, or the members of the group the link
 * names. A group is only a name in its members' settings, so it shows while one lists it.
 */
export function GroupsPage() {
  const route = useRoute()
  const containers = useContainers()
  const images = useImages().data ?? []
  const groups = knownGroups(containers.data ?? [])
  const open = route.page === "groups" ? route.group : undefined

  return (
    <PageBody>
      <DockerAlert />
      {containers.isPending ? (
        <div className={grid}>
          {[0, 1, 2].map((index) => (
            <Card key={index} className="gap-4 p-4">
              <Skeleton className="h-5 w-1/2" />
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-4 w-2/3" />
            </Card>
          ))}
        </div>
      ) : listFailed(containers) ? (
        <ListFailed query={containers} />
      ) : open !== undefined ? (
        <GroupMembers
          name={open}
          members={groups.find((group) => group.name === open)?.members ?? []}
          images={images}
        />
      ) : groups.length ? (
        <div className={grid}>
          {groups.map((group) => (
            <GroupCard key={group.name} group={group} />
          ))}
        </div>
      ) : (
        <Empty className="mx-4 w-auto border lg:mx-6">
          <EmptyHeader>
            <ConceptMedia concept="container" icon={RiGroupLine} />
            <EmptyTitle>{t("groups.none")}</EmptyTitle>
            <EmptyDescription>{t("groups.noneHint")}</EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => openDialog({ kind: "group-connect" })}>
              <RiAddLine data-icon="inline-start" />
              {t("groups.create")}
            </Button>
          </EmptyContent>
        </Empty>
      )}
    </PageBody>
  )
}
