import * as React from "react"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { downloadFile, downloadFolder } from "@/lib/actions"
import { HOME, recentFolders } from "@/lib/desktop-paths"
import { t } from "@/lib/i18n"
import { useContainers, useSettings } from "@/lib/queries"
import { FolderBrowser } from "./folder-browser"
import type { DialogProps } from "./side-drawer"

/**
 * Downloads from a running container as the desktop user: a file as it is, a folder as a
 * .tar.gz archive that the manager makes while it downloads and records as a job.
 */
export function DownloadDialog({ id, open, onOpenChange }: DialogProps & { id: string }) {
  const item = useContainers(false).data?.find((entry) => entry.id === id)
  const limit = useSettings().data?.upload_limit_bytes ?? 0
  const [recent] = React.useState(() => recentFolders(id))
  const base = `containers/${encodeURIComponent(id)}`

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("downloads.title")}</DialogTitle>
          <DialogDescription>{item?.spec.name}</DialogDescription>
        </DialogHeader>
        {item?.state === "running" ? (
          <FolderBrowser
            container={item}
            initial={HOME}
            recent={recent}
            onCancel={() => onOpenChange(false)}
            onDownload={(path, entry) =>
              entry?.type === "file"
                ? downloadFile(base, path, entry.size, limit)
                : downloadFolder(base, path)
            }
          />
        ) : (
          <p className="text-sm text-muted-foreground">{t("downloads.notRunning")}</p>
        )}
      </DialogContent>
    </Dialog>
  )
}
