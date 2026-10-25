import { RiApps2Line, RiPlugLine } from "@remixicon/react"

import { ImageIcon } from "@/components/image-icon"
import { FieldLegend, FieldSet } from "@/components/ui/field"
import { formatList, t } from "@/lib/i18n"
import type { ServerDeclaration } from "@/lib/types"

/**
 * What a template build makes, for someone who installs it as it is: its icon, its description,
 * which is not changed here, and the desktop MCP servers it brings. Advanced editing shows the rest.
 */
export function BuildSummary({
  description,
  icon,
  servers,
}: {
  description: string
  icon?: string
  servers: ServerDeclaration[]
}) {
  return (
    <FieldSet className="gap-2">
      <FieldLegend variant="label" className="mb-0">
        {t("common.description")}
      </FieldLegend>
      <div className="flex items-start gap-3 rounded-xl bg-muted/60 p-3">
        <ImageIcon src={icon} fallback={RiApps2Line} className="size-12 rounded-lg" />
        <div className="flex min-w-0 flex-col gap-2 text-sm">
          <p className="break-words whitespace-pre-line">
            {description.trim() || t("templates.noDescription")}
          </p>
          {servers.length > 0 && (
            <p className="flex items-start gap-1.5 text-xs text-muted-foreground">
              <RiPlugLine className="mt-px size-3.5 shrink-0" aria-hidden />
              {t("images.mcpServers", {
                names: formatList(servers.map((server) => server.name)),
              })}
            </p>
          )}
        </div>
      </div>
    </FieldSet>
  )
}
