import * as React from "react"
import {
  RiBookOpenLine,
  RiFolderOpenLine,
  RiDashboardLine,
  RiListCheck3,
  RiPulseLine,
  RiBookletLine,
  RiSettings3Line,
} from "@remixicon/react"

import { ConceptIcon } from "@/components/concept"
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

/** Pages that belong to another page, which the sidebar keeps open while one of them is. */
const parents: Partial<Record<Page, Page>> = {
  activity: "mcp",
  jobs: "settings",
  dependencies: "settings",
}
export const parentPage = (page: Page): Page | undefined => parents[page]

type NavPage = { page: Page; icon: React.ReactNode; badge?: React.ReactNode }

const navMain: (NavPage & { children?: NavPage[] })[] = [
  { page: "dashboard", icon: <RiDashboardLine /> },
  { page: "containers", icon: <ConceptIcon concept="container" /> },
  { page: "images", icon: <ConceptIcon concept="image" /> },
  { page: "templates", icon: <ConceptIcon concept="template" /> },
  { page: "volumes", icon: <ConceptIcon concept="volume" /> },
  {
    page: "mcp",
    icon: <ConceptIcon concept="mcp" />,
    children: [{ page: "activity", icon: <RiPulseLine /> }],
  },
  { page: "files", icon: <RiFolderOpenLine /> },
  {
    page: "settings",
    icon: <RiSettings3Line />,
    children: [{ page: "jobs", icon: <RiListCheck3 />, badge: <ActiveJobsBadge /> }],
  },
]
const titled = (item: NavPage) => ({ ...item, title: pageTitle(item.page) })

/** A page's icon, in its concept's color on the pages of one kind of thing. */
export const pageIcon = (page: Page) => {
  const pages = navMain.flatMap((item) => [item, ...(item.children ?? [])])
  return pages.find((item) => item.page === page)?.icon
}

const repository = "https://github.com/co3moz/thoric"
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
                <span className="text-base font-semibold tracking-wide">Thoric</span>
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
