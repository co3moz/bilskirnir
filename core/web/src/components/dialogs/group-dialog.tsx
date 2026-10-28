import * as React from "react"

import { ConceptIcon } from "@/components/concept"
import { AliasBadges } from "@/components/container-card"
import { StatusBadge } from "@/components/status-badge"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { runJobs } from "@/lib/actions"
import { closeDialog } from "@/lib/dialogs"
import { containerStatus } from "@/lib/format"
import {
  joinRefusal,
  knownGroups,
  LABEL_PATTERN,
  MOST_GROUPS,
  type JoinRefusal,
} from "@/lib/groups"
import { formatNumber, t } from "@/lib/i18n"
import { useContainers } from "@/lib/queries"
import { matchesPattern } from "@/lib/tags"
import { useSubmit } from "./container-dialogs"
import type { DialogProps } from "./side-drawer"
import { Footer } from "./simple-dialogs"

const refusalText = (refusal: JoinRefusal) =>
  refusal.kind === "full"
    ? t("groups.full", { count: formatNumber(MOST_GROUPS) })
    : t("groups.clash", { name: refusal.name, other: refusal.other })

/**
 * Connects containers to a network group: a new one named here, or `group` from its page. Each
 * chosen container gets the group added to its settings, and a running one joins it at once. A
 * container the manager would refuse, in the most groups already or reached by a name another
 * member answers to, cannot be chosen and says why.
 */
export function GroupConnectDialog({ group, ...props }: DialogProps & { group?: string }) {
  const containers = useContainers().data ?? []
  const [typed, setTyped] = React.useState("")
  const [chosen, setChosen] = React.useState<string[]>([])
  const { busy, error, submit } = useSubmit()
  const id = React.useId()
  const name = group ?? typed
  const valid = !!name && matchesPattern(name, LABEL_PATTERN)
  const members =
    (valid && knownGroups(containers).find((each) => each.name === name)?.members) || []
  const candidates = containers
    .filter((item) => !members.some((member) => member.id === item.id))
    .sort((a, b) => a.spec.name.localeCompare(b.spec.name))
  const joining = candidates.filter((item) => chosen.includes(item.id))
  const refusal = (item: (typeof containers)[number]) =>
    valid ? joinRefusal(item, name, containers, chosen) : null
  const toggle = (item: string, on: boolean) =>
    setChosen(on ? [...chosen, item] : chosen.filter((each) => each !== item))

  const connect = async () => {
    await runJobs(
      joining.map((item) => ({
        path: `containers/${item.id}/settings`,
        body: { groups: [...(item.spec.groups ?? []), name] },
      }))
    )
    closeDialog()
  }

  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-lg">
        <form className="flex flex-col gap-4" onSubmit={submit(connect)}>
          <DialogHeader>
            <DialogTitle>
              {group ? t("groups.connectTo", { name: group }) : t("groups.newTitle")}
            </DialogTitle>
            <DialogDescription>{t("groups.connectHint")}</DialogDescription>
          </DialogHeader>
          <div className="-mx-4 flex max-h-[min(36rem,70vh)] flex-col gap-5 overflow-y-auto px-4 py-1">
            <FieldGroup>
              {!group && (
                <Field>
                  <FieldLabel htmlFor={`${id}-name`}>{t("groups.name")}</FieldLabel>
                  <Input
                    id={`${id}-name`}
                    required
                    autoFocus
                    maxLength={63}
                    pattern={LABEL_PATTERN}
                    autoComplete="off"
                    spellCheck={false}
                    placeholder="build"
                    aria-describedby={`${id}-name-hint`}
                    value={typed}
                    onChange={(event) => setTyped(event.target.value.trim().toLowerCase())}
                    className="font-mono"
                  />
                  <FieldDescription id={`${id}-name-hint`} className="text-xs">
                    {members.length ? t("groups.exists") : t("groups.nameHint")}
                  </FieldDescription>
                </Field>
              )}
              <FieldSet className="gap-3">
                <FieldLegend variant="label" className="mb-0">
                  {t("groups.containers")}
                </FieldLegend>
                {candidates.length ? (
                  <ul className="flex flex-col divide-y rounded-xl border">
                    {candidates.map((item) => {
                      const on = chosen.includes(item.id)
                      const why = refusal(item)
                      const status = containerStatus(item)
                      const rowId = `${id}-${item.id}`
                      return (
                        <li key={item.id} className="px-3 py-2.5">
                          <Field
                            orientation="horizontal"
                            className="items-start gap-3"
                            data-disabled={(!!why && !on) || undefined}
                          >
                            <Checkbox
                              id={rowId}
                              className="mt-0.5"
                              checked={on}
                              // A chosen container stays choosable, so it can be left out again.
                              disabled={!!why && !on}
                              aria-describedby={why ? `${rowId}-why` : undefined}
                              onCheckedChange={(checked) => toggle(item.id, checked === true)}
                            />
                            <ConceptIcon concept="container" className="mt-0.5 size-4 shrink-0" />
                            <FieldContent className="min-w-0 gap-1">
                              <FieldLabel
                                htmlFor={rowId}
                                className="flex w-full flex-wrap items-center gap-x-2 gap-y-1 leading-snug"
                              >
                                <span className="min-w-0 truncate font-medium">
                                  {item.spec.name}
                                </span>
                                <AliasBadges aliases={item.spec.aliases} />
                              </FieldLabel>
                              {why && (
                                <FieldDescription
                                  id={`${rowId}-why`}
                                  className={on ? "text-xs text-destructive" : "text-xs"}
                                >
                                  {refusalText(why)}
                                </FieldDescription>
                              )}
                            </FieldContent>
                            <StatusBadge
                              state={status.key}
                              tone={status.tone}
                              className="shrink-0"
                            />
                          </Field>
                        </li>
                      )
                    })}
                  </ul>
                ) : (
                  <p className="text-sm text-muted-foreground">
                    {containers.length ? t("groups.allMembers") : t("groups.noContainers")}
                  </p>
                )}
              </FieldSet>
              <FieldError>{error}</FieldError>
            </FieldGroup>
          </div>
          <Footer
            busy={busy}
            label={t("groups.connectAction")}
            disabled={!valid || !joining.length || joining.some((item) => refusal(item))}
          />
        </form>
      </DialogContent>
    </Dialog>
  )
}
