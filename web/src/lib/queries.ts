import { QueryCache, QueryClient, useQuery } from "@tanstack/react-query"
import { toast } from "sonner"

import { api, onUnauthorized } from "./api"
import { closeDialog } from "./dialogs"
import { jobActive } from "./format"
import type {
  ContainerItem,
  ImageRecord,
  Job,
  ProfileItem,
  Session,
  Settings,
  SystemInfo,
  Volume,
} from "./types"

const LIST_INTERVAL = 8000
const JOB_INTERVAL = 2000

let signedOut = false

export const queryClient = new QueryClient({
  queryCache: new QueryCache({
    // Several lists fail together when Docker is down; one toast per distinct message is enough.
    onError: (error) => {
      if (!signedOut) toast.error(error.message, { id: error.message })
    },
  }),
  defaultOptions: {
    queries: { retry: false, staleTime: 1000, refetchOnWindowFocus: true },
  },
})

onUnauthorized(() => {
  // Requests still in flight from the old session must not toast over the sign-in page.
  signedOut = true
  closeDialog()
  queryClient.removeQueries({ predicate: (query) => query.queryKey[0] !== "session" })
  queryClient.setQueryData<Session>(["session"], (session) => ({
    auth_enabled: session?.auth_enabled ?? true,
    authenticated: false,
  }))
})

export function signedIn() {
  signedOut = false
}

export const useSession = () =>
  useQuery({ queryKey: ["session"], queryFn: () => api<Session>("session"), staleTime: Infinity })

export const useSettings = () =>
  useQuery({
    queryKey: ["settings"],
    queryFn: () => api<Settings>("settings"),
    staleTime: Infinity,
  })

// Lists refresh while a page that shows them is open and visible; the desktop view opts out.
function useList<T>(path: string, poll: boolean) {
  return useQuery({
    queryKey: [path],
    queryFn: () => api<T>(path),
    refetchInterval: poll ? LIST_INTERVAL : false,
  })
}

export const useContainers = (poll = true) => useList<ContainerItem[]>("containers", poll)
export const useImages = (poll = true) => useList<ImageRecord[]>("images", poll)
export const useProfiles = (poll = true) => useList<ProfileItem[]>("profiles", poll)
export const useVolumes = (poll = true) => useList<Volume[]>("volumes", poll)
export const useJobs = (poll = true) => useList<Job[]>("jobs", poll)
export const useSystem = (poll = true) => useList<SystemInfo>("system", poll)

/** One job with its log, followed until it finishes. */
export const useJob = (id: string | undefined) =>
  useQuery({
    queryKey: ["jobs", id],
    queryFn: () => api<Job>(`jobs/${id}`),
    enabled: !!id,
    refetchInterval: (query) =>
      query.state.data && !jobActive(query.state.data) ? false : JOB_INTERVAL,
  })

export const refreshAll = () => queryClient.invalidateQueries()
