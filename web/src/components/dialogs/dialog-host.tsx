import { Fragment } from "react"

import { closeDialog, useDialogState, type Dialog } from "@/lib/dialogs"
import { ContainerDrawer } from "./container-drawer"
import { CreateContainerDialog, SettingsDialog } from "./container-dialogs"
import { JobDrawer } from "./job-drawer"
import { LogDialog } from "./log-dialog"
import { ImportImageDialog } from "./import-dialog"
import {
  BuildImageDialog,
  DeleteDialog,
  ImageDeleteDialog,
  SnapshotDialog,
  VolumeCopyDialog,
  VolumeDeleteDialog,
  VolumeDialog,
} from "./simple-dialogs"
import { ComposeDialog } from "./compose-dialog"
import { IconDialog } from "./icon-dialog"
import { ImageDrawer } from "./image-drawer"
import { UploadDialog } from "./upload-dialog"
import { DownloadDialog } from "./download-dialog"
import { VolumeBrowser } from "./volume-browser"
import { VolumeImportDialog } from "./volume-import-dialog"

/** Renders the one open dialog; see lib/dialogs.ts. */
export function DialogHost() {
  const { dialog, open, seq } = useDialogState()
  if (!dialog) return null
  // A new `seq` remounts the dialog, so every opening starts with a fresh form.
  return <Fragment key={seq}>{render(dialog, open)}</Fragment>
}

function render(dialog: Dialog, open: boolean) {
  const props = { open, onOpenChange: (next: boolean) => !next && closeDialog() }
  switch (dialog.kind) {
    case "create":
      return <CreateContainerDialog {...props} image={dialog.image} />
    case "details":
      return <ContainerDrawer {...props} id={dialog.id} />
    case "settings":
      return <SettingsDialog {...props} id={dialog.id} />
    case "snapshot":
      return <SnapshotDialog {...props} id={dialog.id} />
    case "delete":
      return <DeleteDialog {...props} id={dialog.id} />
    case "upload":
      return <UploadDialog {...props} id={dialog.id} files={dialog.files} />
    case "download":
      return <DownloadDialog {...props} id={dialog.id} />
    case "build":
      return <BuildImageDialog {...props} config={dialog.config} />
    case "compose":
      return <ComposeDialog {...props} id={dialog.id} />
    case "icon":
      return <IconDialog {...props} id={dialog.id} />
    case "image":
      return <ImageDrawer {...props} id={dialog.id} />
    case "image-delete":
      return <ImageDeleteDialog {...props} id={dialog.id} />
    case "import":
      return <ImportImageDialog {...props} />
    case "volume":
      return <VolumeDialog {...props} />
    case "volume-files":
      return <VolumeBrowser {...props} name={dialog.name} />
    case "volume-import":
      return <VolumeImportDialog {...props} />
    case "volume-copy":
      return <VolumeCopyDialog {...props} name={dialog.name} />
    case "volume-delete":
      return <VolumeDeleteDialog {...props} name={dialog.name} />
    case "job":
      return <JobDrawer {...props} id={dialog.id} />
    case "log":
      return <LogDialog {...props} title={dialog.title} text={dialog.text} status={dialog.status} />
  }
}
