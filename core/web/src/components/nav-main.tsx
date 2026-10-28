import { RiAddCircleLine, RiHammerLine } from "@remixicon/react"

import { ConceptIcon } from "@/components/concept"
import { Button } from "@/components/ui/button"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  useSidebar,
} from "@/components/ui/sidebar"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { openDialog } from "@/lib/dialogs"
import { t } from "@/lib/i18n"
import { href, type Page } from "@/lib/route"

type NavItem = { page: Page; title: string; icon?: React.ReactNode; badge?: React.ReactNode }

/**
 * The pages, with the open one marked. An item's own pages show below it while one of them, or a
 * page that belongs to the item, is open; `section` names the item the open page belongs to.
 */
export function NavMain({
  items,
  active,
  section,
}: {
  items: (NavItem & { section: string; children?: NavItem[] })[]
  active: Page
  section: string
}) {
  const { isMobile, setOpenMobile } = useSidebar()
  // The mobile sidebar is a sheet over the page; following a link should reveal the page.
  const close = () => isMobile && setOpenMobile(false)

  return (
    <SidebarGroup>
      <SidebarGroupContent className="flex flex-col gap-2">
        <SidebarMenu>
          <SidebarMenuItem className="flex items-center gap-2">
            <SidebarMenuButton
              tooltip={t("containers.new")}
              onClick={() => {
                close()
                openDialog({ kind: "create" })
              }}
              className="min-w-8 bg-primary text-primary-foreground duration-200 ease-linear hover:bg-primary/90 hover:text-primary-foreground active:bg-primary/90 active:text-primary-foreground"
            >
              <RiAddCircleLine />
              <span>{t("containers.new")}</span>
            </SidebarMenuButton>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  size="icon"
                  className="size-8 group-data-[collapsible=icon]:opacity-0"
                  variant="outline"
                  onClick={() => {
                    close()
                    openDialog({ kind: "build" })
                  }}
                >
                  <ConceptIcon concept="image" icon={RiHammerLine} />
                  <span className="sr-only">{t("images.build")}</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="right">{t("images.build")}</TooltipContent>
            </Tooltip>
          </SidebarMenuItem>
        </SidebarMenu>
        <SidebarMenu>
          {items.map((item) => {
            const open = item.section === section
            const onChild = item.children?.some((child) => child.page === active)
            return (
              <SidebarMenuItem key={item.section}>
                <SidebarMenuButton tooltip={item.title} isActive={open && !onChild} asChild>
                  <a href={href({ page: item.page })} onClick={close}>
                    {item.icon}
                    <span>{item.title}</span>
                  </a>
                </SidebarMenuButton>
                {open && item.children && (
                  <SidebarMenuSub>
                    {item.children.map((child) => (
                      <SidebarMenuSubItem key={child.page}>
                        <SidebarMenuSubButton isActive={child.page === active} asChild>
                          <a href={href({ page: child.page })} onClick={close}>
                            {child.icon}
                            <span>{child.title}</span>
                            {child.badge}
                          </a>
                        </SidebarMenuSubButton>
                      </SidebarMenuSubItem>
                    ))}
                  </SidebarMenuSub>
                )}
              </SidebarMenuItem>
            )
          })}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}
