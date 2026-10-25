import * as React from "react"
import { toast } from "sonner"

import { IconPicker } from "@/components/image-icon"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { api } from "@/lib/api"
import { closeDialog } from "@/lib/dialogs"
import { iconUrl } from "@/lib/icons"
import { t } from "@/lib/i18n"
import { refreshAll, useImages } from "@/lib/queries"
import { useSubmit } from "./container-dialogs"
import type { DialogProps } from "./side-drawer"
import { Footer } from "./simple-dialogs"

/** Changes or removes the icon a saved image shows in lists. */
export function IconDialog({ id, ...props }: DialogProps & { id: string }) {
  const image = useImages().data?.find((entry) => entry.id === id)
  // `undefined` keeps the current icon, `null` removes it and a data URL replaces it.
  const [choice, setChoice] = React.useState<string | null>()
  const [iconError, setIconError] = React.useState<string>()
  const { busy, error, submit } = useSubmit()
  const field = React.useId()
  const shown = choice === undefined ? image && iconUrl(image) : (choice ?? undefined)
  const name = image?.name ?? ""

  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-md">
        <form
          className="flex flex-col gap-4"
          onSubmit={submit(async () => {
            if (choice === null) await api(`images/${id}/icon`, undefined, "DELETE")
            else if (choice) await api(`images/${id}/icon`, { icon: choice }, "PUT")
            closeDialog()
            toast.success(
              choice === null ? t("icons.removed", { name }) : t("icons.saved", { name })
            )
            void refreshAll()
          })}
        >
          <DialogHeader>
            <DialogTitle>{t("icons.titleOf", { name })}</DialogTitle>
            <DialogDescription>{t("icons.description")}</DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field data-invalid={!!iconError}>
              <FieldLabel htmlFor={`${field}-icon`}>{t("icons.label")}</FieldLabel>
              <IconPicker
                id={`${field}-icon`}
                value={shown}
                onChange={(value) => setChoice(value ?? null)}
                onError={setIconError}
              />
              <FieldError>{iconError}</FieldError>
            </Field>
            <FieldError>{error}</FieldError>
          </FieldGroup>
          <Footer busy={busy} label={t("common.save")} disabled={choice === undefined} />
        </form>
      </DialogContent>
    </Dialog>
  )
}
