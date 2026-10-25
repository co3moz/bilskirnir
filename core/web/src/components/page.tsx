import { RiAlertLine } from "@remixicon/react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { t } from "@/lib/i18n"
import { useSystem } from "@/lib/queries"

/** The content column of dashboard-01, sized by its `@container/main` queries. */
export function PageBody({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 flex-col">
      <div className="@container/main flex flex-1 flex-col gap-2">
        <div className="flex flex-col gap-4 py-4 md:gap-6 md:py-6">{children}</div>
      </div>
    </div>
  )
}

/** A section heading with its actions, inside the page's side padding. */
export function SectionHeading({
  title,
  description,
  children,
}: {
  title: string
  description?: string
  children?: React.ReactNode
}) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-2 px-4 lg:px-6">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-base font-medium">{title}</h2>
        {description && <p className="text-sm text-muted-foreground">{description}</p>}
      </div>
      {children && <div className="flex items-center gap-2">{children}</div>}
    </div>
  )
}

export function DockerAlert() {
  const system = useSystem().data
  if (!system || system.connected) return null
  return (
    <div className="px-4 lg:px-6">
      <Alert variant="destructive">
        <RiAlertLine />
        <AlertTitle>{t("docker.unreachable")}</AlertTitle>
        <AlertDescription>{system.error || t("docker.checkDaemon")}</AlertDescription>
      </Alert>
    </div>
  )
}
