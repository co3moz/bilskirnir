import * as React from "react"

import { Button } from "@/components/ui/button"
import { FieldError } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { t } from "@/lib/i18n"
import { entryNameError } from "@/lib/volume-files"

/** A name for a new folder or a renamed entry, asked for inline above a file list. */
export function NameForm({
  label,
  initial,
  busy,
  onSubmit,
  onCancel,
}: {
  label: string
  initial: string
  busy: boolean
  onSubmit: (name: string) => void
  onCancel: () => void
}) {
  const [name, setName] = React.useState(initial)
  const [error, setError] = React.useState("")
  return (
    <form
      className="flex flex-col gap-1.5"
      onSubmit={(event) => {
        event.preventDefault()
        const problem = entryNameError(name.trim())
        setError(problem)
        if (!problem) onSubmit(name.trim())
      }}
    >
      <div className="flex gap-2">
        <Input
          aria-label={label}
          placeholder={label}
          value={name}
          autoFocus
          disabled={busy}
          onChange={(event) => setName(event.target.value)}
          className="font-mono"
        />
        <Button type="submit" disabled={busy}>
          {t("common.save")}
        </Button>
        <Button type="button" variant="outline" disabled={busy} onClick={onCancel}>
          {t("common.cancel")}
        </Button>
      </div>
      <FieldError>{error}</FieldError>
    </form>
  )
}
