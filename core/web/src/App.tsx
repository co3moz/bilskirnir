import * as React from "react"
import { RiAddLine, RiAlertLine } from "@remixicon/react"
import { toast } from "sonner"

import { AppSidebar, pageIcon, pageTitle } from "@/components/app-sidebar"
import { DialogHost } from "@/components/dialogs/dialog-host"
import { HumanRequestsBanner } from "@/components/human-requests"
import { SiteHeader } from "@/components/site-header"
import { Button } from "@/components/ui/button"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar"
import { Spinner } from "@/components/ui/spinner"
import { closeDialog, openDialog } from "@/lib/dialogs"
import { t, useLocale } from "@/lib/i18n"
import { useSession, useSettings } from "@/lib/queries"
import { href, useRoute, type Page } from "@/lib/route"
import { useServerEvents } from "@/lib/server-events"
import { ActivityPage } from "@/pages/activity"
import { DashboardPage } from "@/pages/dashboard"
import { DependenciesPage } from "@/pages/dependencies"
import { FilesPage } from "@/pages/files"
import { ContainersPage } from "@/pages/containers"
import { ImagesPage } from "@/pages/images"
import { JobsPage } from "@/pages/jobs"
import { LoginPage } from "@/pages/login"
import { McpPage } from "@/pages/mcp"
import { SettingsPage } from "@/pages/settings"
import { TemplatesPage } from "@/pages/templates"
import { VolumesPage } from "@/pages/volumes"

// noVNC is most of the bundle and only the desktop view needs it.
const DesktopPage = React.lazy(() =>
  import("@/pages/desktop").then((module) => ({ default: module.DesktopPage }))
)

const pages: Record<Page, () => React.ReactNode> = {
  dashboard: DashboardPage,
  containers: ContainersPage,
  images: ImagesPage,
  templates: TemplatesPage,
  volumes: VolumesPage,
  mcp: McpPage,
  files: FilesPage,
  jobs: JobsPage,
  activity: ActivityPage,
  dependencies: DependenciesPage,
  settings: SettingsPage,
}

function Loading() {
  return (
    <div className="flex min-h-svh items-center justify-center bg-sidebar">
      <Spinner className="size-6 text-muted-foreground" />
    </div>
  )
}

function ConnectionFailed({ message }: { message: string }) {
  return (
    <Empty className="min-h-svh bg-sidebar">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <RiAlertLine />
        </EmptyMedia>
        <EmptyTitle>{t("app.connectionFailed")}</EmptyTitle>
        <EmptyDescription>{message}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}

function readSidebarCookie() {
  return !document.cookie.split("; ").includes("sidebar_state=false")
}

function Shell({ page }: { page: Page }) {
  const Content = pages[page]
  return (
    <SidebarProvider
      defaultOpen={readSidebarCookie()}
      style={
        {
          "--sidebar-width": "calc(var(--spacing) * 72)",
          "--header-height": "calc(var(--spacing) * 12)",
        } as React.CSSProperties
      }
    >
      <AppSidebar variant="inset" page={page} />
      <SidebarInset>
        <SiteHeader title={pageTitle(page)} icon={pageIcon(page)}>
          {page === "containers" && (
            <Button size="sm" onClick={() => openDialog({ kind: "create" })}>
              <RiAddLine data-icon="inline-start" />
              <span className="hidden sm:inline">{t("containers.new")}</span>
              <span className="sr-only sm:hidden">{t("containers.new")}</span>
            </Button>
          )}
        </SiteHeader>
        <HumanRequestsBanner />
        <Content />
      </SidebarInset>
    </SidebarProvider>
  )
}

function Authenticated() {
  const route = useRoute()
  const settings = useSettings()
  const locale = useLocale()
  const key = href(route)
  useServerEvents()

  // A dialog belongs to the page it was opened on.
  React.useEffect(() => closeDialog(), [key])
  React.useEffect(() => {
    document.title = `${route.page === "desktop" ? t("nav.desktop") : pageTitle(route.page)} · Bilskirnir`
  }, [route.page, locale])

  if (settings.isPending) return <Loading />
  if (settings.error) return <ConnectionFailed message={settings.error.message} />
  return (
    <>
      {route.page === "desktop" ? (
        <React.Suspense fallback={<Loading />}>
          <DesktopPage id={route.id} />
        </React.Suspense>
      ) : (
        <Shell page={route.page} />
      )}
      <DialogHost />
    </>
  )
}

/** Files dropped anywhere else would replace the page with the file; explain where they go. */
function useDropGuard() {
  React.useEffect(() => {
    const files = (event: DragEvent) => event.dataTransfer?.types.includes("Files")
    const over = (event: DragEvent) => {
      if (!files(event)) return
      event.preventDefault()
      event.dataTransfer!.dropEffect = "none"
    }
    const drop = (event: DragEvent) => {
      if (!files(event)) return
      event.preventDefault()
      toast.error(t("uploads.dropHere"))
    }
    document.addEventListener("dragover", over)
    document.addEventListener("drop", drop)
    return () => {
      document.removeEventListener("dragover", over)
      document.removeEventListener("drop", drop)
    }
  }, [])
}

export function App() {
  const session = useSession()
  useDropGuard()
  if (session.isPending) return <Loading />
  if (session.error) return <ConnectionFailed message={session.error.message} />
  return session.data.authenticated ? <Authenticated /> : <LoginPage />
}
