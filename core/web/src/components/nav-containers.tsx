import {
  RiStopCircleLine,
  RiInformationLine,
  RiComputerLine,
  RiMoreLine,
  RiDownloadLine,
  RiUploadLine,
} from "@remixicon/react"

import { ConceptIcon } from "@/components/concept"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar"
import { attempt, containerAction } from "@/lib/actions"
import { t } from "@/lib/i18n"
import { openDialog } from "@/lib/dialogs"
import { useContainers } from "@/lib/queries"
import { href } from "@/lib/route"

const SHOWN = 6

/** Quick access to running containers, where dashboard-01 lists documents. */
export function NavContainers() {
  const { isMobile, setOpenMobile } = useSidebar()
  const running = (useContainers().data ?? []).filter((item) => item.state === "running")
  const close = () => isMobile && setOpenMobile(false)

  return (
    <SidebarGroup className="group-data-[collapsible=icon]:hidden">
      <SidebarGroupLabel>{t("nav.running")}</SidebarGroupLabel>
      <SidebarMenu>
        {running.length === 0 && (
          <SidebarMenuItem>
            <span className="px-2 text-xs text-sidebar-foreground/60">{t("nav.noneRunning")}</span>
          </SidebarMenuItem>
        )}
        {running.slice(0, SHOWN).map((container) => (
          <SidebarMenuItem key={container.id}>
            <SidebarMenuButton
              asChild
              tooltip={t("containers.connectTo", { name: container.spec.name })}
            >
              <a href={href({ page: "desktop", id: container.id })}>
                <ConceptIcon concept="container" />
                <span>{container.spec.name}</span>
              </a>
            </SidebarMenuButton>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuAction showOnHover className="rounded-sm data-[state=open]:bg-accent">
                  <RiMoreLine />
                  <span className="sr-only">
                    {t("containers.actionsOf", { name: container.spec.name })}
                  </span>
                </SidebarMenuAction>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                className="w-44 rounded-lg"
                side={isMobile ? "bottom" : "right"}
                align={isMobile ? "end" : "start"}
              >
                <DropdownMenuItem tone="success" asChild>
                  <a href={href({ page: "desktop", id: container.id })}>
                    <RiComputerLine />
                    {t("containers.connect")}
                  </a>
                </DropdownMenuItem>
                <DropdownMenuItem
                  tone="transfer"
                  onSelect={() => {
                    close()
                    openDialog({ kind: "upload", id: container.id })
                  }}
                >
                  <RiUploadLine />
                  {t("uploads.title")}
                </DropdownMenuItem>
                <DropdownMenuItem
                  tone="transfer"
                  onSelect={() => {
                    close()
                    openDialog({ kind: "download", id: container.id })
                  }}
                >
                  <RiDownloadLine />
                  {t("downloads.title")}
                </DropdownMenuItem>
                <DropdownMenuItem
                  tone="info"
                  onSelect={() => {
                    close()
                    openDialog({ kind: "details", id: container.id })
                  }}
                >
                  <RiInformationLine />
                  {t("containers.details")}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  tone="warning"
                  onSelect={() => attempt(() => containerAction(container.id, "stop"))}
                >
                  <RiStopCircleLine />
                  {t("containers.stop")}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </SidebarMenuItem>
        ))}
        {running.length > SHOWN && (
          <SidebarMenuItem>
            <SidebarMenuButton asChild className="text-sidebar-foreground/70">
              <a href={href({ page: "containers" })} onClick={close}>
                <RiMoreLine className="text-sidebar-foreground/70" />
                <span>{t("nav.more", { count: running.length - SHOWN })}</span>
              </a>
            </SidebarMenuButton>
          </SidebarMenuItem>
        )}
      </SidebarMenu>
    </SidebarGroup>
  )
}
