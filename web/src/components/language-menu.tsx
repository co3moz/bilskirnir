import { RiTranslate2 } from "@remixicon/react"
import { cn } from "cn"

import {
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
} from "@/components/ui/dropdown-menu"
import {
  getChosenLocale,
  localeNames,
  locales,
  setLocale,
  t,
  useLocale,
  type Locale,
} from "@/lib/i18n"

const AUTOMATIC = "automatic"

const choose = (value: string) => setLocale(value === AUTOMATIC ? null : (value as Locale))

/** The language choice in the account menu: the browser's language, or one picked here. */
export function LanguageMenu() {
  useLocale()
  return (
    <>
      <DropdownMenuLabel className="text-xs text-muted-foreground">
        {t("account.language")}
      </DropdownMenuLabel>
      <DropdownMenuRadioGroup value={getChosenLocale() ?? AUTOMATIC} onValueChange={choose}>
        <DropdownMenuRadioItem value={AUTOMATIC}>
          {t("account.languageAutomatic")}
        </DropdownMenuRadioItem>
        {locales.map((locale) => (
          <DropdownMenuRadioItem key={locale} value={locale} lang={locale}>
            {localeNames[locale]}
          </DropdownMenuRadioItem>
        ))}
      </DropdownMenuRadioGroup>
    </>
  )
}

/** The same choice before sign-in, where there is no account menu yet. */
export function LanguageSelect({ className }: { className?: string }) {
  useLocale()
  return (
    <label className={cn("flex items-center gap-2 text-xs text-muted-foreground", className)}>
      <RiTranslate2 className="size-4" aria-hidden />
      <span className="sr-only">{t("account.language")}</span>
      <select
        className="rounded-md border bg-transparent px-2 py-1 text-xs text-foreground outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
        value={getChosenLocale() ?? AUTOMATIC}
        onChange={(event) => choose(event.target.value)}
      >
        <option value={AUTOMATIC}>{t("account.languageAutomatic")}</option>
        {locales.map((locale) => (
          <option key={locale} value={locale} lang={locale}>
            {localeNames[locale]}
          </option>
        ))}
      </select>
    </label>
  )
}
