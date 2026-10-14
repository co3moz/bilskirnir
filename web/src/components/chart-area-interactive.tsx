import * as React from "react"
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts"

import { useIsMobile } from "@/hooks/use-mobile"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { jobActivity } from "@/lib/format"
import { useJobs } from "@/lib/queries"

const chartConfig = {
  completed: { label: "Completed", color: "var(--primary)" },
  failed: { label: "Failed or interrupted", color: "var(--destructive)" },
} satisfies ChartConfig

const ranges = [
  { value: "90", label: "Last 3 months" },
  { value: "30", label: "Last 30 days" },
  { value: "7", label: "Last 7 days" },
]

const day = (value: string) =>
  new Date(`${value}T00:00:00`).toLocaleDateString("en-US", { month: "short", day: "numeric" })

/** Finished jobs per day: how busy the manager has been and whether operations succeed. */
export function ChartAreaInteractive() {
  const isMobile = useIsMobile()
  const [range, setRange] = React.useState("30")
  const jobs = useJobs().data
  const days = isMobile ? 7 : Number(range)
  const data = React.useMemo(() => jobActivity(jobs ?? [], days), [jobs, days])
  const total = data.reduce((sum, point) => sum + point.completed + point.failed, 0)
  const selected = ranges.find((item) => item.value === String(days))?.label ?? "Last 7 days"

  return (
    <Card className="@container/card">
      <CardHeader>
        <CardTitle>Job activity</CardTitle>
        <CardDescription>
          <span className="hidden @[540px]/card:block">
            {total} finished {total === 1 ? "job" : "jobs"}, {selected.toLowerCase()}, from the
            latest 100
          </span>
          <span className="@[540px]/card:hidden">{selected}</span>
        </CardDescription>
        <CardAction>
          <ToggleGroup
            type="single"
            value={String(days)}
            onValueChange={(value) => value && setRange(value)}
            variant="outline"
            className="hidden *:data-[slot=toggle-group-item]:px-4! @[767px]/card:flex"
          >
            {ranges.map((item) => (
              <ToggleGroupItem key={item.value} value={item.value}>
                {item.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <Select value={String(days)} onValueChange={setRange}>
            <SelectTrigger
              className="flex w-40 **:data-[slot=select-value]:block **:data-[slot=select-value]:truncate @[767px]/card:hidden"
              size="sm"
              aria-label="Time range"
            >
              <SelectValue placeholder="Last 30 days" />
            </SelectTrigger>
            <SelectContent className="rounded-xl">
              {ranges.map((item) => (
                <SelectItem key={item.value} value={item.value} className="rounded-lg">
                  {item.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </CardAction>
      </CardHeader>
      <CardContent className="px-2 pt-4 sm:px-6 sm:pt-6">
        <ChartContainer config={chartConfig} className="aspect-auto h-[250px] w-full">
          <AreaChart data={data} margin={{ left: 0, right: 8 }}>
            <defs>
              <linearGradient id="fillCompleted" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="var(--color-completed)" stopOpacity={0.8} />
                <stop offset="95%" stopColor="var(--color-completed)" stopOpacity={0.1} />
              </linearGradient>
              <linearGradient id="fillFailed" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="var(--color-failed)" stopOpacity={0.8} />
                <stop offset="95%" stopColor="var(--color-failed)" stopOpacity={0.1} />
              </linearGradient>
            </defs>
            <CartesianGrid vertical={false} />
            <XAxis
              dataKey="date"
              tickLine={false}
              axisLine={false}
              tickMargin={8}
              minTickGap={32}
              tickFormatter={day}
            />
            <YAxis allowDecimals={false} tickLine={false} axisLine={false} width={28} />
            <ChartTooltip
              cursor={false}
              content={
                <ChartTooltipContent
                  labelFormatter={(value) => day(String(value))}
                  indicator="dot"
                />
              }
            />
            <Area
              dataKey="failed"
              type="monotone"
              fill="url(#fillFailed)"
              stroke="var(--color-failed)"
              stackId="a"
            />
            <Area
              dataKey="completed"
              type="monotone"
              fill="url(#fillCompleted)"
              stroke="var(--color-completed)"
              stackId="a"
            />
            <ChartLegend content={<ChartLegendContent />} />
          </AreaChart>
        </ChartContainer>
      </CardContent>
    </Card>
  )
}
