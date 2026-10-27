import { createElement, Fragment, useSyncExternalStore, type ReactNode } from "react"

import { de } from "@/locales/de"
import { en, type MessageKey, type Messages } from "@/locales/en"
import { es } from "@/locales/es"
import { fr } from "@/locales/fr"
import { tr } from "@/locales/tr"

export type { MessageKey } from "@/locales/en"

export const catalogs = { en, de, fr, es, tr } satisfies Record<string, Messages>
export type Locale = keyof typeof catalogs
export const locales = Object.keys(catalogs) as Locale[]

/** Each language in its own words, as the language menu lists it. */
export const localeNames: Record<Locale, string> = {
  en: "English",
  de: "Deutsch",
  fr: "Français",
  es: "Español",
  tr: "Türkçe",
}

/** The regional form dates and numbers follow. */
const regions: Record<Locale, string> = {
  en: "en-US",
  de: "de-DE",
  fr: "fr-FR",
  es: "es-ES",
  tr: "tr-TR",
}

const STORAGE_KEY = "thoric-locale"

const isLocale = (value: unknown): value is Locale =>
  typeof value === "string" && Object.hasOwn(catalogs, value)

/** The first of the browser's languages there is a catalog for, English otherwise. */
export function detectLocale(languages: readonly string[]): Locale {
  for (const language of languages) {
    const base = language.toLowerCase().split(/[-_]/)[0]
    if (isLocale(base)) return base
  }
  return "en"
}

function browserLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return []
  return navigator.languages?.length ? navigator.languages : [navigator.language ?? ""]
}

function stored(): Locale | null {
  try {
    const value = localStorage.getItem(STORAGE_KEY)
    return isLocale(value) ? value : null
  } catch {
    // Storage can be blocked; the browser's language still applies.
    return null
  }
}

// Outside a browser, as in the unit tests, the interface is English unless one is chosen.
const inBrowser = typeof window !== "undefined"
const automatic = () => (inBrowser ? detectLocale(browserLanguages()) : "en")
let chosen: Locale | null = inBrowser ? stored() : null
let current: Locale = chosen ?? automatic()
const listeners = new Set<() => void>()

function apply() {
  if (typeof document !== "undefined") document.documentElement.lang = current
}
apply()

export const getLocale = () => current
/** The language picked in the menu, or null when the browser's language applies. */
export const getChosenLocale = () => chosen

/** Pick a language, or follow the browser's again with null. */
export function setLocale(locale: Locale | null) {
  chosen = locale
  try {
    if (locale) localStorage.setItem(STORAGE_KEY, locale)
    else localStorage.removeItem(STORAGE_KEY)
  } catch {
    // The choice still applies to this page.
  }
  current = locale ?? automatic()
  apply()
  listeners.forEach((listener) => listener())
}

const subscribe = (listener: () => void) => {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** The current language; a component that renders text calls this to follow a change. */
export const useLocale = () => useSyncExternalStore(subscribe, getLocale, getLocale)

/** Whether a key built at run time, such as `state.${state}`, names a message. */
export const has = (key: string): key is MessageKey => Object.hasOwn(en, key)

export type Values = Record<string, string | number>

/** Fill `{name}` placeholders; a value that is not given keeps its placeholder. */
export function interpolate(template: string, values?: Values) {
  if (!values) return template
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.hasOwn(values, name) ? String(values[name]) : match
  )
}

/**
 * A message in the current language. Call it while rendering or handling an event, never at
 * module scope, so a change of language reaches it.
 */
export function t(key: MessageKey, values?: Values) {
  return interpolate(catalogs[current][key] ?? en[key], values)
}

/**
 * A message whose placeholders hold elements, such as a path in `<code>`: the parts in order,
 * for React to render, so each language keeps its own word order around them.
 */
export function rich(key: MessageKey, values: Record<string, ReactNode>): ReactNode[] {
  return t(key)
    .split(/(\{\w+\})/)
    .filter(Boolean)
    .map((part, index) => {
      const name = /^\{(\w+)\}$/.exec(part)?.[1]
      const node = name && Object.hasOwn(values, name) ? values[name] : part
      return createElement(Fragment, { key: index }, node)
    })
}

/**
 * The plural form of a message for `count`: `key.one` or `key.other`, as the language's own
 * rules choose; `{count}` is filled in.
 */
export function plural(
  count: number,
  forms: { one: MessageKey; other: MessageKey },
  values?: Values
) {
  const form = new Intl.PluralRules(regions[current]).select(count) === "one" ? "one" : "other"
  return t(forms[form], { count: formatNumber(count), ...values })
}

export const formatNumber = (value: number, options?: Intl.NumberFormatOptions) =>
  new Intl.NumberFormat(regions[current], options).format(value)

export const formatDate = (seconds: number, options?: Intl.DateTimeFormatOptions) =>
  new Date(seconds * 1000).toLocaleString(regions[current], options)

/** A time relative to now, short, such as "5 min. ago" or "yesterday", in the current language. */
export const formatRelative = (value: number, unit: Intl.RelativeTimeFormatUnit) =>
  new Intl.RelativeTimeFormat(regions[current], { numeric: "auto", style: "short" }).format(
    value,
    unit
  )

/** A list such as "a, b and c" in the current language, without the serial comma in English. */
export const formatList = (items: string[]) =>
  new Intl.ListFormat(current === "en" ? "en-GB" : regions[current], {
    type: "conjunction",
  }).format(items)
