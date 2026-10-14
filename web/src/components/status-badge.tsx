import {
  RiErrorWarningLine,
  RiCheckboxCircleFill,
  RiTimeLine,
  RiStopCircleLine,
  RiLoader4Line,
} from "@remixicon/react"
import { cn } from "cn"

import { Badge } from "@/components/ui/badge"
import { stateLabels, type Tone } from "@/lib/format"

const icons: Record<Tone, React.ReactNode> = {
  success: <RiCheckboxCircleFill className="text-success" />,
  info: <RiLoader4Line className="animate-spin text-primary" />,
  warning: <RiTimeLine className="text-warning" />,
  danger: <RiErrorWarningLine className="text-destructive" />,
  muted: <RiStopCircleLine className="text-muted-foreground" />,
}

/** A state always pairs its color with an icon and words, never color alone. */
export function StatusBadge({
  state,
  tone,
  label,
  className,
}: {
  state: string
  tone: Tone
  label?: string
  className?: string
}) {
  return (
    <Badge variant="outline" className={cn("px-1.5 text-muted-foreground", className)}>
      {icons[tone]}
      {label ?? stateLabels[state] ?? state}
    </Badge>
  )
}
