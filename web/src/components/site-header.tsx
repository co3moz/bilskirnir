import { Separator } from "@/components/ui/separator"
import { SidebarTrigger } from "@/components/ui/sidebar"
import { t } from "@/lib/i18n"
import { useSystem } from "@/lib/queries"

function DockerStatus() {
  const system = useSystem().data
  if (!system) return null
  return (
    <span
      className="hidden items-center gap-2 text-xs text-muted-foreground sm:flex"
      title={system.connected ? undefined : system.error}
    >
      <span
        aria-hidden
        className={`size-2 rounded-full ${system.connected ? "bg-success" : "bg-destructive"}`}
      />
      {system.connected ? `Docker ${system.version ?? ""}`.trim() : t("state.unavailable")}
    </span>
  )
}

export function SiteHeader({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <header className="flex h-(--header-height) shrink-0 items-center gap-2 border-b transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-(--header-height)">
      <div className="flex w-full items-center gap-1 px-4 lg:gap-2 lg:px-6">
        <SidebarTrigger className="-ml-1" />
        <Separator
          orientation="vertical"
          className="mx-2 data-vertical:h-4 data-vertical:self-center"
        />
        <h1 className="text-base font-medium">{title}</h1>
        <div className="ml-auto flex items-center gap-3">
          <DockerStatus />
          {children}
        </div>
      </div>
    </header>
  )
}
