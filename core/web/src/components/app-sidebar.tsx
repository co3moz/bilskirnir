import * as React from "react"
import {
  RiApps2Line,
  RiBookOpenLine,
  RiDatabase2Line,
  RiFolderOpenLine,
  RiStackLine,
  RiDashboardLine,
  RiComputerLine,
  RiListCheck3,
  RiPlugLine,
  RiPulseLine,
  RiBookletLine,
  RiSettings3Line,
} from "@remixicon/react"

import { ActiveJobsBadge } from "@/components/jobs-menu"
import { NavContainers } from "@/components/nav-containers"
import { NavMain } from "@/components/nav-main"
import { NavSecondary } from "@/components/nav-secondary"
import { NavUser } from "@/components/nav-user"
import { UpdateNotice } from "@/components/update-notice"
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
import { useUpdate } from "@/lib/queries"
import { href, type Page } from "@/lib/route"

export const pageTitle = (page: Page) => t(`nav.${page}`)

/** Pages that belong to Management, which the sidebar keeps open while one of them is. */
const underSettings: Page[] = ["jobs", "dependencies"]
export const parentPage = (page: Page): Page | undefined =>
  underSettings.includes(page) ? "settings" : undefined

type NavPage = { page: Page; icon: React.ReactNode; badge?: React.ReactNode }

const navMain: (NavPage & { children?: NavPage[] })[] = [
  { page: "dashboard", icon: <RiDashboardLine /> },
  { page: "containers", icon: <RiComputerLine /> },
  { page: "images", icon: <RiStackLine /> },
  { page: "templates", icon: <RiApps2Line /> },
  { page: "volumes", icon: <RiDatabase2Line /> },
  { page: "mcp", icon: <RiPlugLine /> },
  { page: "files", icon: <RiFolderOpenLine /> },
  { page: "activity", icon: <RiPulseLine /> },
  {
    page: "settings",
    icon: <RiSettings3Line />,
    children: [{ page: "jobs", icon: <RiListCheck3 />, badge: <ActiveJobsBadge /> }],
  },
]
const titled = (item: NavPage) => ({ ...item, title: pageTitle(item.page) })

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
  // The release this manager runs, also while release checks are turned off.
  const version = useUpdate().data?.current
  return (
    <Sidebar collapsible="offcanvas" {...props}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton asChild className="data-[slot=sidebar-menu-button]:p-1.5!">
              <a href={href({ page: "dashboard" })}>
                <img src="/mark.svg" alt="" className="size-6!" />
                <span className="text-base font-semibold tracking-wide">Bilskirnir</span>
                {version && (
                  <span className="ml-auto font-mono text-xs text-muted-foreground">
                    {t("nav.version", { version })}
                  </span>
                )}
              </a>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <NavMain
          active={page}
          section={parentPage(page) ?? page}
          items={navMain.map((item) => ({
            ...titled(item),
            children: item.children?.map(titled),
          }))}
        />
        <NavContainers />
        <NavSecondary items={navSecondary()} className="mt-auto" />
      </SidebarContent>
      <SidebarFooter>
        <UpdateNotice />
        <NavUser />
      </SidebarFooter>
    </Sidebar>
  )
}
