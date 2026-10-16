import {
  RiStopCircleLine,
  RiInformationLine,
  RiComputerLine,
  RiMoreLine,
  RiUploadLine,
} from "@remixicon/react"

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
      <SidebarGroupLabel>Running containers</SidebarGroupLabel>
      <SidebarMenu>
        {running.length === 0 && (
          <SidebarMenuItem>
            <span className="px-2 text-xs text-sidebar-foreground/60">
              No container is running.
            </span>
          </SidebarMenuItem>
        )}
        {running.slice(0, SHOWN).map((container) => (
          <SidebarMenuItem key={container.id}>
            <SidebarMenuButton asChild tooltip={`Connect to ${container.spec.name}`}>
              <a href={href({ page: "desktop", id: container.id })}>
                <RiComputerLine />
                <span>{container.spec.name}</span>
              </a>
            </SidebarMenuButton>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuAction showOnHover className="rounded-sm data-[state=open]:bg-accent">
                  <RiMoreLine />
                  <span className="sr-only">{container.spec.name} actions</span>
                </SidebarMenuAction>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                className="w-44 rounded-lg"
                side={isMobile ? "bottom" : "right"}
                align={isMobile ? "end" : "start"}
              >
                <DropdownMenuItem asChild>
                  <a href={href({ page: "desktop", id: container.id })}>
                    <RiComputerLine />
                    Connect
                  </a>
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => {
                    close()
                    openDialog({ kind: "upload", id: container.id })
                  }}
                >
                  <RiUploadLine />
                  Upload files
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => {
                    close()
                    openDialog({ kind: "details", id: container.id })
                  }}
                >
                  <RiInformationLine />
                  Details
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  variant="destructive"
                  onSelect={() => attempt(() => containerAction(container.id, "stop"))}
                >
                  <RiStopCircleLine />
                  Stop
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
                <span>{running.length - SHOWN} more</span>
              </a>
            </SidebarMenuButton>
          </SidebarMenuItem>
        )}
      </SidebarMenu>
    </SidebarGroup>
  )
}
