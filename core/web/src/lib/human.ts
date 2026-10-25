import { useQuery } from "@tanstack/react-query"

import { api } from "./api"
import { t } from "./i18n"
import { queryClient, streamedInterval } from "./queries"
import type { HumanRequest } from "./types"

/**
 * Agents' requests for a person: what an agent cannot do itself in its desktop, such as a CAPTCHA
 * or a sign-in, waits on every page and above that desktop until someone answers it here.
 */

export const HUMAN_REQUESTS = ["human-requests"]

/** The requests that wait for a person, oldest first, which the event stream keeps current. */
export const useHumanRequests = () =>
  useQuery({
    queryKey: HUMAN_REQUESTS,
    queryFn: () => api<{ items: HumanRequest[] }>("human-requests").then((data) => data.items),
    refetchInterval: () => streamedInterval(true),
  })

/** The waiting requests after news of one: a new one joins them, an answered one leaves. */
export function withRequest(list: HumanRequest[] | undefined, request: HumanRequest) {
  if (!list) return list
  const listed = list.some((item) => item.id === request.id)
  if (request.status !== "pending")
    return listed ? list.filter((item) => item.id !== request.id) : list
  if (listed) return list
  return [...list, request].sort((a, b) => a.created_at - b.created_at)
}

/** The requests a later read of the list has that an earlier one did not. */
export function newRequests(before: HumanRequest[], after: HumanRequest[]) {
  const known = new Set(before.map((request) => request.id))
  return after.filter((request) => !known.has(request.id))
}

/** One container's requests. */
export const requestsFor = (list: HumanRequest[], container: string) =>
  list.filter((request) => request.container_id === container)

/** How long ago a request came, to the minute: finer than that tells a waiting person nothing. */
export function askedAgo(createdAt: number, now: number) {
  const seconds = Math.max(0, now - createdAt)
  if (seconds < 60) return t("human.justNow")
  if (seconds < 3600) return t("human.minutesAgo", { minutes: Math.floor(seconds / 60) })
  return t("human.hoursAgo", { hours: Math.floor(seconds / 3600) })
}

/** What answering sends: the answer, and the reply when there is one. */
export function answerBody(answer: "done" | "decline", reply: string) {
  const text = reply.trim()
  return text ? { answer, reply: text } : { answer }
}

/** Answer a request. It leaves this page's list at once; the event tells the other pages. */
export async function answerRequest(id: string, answer: "done" | "decline", reply: string) {
  const answered = await api<HumanRequest>(
    `human-requests/${encodeURIComponent(id)}/answer`,
    answerBody(answer, reply)
  )
  queryClient.setQueryData<HumanRequest[]>(HUMAN_REQUESTS, (list) => withRequest(list, answered))
  return answered
}
