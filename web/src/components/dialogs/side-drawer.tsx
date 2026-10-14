import { cn } from "cn"

import { Drawer, DrawerContent } from "@/components/ui/drawer"
import { useIsMobile } from "@/hooks/use-mobile"

export type DialogProps = { open: boolean; onOpenChange: (open: boolean) => void }

/** dashboard-01's row viewer: a panel from the right, or from the bottom on a phone. */
export function SideDrawer({
  open,
  onOpenChange,
  wide = false,
  className,
  children,
}: DialogProps & { wide?: boolean; className?: string; children: React.ReactNode }) {
  const isMobile = useIsMobile()
  return (
    <Drawer open={open} onOpenChange={onOpenChange} direction={isMobile ? "bottom" : "right"}>
      <DrawerContent
        className={cn(
          wide
            ? "data-[vaul-drawer-direction=right]:sm:max-w-2xl"
            : "data-[vaul-drawer-direction=right]:sm:max-w-md",
          className
        )}
      >
        {children}
      </DrawerContent>
    </Drawer>
  )
}
