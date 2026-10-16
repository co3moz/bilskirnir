import { StatusBadge } from "@/components/status-badge"
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { jobTone } from "@/lib/format"
import { t } from "@/lib/i18n"
import type { DialogProps } from "./side-drawer"

/** Container logs and diagnostics, shown as the manager returned them. */
export function LogDialog({
  title,
  text,
  status,
  ...props
}: DialogProps & { title: string; text: string; status?: string }) {
  return (
    <Dialog {...props}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <pre className="min-h-32 flex-1 overflow-auto rounded-md border bg-muted/40 p-3 font-mono text-xs leading-relaxed break-words whitespace-pre-wrap">
          {text || t("logs.empty")}
        </pre>
        {status && <StatusBadge state={status} tone={jobTone(status)} />}
      </DialogContent>
    </Dialog>
  )
}
