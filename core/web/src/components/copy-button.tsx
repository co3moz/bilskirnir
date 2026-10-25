import * as React from "react"
import { RiCheckLine, RiFileCopyLine } from "@remixicon/react"
import { cn } from "cn"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { copyText } from "@/lib/actions"
import { t } from "@/lib/i18n"

/** Copies `text`, and shows a check for a moment once it did. */
export function CopyButton({
  text,
  label,
  className,
  size = "icon-sm",
}: {
  text: string
  /** What is copied, as the button's accessible name and tooltip. */
  label: string
  className?: string
  size?: "icon-xs" | "icon-sm"
}) {
  const [copied, setCopied] = React.useState(false)
  React.useEffect(() => {
    if (!copied) return
    const timer = setTimeout(() => setCopied(false), 1500)
    return () => clearTimeout(timer)
  }, [copied])
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size={size}
          className={cn("shrink-0", className)}
          aria-label={label}
          onClick={async () => {
            if (await copyText(text)) setCopied(true)
            else toast.error(t("common.copyFailed"))
          }}
        >
          {copied ? <RiCheckLine className="text-success" /> : <RiFileCopyLine />}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{copied ? t("common.copied") : label}</TooltipContent>
    </Tooltip>
  )
}
