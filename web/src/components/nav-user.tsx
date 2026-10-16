import {
  RiMore2Line,
  RiLogoutBoxRLine,
  RiComputerLine,
  RiMoonLine,
  RiShieldCheckLine,
  RiShieldCrossLine,
  RiSunLine,
} from "@remixicon/react"

import { LanguageMenu } from "@/components/language-menu"
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
import { t } from "@/lib/i18n"
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
        <span className="truncate font-medium">
          {auth ? t("account.administrator") : t("account.local")}
        </span>
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
      ? t("state.unavailable")
      : t("docker.checking")

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
                <Identity
                  auth={auth}
                  detail={auth ? t("account.signedIn") : t("account.signInOff")}
                />
              </div>
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="text-xs text-muted-foreground">
              {t("account.theme")}
            </DropdownMenuLabel>
            <DropdownMenuRadioGroup
              value={theme}
              onValueChange={(value) => setTheme(value as Theme)}
            >
              <DropdownMenuRadioItem value="dark">
                <RiMoonLine />
                {t("account.dark")}
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="light">
                <RiSunLine />
                {t("account.light")}
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="system">
                <RiComputerLine />
                {t("account.system")}
              </DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
            <DropdownMenuSeparator />
            <LanguageMenu />
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
                    {t("account.signOut")}
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
