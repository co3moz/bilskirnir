import * as React from "react"
import { PlusIcon, TriangleAlertIcon } from "lucide-react"
import { toast } from "sonner"

import { AppSidebar, pageTitles } from "@/components/app-sidebar"
import { DialogHost } from "@/components/dialogs/dialog-host"
import { SiteHeader } from "@/components/site-header"
import { Button } from "@/components/ui/button"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar"
import { Spinner } from "@/components/ui/spinner"
import { closeDialog, openDialog } from "@/lib/dialogs"
import { useSession, useSettings } from "@/lib/queries"
import { href, useRoute, type Page } from "@/lib/route"
import { DashboardPage } from "@/pages/dashboard"
import { EnvironmentsPage } from "@/pages/environments"
import { ImagesPage } from "@/pages/images"
import { JobsPage } from "@/pages/jobs"
import { LoginPage } from "@/pages/login"
import { VolumesPage } from "@/pages/volumes"

// noVNC is most of the bundle and only the desktop view needs it.
const DesktopPage = React.lazy(() =>
  import("@/pages/desktop").then((module) => ({ default: module.DesktopPage }))
)

const pages: Record<Page, () => React.ReactNode> = {
  dashboard: DashboardPage,
  environments: EnvironmentsPage,
  images: ImagesPage,
  volumes: VolumesPage,
  jobs: JobsPage,
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
          <TriangleAlertIcon />
        </EmptyMedia>
        <EmptyTitle>Connection failed</EmptyTitle>
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
        <SiteHeader title={pageTitles[page]}>
          {page === "environments" && (
            <Button size="sm" onClick={() => openDialog({ kind: "create" })}>
              <PlusIcon data-icon="inline-start" />
              <span className="hidden sm:inline">New environment</span>
              <span className="sr-only sm:hidden">New environment</span>
            </Button>
          )}
        </SiteHeader>
        <Content />
      </SidebarInset>
    </SidebarProvider>
  )
}

function Authenticated() {
  const route = useRoute()
  const settings = useSettings()
  const key = href(route)

  // A dialog belongs to the page it was opened on.
  React.useEffect(() => closeDialog(), [key])
  React.useEffect(() => {
    document.title =
      route.page === "desktop" ? "Desktop · Bilskirnir" : `${pageTitles[route.page]} · Bilskirnir`
  }, [route.page])

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
      toast.error("Drop files on an environment card or its desktop.")
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
