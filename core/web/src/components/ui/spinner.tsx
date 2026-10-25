import { cn } from "cn"
import { RiLoaderLine } from "@remixicon/react"

import { t } from "@/lib/i18n"

function Spinner({ className, ...props }: React.ComponentProps<typeof RiLoaderLine>) {
  return (
    <RiLoaderLine
      data-slot="spinner"
      role="status"
      aria-label={t("common.loading")}
      className={cn("size-4 animate-spin", className)}
      {...props}
    />
  )
}

export { Spinner }
