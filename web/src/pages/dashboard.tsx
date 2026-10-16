import * as React from "react"

import { DockerAlert, PageBody } from "@/components/page"
import { ContainersTable } from "@/components/containers-table"
import { SectionCards } from "@/components/section-cards"
import { Skeleton } from "@/components/ui/skeleton"

// The charting library is the largest dependency after noVNC; load it with the chart.
const ChartAreaInteractive = React.lazy(() =>
  import("@/components/chart-area-interactive").then((module) => ({
    default: module.ChartAreaInteractive,
  }))
)

/** dashboard-01: summary cards, the activity chart and the containers table. */
export function DashboardPage() {
  return (
    <PageBody>
      <DockerAlert />
      <SectionCards />
      <div className="px-4 lg:px-6">
        <React.Suspense fallback={<Skeleton className="h-[22rem] rounded-xl" />}>
          <ChartAreaInteractive />
        </React.Suspense>
      </div>
      <ContainersTable />
    </PageBody>
  )
}
