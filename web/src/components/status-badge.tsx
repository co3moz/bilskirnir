import {
  CircleAlertIcon,
  CircleCheckIcon,
  CircleDashedIcon,
  CircleStopIcon,
  LoaderIcon,
} from "lucide-react"
import { cn } from "cn"

import { Badge } from "@/components/ui/badge"
import { stateLabels, type Tone } from "@/lib/format"

const icons: Record<Tone, React.ReactNode> = {
  success: <CircleCheckIcon className="fill-success text-background" />,
  info: <LoaderIcon className="animate-spin text-primary" />,
  warning: <CircleDashedIcon className="text-warning" />,
  danger: <CircleAlertIcon className="text-destructive" />,
  muted: <CircleStopIcon className="text-muted-foreground" />,
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
