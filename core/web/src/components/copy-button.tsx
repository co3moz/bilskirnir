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
  showLabel = false,
}: {
  text: string
  /** What is copied, as the button's accessible name and tooltip. */
  label: string
  className?: string
  size?: "icon-xs" | "icon-sm"
  /** Shows `label` on the button itself, such as in a footer beside other labelled buttons. */
  showLabel?: boolean
}) {
  const [copied, setCopied] = React.useState(false)
  React.useEffect(() => {
    if (!copied) return
    const timer = setTimeout(() => setCopied(false), 1500)
    return () => clearTimeout(timer)
  }, [copied])
  const Icon = copied ? RiCheckLine : RiFileCopyLine
  const button = (
    <Button
      type="button"
      variant="outline"
      size={showLabel ? "default" : size}
      className={cn("shrink-0", className)}
      aria-label={showLabel ? undefined : label}
      onClick={async () => {
        if (await copyText(text)) setCopied(true)
        else toast.error(t("common.copyFailed"))
      }}
    >
      <Icon
        className={copied ? "text-success" : undefined}
        data-icon={showLabel ? "inline-start" : undefined}
      />
      {showLabel && (copied ? t("common.copied") : label)}
    </Button>
  )
  if (showLabel) return button
  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent>{copied ? t("common.copied") : label}</TooltipContent>
    </Tooltip>
  )
}
