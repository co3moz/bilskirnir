import * as React from "react"
import {
  BookOpenIcon,
  DatabaseIcon,
  LayersIcon,
  LayoutDashboardIcon,
  ListChecksIcon,
  MonitorIcon,
  PlugIcon,
} from "lucide-react"

import { NavDesktops } from "@/components/nav-desktops"
import { NavMain } from "@/components/nav-main"
import { NavSecondary } from "@/components/nav-secondary"
import { NavUser } from "@/components/nav-user"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { href, type Page } from "@/lib/route"

export const pageTitles: Record<Page, string> = {
  dashboard: "Dashboard",
  environments: "Environments",
  images: "Images",
  volumes: "Volumes",
  jobs: "Jobs",
}

const navMain = [
  { page: "dashboard", icon: <LayoutDashboardIcon /> },
  { page: "environments", icon: <MonitorIcon /> },
  { page: "images", icon: <LayersIcon /> },
  { page: "volumes", icon: <DatabaseIcon /> },
  { page: "jobs", icon: <ListChecksIcon /> },
] satisfies { page: Page; icon: React.ReactNode }[]

const repository = "https://github.com/co3moz/bilskirnir"
const navSecondary = [
  { title: "Documentation", url: `${repository}#readme`, icon: <BookOpenIcon /> },
  { title: "Agent access (MCP)", url: `${repository}/blob/master/docs/MCP.md`, icon: <PlugIcon /> },
]

export function AppSidebar({
  page,
  ...props
}: React.ComponentProps<typeof Sidebar> & { page: Page }) {
  return (
    <Sidebar collapsible="offcanvas" {...props}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton asChild className="data-[slot=sidebar-menu-button]:p-1.5!">
              <a href={href({ page: "dashboard" })}>
                <img src="/mark.svg" alt="" className="size-6!" />
                <span className="text-base font-semibold tracking-wide">Bilskirnir</span>
              </a>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <NavMain
          active={page}
          items={navMain.map((item) => ({ ...item, title: pageTitles[item.page] }))}
        />
        <NavDesktops />
        <NavSecondary items={navSecondary} className="mt-auto" />
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
    </Sidebar>
  )
}
