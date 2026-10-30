import * as React from "react"

import { ErrorBoundary } from "@/components/error-boundary"
import { DockerAlert, PageBody } from "@/components/page"
import { SectionCards } from "@/components/section-cards"
import { Skeleton } from "@/components/ui/skeleton"

// The charting library is the largest dependency after noVNC; load it with the charts.
const ChartAreaInteractive = React.lazy(() =>
  import("@/components/chart-area-interactive").then((module) => ({
    default: module.ChartAreaInteractive,
  }))
)
const UsageOverview = React.lazy(() =>
  import("@/components/usage-charts").then((module) => ({ default: module.UsageOverview }))
)

/** dashboard-01: summary cards, resource usage and the activity chart; containers have a page. */
export function DashboardPage() {
  return (
    <PageBody>
      <DockerAlert />
      <SectionCards />
      <div className="flex flex-col gap-4 px-4 md:gap-6 lg:px-6">
        <ErrorBoundary variant="section">
          <React.Suspense fallback={<Skeleton className="h-[22rem] rounded-xl" />}>
            <UsageOverview />
          </React.Suspense>
        </ErrorBoundary>
        <ErrorBoundary variant="section">
          <React.Suspense fallback={<Skeleton className="h-[22rem] rounded-xl" />}>
            <ChartAreaInteractive />
          </React.Suspense>
        </ErrorBoundary>
      </div>
    </PageBody>
  )
}
