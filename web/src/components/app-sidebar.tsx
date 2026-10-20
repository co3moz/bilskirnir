import * as React from "react"
import {
  RiApps2Line,
  RiBookOpenLine,
  RiDatabase2Line,
  RiFolderOpenLine,
  RiStackLine,
  RiDashboardLine,
  RiListCheck3,
  RiComputerLine,
  RiPlugLine,
  RiBookletLine,
} from "@remixicon/react"

import { NavContainers } from "@/components/nav-containers"
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
import { t } from "@/lib/i18n"
import { href, type Page } from "@/lib/route"

export const pageTitle = (page: Page) => t(`nav.${page}`)

const navMain = [
  { page: "dashboard", icon: <RiDashboardLine /> },
  { page: "containers", icon: <RiComputerLine /> },
  { page: "images", icon: <RiStackLine /> },
  { page: "templates", icon: <RiApps2Line /> },
  { page: "volumes", icon: <RiDatabase2Line /> },
  { page: "mcp", icon: <RiPlugLine /> },
  { page: "files", icon: <RiFolderOpenLine /> },
  { page: "jobs", icon: <RiListCheck3 /> },
] satisfies { page: Page; icon: React.ReactNode }[]

const repository = "https://github.com/co3moz/bilskirnir"
const navSecondary = () => [
  { title: t("nav.documentation"), url: `${repository}#readme`, icon: <RiBookOpenLine /> },
  {
    title: t("nav.mcpGuide"),
    url: `${repository}/blob/master/docs/MCP.md`,
    icon: <RiBookletLine />,
  },
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
          items={navMain.map((item) => ({ ...item, title: pageTitle(item.page) }))}
        />
        <NavContainers />
        <NavSecondary items={navSecondary()} className="mt-auto" />
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
    </Sidebar>
  )
}
