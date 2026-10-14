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
import { attempt, environmentAction } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { useEnvironments } from "@/lib/queries"
import { href } from "@/lib/route"

const SHOWN = 6

/** Quick access to running desktops, where dashboard-01 lists documents. */
export function NavDesktops() {
  const { isMobile, setOpenMobile } = useSidebar()
  const running = (useEnvironments().data ?? []).filter((item) => item.state === "running")
  const close = () => isMobile && setOpenMobile(false)

  return (
    <SidebarGroup className="group-data-[collapsible=icon]:hidden">
      <SidebarGroupLabel>Running desktops</SidebarGroupLabel>
      <SidebarMenu>
        {running.length === 0 && (
          <SidebarMenuItem>
            <span className="px-2 text-xs text-sidebar-foreground/60">No desktop is running.</span>
          </SidebarMenuItem>
        )}
        {running.slice(0, SHOWN).map(({ environment }) => (
          <SidebarMenuItem key={environment.id}>
            <SidebarMenuButton asChild tooltip={`Connect to ${environment.spec.name}`}>
              <a href={href({ page: "desktop", id: environment.id })}>
                <RiComputerLine />
                <span>{environment.spec.name}</span>
              </a>
            </SidebarMenuButton>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuAction showOnHover className="rounded-sm data-[state=open]:bg-accent">
                  <RiMoreLine />
                  <span className="sr-only">{environment.spec.name} actions</span>
                </SidebarMenuAction>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                className="w-44 rounded-lg"
                side={isMobile ? "bottom" : "right"}
                align={isMobile ? "end" : "start"}
              >
                <DropdownMenuItem asChild>
                  <a href={href({ page: "desktop", id: environment.id })}>
                    <RiComputerLine />
                    Connect
                  </a>
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => {
                    close()
                    openDialog({ kind: "upload", id: environment.id })
                  }}
                >
                  <RiUploadLine />
                  Upload files
                </DropdownMenuItem>
                <DropdownMenuItem
                  onSelect={() => {
                    close()
                    openDialog({ kind: "details", id: environment.id })
                  }}
                >
                  <RiInformationLine />
                  Details
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  variant="destructive"
                  onSelect={() => attempt(() => environmentAction(environment.id, "stop"))}
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
              <a href={href({ page: "environments" })} onClick={close}>
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
