import {
  RiMore2Line,
  RiLogoutBoxRLine,
  RiComputerLine,
  RiMoonLine,
  RiShieldCheckLine,
  RiShieldCrossLine,
  RiSunLine,
} from "@remixicon/react"

import { useTheme, type Theme } from "@/components/theme-provider"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar"
import { attempt } from "@/lib/actions"
import { api, sessionExpired } from "@/lib/api"
import { useSession, useSystem } from "@/lib/queries"

function Identity({ auth, detail }: { auth: boolean; detail: string }) {
  return (
    <>
      <Avatar className="h-8 w-8 rounded-lg">
        <AvatarFallback className="rounded-lg bg-sidebar-accent text-sidebar-accent-foreground">
          {auth ? (
            <RiShieldCheckLine className="size-4" />
          ) : (
            <RiShieldCrossLine className="size-4" />
          )}
        </AvatarFallback>
      </Avatar>
      <div className="grid flex-1 text-left text-sm leading-tight">
        <span className="truncate font-medium">{auth ? "Administrator" : "Local access"}</span>
        <span className="truncate text-xs text-muted-foreground">{detail}</span>
      </div>
    </>
  )
}

export function NavUser() {
  const { isMobile } = useSidebar()
  const { theme, setTheme } = useTheme()
  const auth = useSession().data?.auth_enabled ?? false
  const system = useSystem().data
  const docker = system?.connected
    ? `Docker ${system.version ?? ""}`.trim()
    : system
      ? "Docker unavailable"
      : "Checking Docker…"

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton
              size="lg"
              className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
            >
              <Identity auth={auth} detail={docker} />
              <RiMore2Line className="ml-auto size-4" />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            className="w-(--radix-dropdown-menu-trigger-width) min-w-56 rounded-lg"
            side={isMobile ? "bottom" : "right"}
            align="end"
            sideOffset={4}
          >
            <DropdownMenuLabel className="p-0 font-normal">
              <div className="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
                <Identity auth={auth} detail={auth ? "Signed in" : "Sign-in is turned off"} />
              </div>
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="text-xs text-muted-foreground">Theme</DropdownMenuLabel>
            <DropdownMenuRadioGroup
              value={theme}
              onValueChange={(value) => setTheme(value as Theme)}
            >
              <DropdownMenuRadioItem value="dark">
                <RiMoonLine />
                Dark
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="light">
                <RiSunLine />
                Light
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="system">
                <RiComputerLine />
                System
              </DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
            {auth && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuGroup>
                  <DropdownMenuItem
                    onSelect={() =>
                      attempt(async () => {
                        await api("logout", {})
                        sessionExpired()
                      })
                    }
                  >
                    <RiLogoutBoxRLine />
                    Sign out
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}
