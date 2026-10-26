import { ConceptIcon } from "@/components/concept"
import { FileExplorer } from "@/components/file-explorer"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { rich, t } from "@/lib/i18n"
import { volumeApi } from "@/lib/volume-files"
import type { DialogProps } from "./side-drawer"

/** Browses a volume's files and changes them as the desktop user. */
export function VolumeBrowser({ name, open, onOpenChange }: DialogProps & { name: string }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ConceptIcon concept="volume" className="size-4 shrink-0" />
            {rich("files.title", { volume: <code className="font-mono">{name}</code> })}
          </DialogTitle>
          <DialogDescription>{t("files.description")}</DialogDescription>
        </DialogHeader>
        <FileExplorer
          base={volumeApi(name)}
          queryKey={["volumes", name, "files"]}
          rootName={name}
          emptyRoot={t("files.volumeEmpty")}
          paneClassName="h-[min(26rem,50vh)]"
          active={open}
        />
      </DialogContent>
    </Dialog>
  )
}
