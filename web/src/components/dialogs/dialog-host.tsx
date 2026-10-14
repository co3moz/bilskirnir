import { Fragment } from "react"

import { closeDialog, useDialogState, type Dialog } from "@/lib/dialogs"
import { EnvironmentDrawer } from "./environment-drawer"
import { CreateEnvironmentDialog, SettingsDialog } from "./environment-dialogs"
import { JobDrawer } from "./job-drawer"
import { LogDialog } from "./log-dialog"
import {
  BuildImageDialog,
  DeleteDialog,
  ImportImageDialog,
  SnapshotDialog,
  VolumeDialog,
} from "./simple-dialogs"
import { UploadDialog } from "./upload-dialog"

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
      return <CreateEnvironmentDialog {...props} image={dialog.image} />
    case "details":
      return <EnvironmentDrawer {...props} id={dialog.id} />
    case "settings":
      return <SettingsDialog {...props} id={dialog.id} />
    case "snapshot":
      return <SnapshotDialog {...props} id={dialog.id} />
    case "delete":
      return <DeleteDialog {...props} id={dialog.id} />
    case "upload":
      return <UploadDialog {...props} id={dialog.id} files={dialog.files} />
    case "build":
      return <BuildImageDialog {...props} />
    case "import":
      return <ImportImageDialog {...props} />
    case "volume":
      return <VolumeDialog {...props} />
    case "job":
      return <JobDrawer {...props} id={dialog.id} />
    case "log":
      return <LogDialog {...props} title={dialog.title} text={dialog.text} status={dialog.status} />
  }
}
