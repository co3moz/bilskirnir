import { useInfiniteQuery, useQuery } from "@tanstack/react-query"

import { activityPath, type ActivityFilters, type ActivityPage } from "./activity"
import { api } from "./api"

/** How often the log is read again while it shows. */
const INTERVAL = 5000

/**
 * The log, newest first, a page at a time. Polling reads every loaded page again from the newest,
 * so new calls arrive at the top and the older pages keep following their cursors.
 */
export const useActivity = (filters: ActivityFilters) =>
  useInfiniteQuery({
    queryKey: ["activity", filters],
    queryFn: ({ pageParam }) => api<ActivityPage>(activityPath(filters, pageParam)),
    initialPageParam: undefined as number | undefined,
    getNextPageParam: (last) => last.next ?? undefined,
    refetchInterval: INTERVAL,
    meta: { quiet: true },
  })

/** The newest few calls, for a container's or an endpoint's own list. */
export const useRecentActivity = (filters: ActivityFilters, limit: number) =>
  useQuery({
    queryKey: ["activity", filters, limit],
    queryFn: () => api<ActivityPage>(activityPath(filters, undefined, limit)),
    refetchInterval: INTERVAL,
    meta: { quiet: true },
  })
