import { Fragment, StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClientProvider } from "@tanstack/react-query"

import "./index.css"
import { App } from "@/App"
import { ThemeProvider } from "@/components/theme-provider"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { useLocale } from "@/lib/i18n"
import { queryClient } from "@/lib/queries"

/**
 * Renders the interface again in a newly chosen language. Text is looked up while rendering, so
 * a fresh render is all a change needs; the query cache outlives it.
 */
function Localized({ children }: { children: React.ReactNode }) {
  return <Fragment key={useLocale()}>{children}</Fragment>
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <TooltipProvider>
          <Localized>
            <App />
          </Localized>
          <Toaster />
        </TooltipProvider>
      </QueryClientProvider>
    </ThemeProvider>
  </StrictMode>
)
