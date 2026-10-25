import * as React from "react"
import { RiComputerLine, RiUserVoiceLine } from "@remixicon/react"
import { toast } from "sonner"
import { cn } from "cn"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import { answerRequest, askedAgo, requestsFor, useHumanRequests } from "@/lib/human"
import { t } from "@/lib/i18n"
import { href } from "@/lib/route"
import type { HumanRequest } from "@/lib/types"

/** The time in Unix seconds, read again every `every` milliseconds. */
function useNow(every: number) {
  const [now, setNow] = React.useState(() => Math.floor(Date.now() / 1000))
  React.useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), every)
    return () => clearInterval(timer)
  }, [every])
  return now
}

/**
 * An optional reply and the two answers, after `lead`; Enter in the reply answers Done. Where the
 * row is narrow the answers wrap below the reply, to its right.
 */
function Answer({
  request,
  lead,
  className,
}: {
  request: HumanRequest
  lead?: React.ReactNode
  className?: string
}) {
  const [reply, setReply] = React.useState("")
  const [sending, setSending] = React.useState<"done" | "decline" | null>(null)
  const send = async (answer: "done" | "decline") => {
    setSending(answer)
    try {
      await answerRequest(request.id, answer, reply)
    } catch (error) {
      toast.error((error as Error).message)
      setSending(null)
    }
  }
  return (
    <form
      className={cn("flex min-w-0 flex-wrap items-center gap-2", className)}
      onSubmit={(event) => {
        event.preventDefault()
        void send("done")
      }}
    >
      {lead}
      <Input
        value={reply}
        onChange={(event) => setReply(event.target.value)}
        maxLength={1000}
        placeholder={t("human.replyPlaceholder")}
        aria-label={t("human.replyTo", { name: request.container_name })}
        className="h-8 w-auto min-w-44 flex-1 sm:max-w-sm"
      />
      <div className="ml-auto flex gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={!!sending}
          onClick={() => void send("decline")}
        >
          {sending === "decline" && <Spinner data-icon="inline-start" />}
          {t("human.decline")}
        </Button>
        <Button type="submit" size="sm" disabled={!!sending}>
          {sending === "done" && <Spinner data-icon="inline-start" />}
          {t("human.done")}
        </Button>
      </div>
    </form>
  )
}

/**
 * The agents waiting for a person, above every dashboard page: which desktop, what is needed and
 * since when, a way to it and the answers.
 */
export function HumanRequestsBanner() {
  const requests = useHumanRequests().data ?? []
  const now = useNow(30_000)
  if (!requests.length) return null
  return (
    <section aria-label={t("human.title")} className="flex flex-col gap-2 px-4 pt-4 lg:px-6">
      {requests.map((request) => (
        <article
          key={request.id}
          aria-label={t("human.asks", { name: request.container_name })}
          className="flex gap-3 rounded-2xl border border-warning/40 bg-warning/5 p-3"
        >
          <RiUserVoiceLine aria-hidden className="mt-0.5 size-5 shrink-0 text-warning" />
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            <div className="flex flex-col gap-0.5">
              <div className="flex flex-wrap items-baseline justify-between gap-x-3">
                <h2 className="text-sm font-medium">
                  {t("human.asks", { name: request.container_name })}
                </h2>
                <p className="text-xs text-muted-foreground">
                  {askedAgo(request.created_at, now)}
                  {request.endpoint && ` · ${t("human.via", { endpoint: request.endpoint })}`}
                </p>
              </div>
              <p className="text-sm break-words whitespace-pre-wrap">{request.message}</p>
            </div>
            <Answer
              request={request}
              lead={
                <Button variant="outline" size="sm" asChild>
                  <a
                    href={href({ page: "desktop", id: request.container_id })}
                    aria-label={t("human.openDesktop")}
                  >
                    <RiComputerLine data-icon="inline-start" />
                    <span className="hidden sm:inline">{t("human.openDesktop")}</span>
                  </a>
                </Button>
              }
            />
          </div>
        </article>
      ))}
    </section>
  )
}

/** This desktop's requests, in a strip under the desktop view's toolbar. */
export function HumanRequestStrip({ container }: { container: string }) {
  const requests = requestsFor(useHumanRequests().data ?? [], container)
  const shown = requests.map((request) => request.id).join(" ")
  // The strip says it all here; a toast pointing at this desktop would only cover it.
  React.useEffect(() => {
    for (const id of shown.split(" ")) if (id) toast.dismiss(`human-${id}`)
  }, [shown])
  if (!requests.length) return null
  return (
    <section
      aria-label={t("human.title")}
      className="flex max-h-[40vh] shrink-0 flex-col divide-y divide-warning/30 overflow-y-auto border-b border-warning/40 bg-warning/10"
    >
      {requests.map((request) => (
        <div key={request.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2">
          <RiUserVoiceLine aria-hidden className="size-4 shrink-0 text-warning" />
          <p className="min-w-0 flex-1 basis-64 text-sm break-words whitespace-pre-wrap">
            <span className="sr-only">{t("human.asks", { name: request.container_name })}: </span>
            {request.message}
          </p>
          <Answer request={request} className="flex-1 basis-72 sm:max-w-md" />
        </div>
      ))}
    </section>
  )
}
