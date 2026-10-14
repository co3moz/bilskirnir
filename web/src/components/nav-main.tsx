import { RiAddCircleLine, RiHammerLine } from "@remixicon/react"

import { Button } from "@/components/ui/button"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { openDialog } from "@/lib/dialogs"
import { jobActive } from "@/lib/format"
import { useJobs } from "@/lib/queries"
import { href, type Page } from "@/lib/route"

export function NavMain({
  items,
  active,
}: {
  items: { page: Page; title: string; icon?: React.ReactNode }[]
  active: Page
}) {
  const { isMobile, setOpenMobile } = useSidebar()
  const jobs = useJobs()
  const pending = jobs.data?.filter(jobActive).length ?? 0
  // The mobile sidebar is a sheet over the page; following a link should reveal the page.
  const close = () => isMobile && setOpenMobile(false)

  return (
    <SidebarGroup>
      <SidebarGroupContent className="flex flex-col gap-2">
        <SidebarMenu>
          <SidebarMenuItem className="flex items-center gap-2">
            <SidebarMenuButton
              tooltip="New environment"
              onClick={() => {
                close()
                openDialog({ kind: "create" })
              }}
              className="min-w-8 bg-primary text-primary-foreground duration-200 ease-linear hover:bg-primary/90 hover:text-primary-foreground active:bg-primary/90 active:text-primary-foreground"
            >
              <RiAddCircleLine />
              <span>New environment</span>
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
                  <RiHammerLine />
                  <span className="sr-only">Build image</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="right">Build image</TooltipContent>
            </Tooltip>
          </SidebarMenuItem>
        </SidebarMenu>
        <SidebarMenu>
          {items.map((item) => (
            <SidebarMenuItem key={item.page}>
              <SidebarMenuButton tooltip={item.title} isActive={item.page === active} asChild>
                <a href={href({ page: item.page })} onClick={close}>
                  {item.icon}
                  <span>{item.title}</span>
                </a>
              </SidebarMenuButton>
              {item.page === "jobs" && pending > 0 && (
                <SidebarMenuBadge aria-label={`${pending} jobs in progress`}>
                  {pending}
                </SidebarMenuBadge>
              )}
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}
