import * as React from "react"
import {
  RiBookOpenLine,
  RiFolderOpenLine,
  RiDashboardLine,
  RiGroupLine,
  RiLinkM,
  RiListCheck3,
  RiPulseLine,
  RiBookletLine,
  RiSearchEyeLine,
  RiServerLine,
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

type NavPage = { page: Page; icon: React.ReactNode; badge?: React.ReactNode }
/**
 * A sidebar entry, with the pages listed below it while one of them is open. A section is only
 * a name for its pages, and its link opens the first of them.
 */
type NavEntry = NavPage & { section?: "mcp"; children?: NavPage[] }

const navMain: NavEntry[] = [
  { page: "dashboard", icon: <RiDashboardLine /> },
  {
    page: "containers",
    icon: <ConceptIcon concept="container" />,
    children: [{ page: "groups", icon: <ConceptIcon concept="container" icon={RiGroupLine} /> }],
  },
  { page: "images", icon: <ConceptIcon concept="image" /> },
  { page: "templates", icon: <ConceptIcon concept="template" /> },
  { page: "volumes", icon: <ConceptIcon concept="volume" /> },
  {
    section: "mcp",
    page: "mcp-endpoints",
    icon: <ConceptIcon concept="mcp" />,
    children: [
      { page: "mcp-endpoints", icon: <ConceptIcon concept="mcp" icon={RiLinkM} /> },
      { page: "mcp-servers", icon: <ConceptIcon concept="mcp" icon={RiServerLine} /> },
      { page: "mcp-inspector", icon: <RiSearchEyeLine /> },
      { page: "activity", icon: <RiPulseLine /> },
    ],
  },
  { page: "files", icon: <RiFolderOpenLine /> },
  {
    page: "settings",
    icon: <RiSettings3Line />,
    children: [{ page: "jobs", icon: <RiListCheck3 />, badge: <ActiveJobsBadge /> }],
  },
]
const titled = (item: NavPage) => ({ ...item, title: pageTitle(item.page) })
const entryTitle = (entry: NavEntry) =>
  entry.section ? t(`nav.${entry.section}`) : pageTitle(entry.page)

/** Pages the sidebar does not list, by the page they are reached from. */
const reachedFrom: Partial<Record<Page, Page>> = { dependencies: "settings" }

/** The sidebar entry a page is listed under, or that of the page it is reached from. */
function entryOf(page: Page) {
  const listed = reachedFrom[page] ?? page
  return navMain.find(
    (entry) => entry.page === listed || entry.children?.some((child) => child.page === listed)
  )
}

/** The sidebar entry a page belongs to, which the sidebar keeps open while the page shows. */
export function pageSection(page: Page): string {
  const entry = entryOf(page)
  return entry ? (entry.section ?? entry.page) : page
}

/** The entry the header names before a page, when the page is not that entry's own. */
export function pageParent(page: Page) {
  const entry = entryOf(page)
  if (!entry || (entry.section ?? entry.page) === page) return undefined
  return { title: entryTitle(entry), href: href({ page: entry.page }) }
}

/** A page's icon, in its concept's color on the pages of one kind of thing. */
export const pageIcon = (page: Page) => {
  // A section's first page has an icon of its own beside the section's.
  const pages = navMain.flatMap((item) => [...(item.children ?? []), item])
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
          section={pageSection(page)}
          items={navMain.map((item) => ({
            ...item,
            section: item.section ?? item.page,
            title: entryTitle(item),
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
