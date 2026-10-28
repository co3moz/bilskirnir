import * as React from "react"
import { cn } from "cn"
import { RiAddLine, RiCloseLine } from "@remixicon/react"

import { Badge } from "@/components/ui/badge"
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover"
import { t } from "@/lib/i18n"
import { matchesPattern, splitTags, tagOptions } from "@/lib/tags"

const iconButton =
  "inline-flex shrink-0 items-center justify-center rounded-full text-muted-foreground outline-none hover:bg-foreground/10 hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50"

/**
 * Names entered one at a time and shown as tags, after shadcn-tag-input
 * (https://shadcn-tag-input-kappa.vercel.app) but without cmdk, whose input takes over the field's
 * id. Enter, a comma or a space adds what was typed, as does leaving the field, and Backspace in
 * the empty field removes the last tag. `suggestions` open in a list under the field while typing,
 * on a click or with an arrow key, with `detail` beside each. Text that does not match `pattern`
 * stays in the field, whose `pattern` then keeps the form from being sent.
 */
function TagInput({
  id,
  value,
  onChange,
  suggestions = [],
  detail,
  pattern,
  normalize = (text) => text,
  placeholder,
  className,
  "aria-describedby": describedBy,
}: {
  id?: string
  value: string[]
  onChange: (value: string[]) => void
  suggestions?: readonly string[]
  detail?: (name: string) => React.ReactNode
  pattern?: string
  normalize?: (text: string) => string
  placeholder?: string
  className?: string
  "aria-describedby"?: string
}) {
  const listId = React.useId()
  const field = React.useRef<HTMLDivElement>(null)
  const input = React.useRef<HTMLInputElement>(null)
  const [text, setText] = React.useState("")
  const [open, setOpen] = React.useState(false)
  const [active, setActive] = React.useState(0)
  // Set when typed text could not be added, until it changes.
  const [refused, setRefused] = React.useState(false)
  const typed = text.trim()
  const valid = !!typed && matchesPattern(typed, pattern)
  const options = suggestions.length ? tagOptions(typed, value, suggestions, valid) : []
  const shown = open && options.length > 0
  const current = Math.min(active, options.length - 1)
  const optionId = (index: number) => `${listId}-${index}`

  React.useEffect(() => {
    if (shown) document.getElementById(`${listId}-${current}`)?.scrollIntoView({ block: "nearest" })
  }, [shown, current, listId])

  const type = (next: string) => {
    setText(next)
    setActive(0)
    setRefused(false)
  }
  const add = (names: string[]) => {
    const next = [...value]
    for (const name of names) if (!next.includes(name)) next.push(name)
    if (next.length !== value.length) onChange(next)
  }
  const commit = () => {
    if (!typed) return false
    if (!valid) {
      setRefused(true)
      return false
    }
    add([typed])
    type("")
    return true
  }
  const choose = (name: string) => {
    add([name])
    type("")
  }

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp": {
        if (!suggestions.length) return
        event.preventDefault()
        if (!shown) {
          setOpen(true)
          setActive(0)
        } else {
          const step = event.key === "ArrowDown" ? 1 : -1
          setActive((current + step + options.length) % options.length)
        }
        return
      }
      case "Enter":
        // With nothing to add, Enter sends the form as in any other field.
        if (shown) {
          event.preventDefault()
          choose(options[current]!.name)
        } else if (typed) {
          event.preventDefault()
          commit()
        }
        return
      case ",":
      case " ":
        event.preventDefault()
        commit()
        return
      case "Backspace":
        if (!text && value.length) {
          event.preventDefault()
          onChange(value.slice(0, -1))
        }
        return
    }
  }

  return (
    <Popover open={shown} onOpenChange={setOpen}>
      <PopoverAnchor asChild>
        <div
          ref={field}
          data-slot="tag-input"
          onClick={(event) => {
            if ((event.target as Element).closest("button")) return
            input.current?.focus()
            setOpen(true)
          }}
          className={cn(
            "flex min-h-9 w-full cursor-text flex-wrap items-center gap-1 rounded-[1.125rem] border border-input bg-input/30 py-1.5 pr-1.5 pl-1.5 text-base transition-colors focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50 has-aria-invalid:border-destructive has-aria-invalid:ring-[3px] has-aria-invalid:ring-destructive/20 md:text-sm dark:has-aria-invalid:border-destructive/50 dark:has-aria-invalid:ring-destructive/40",
            className
          )}
        >
          {value.map((name) => (
            <Badge key={name} variant="secondary" className="h-6 overflow-visible pr-0.5 font-mono">
              {name}
              <button
                type="button"
                aria-label={t("tagInput.remove", { name })}
                onClick={() => {
                  onChange(value.filter((other) => other !== name))
                  input.current?.focus()
                }}
                className={cn(iconButton, "size-5 [&_svg]:size-3.5")}
              >
                <RiCloseLine />
              </button>
            </Badge>
          ))}
          <input
            ref={input}
            id={id}
            role={suggestions.length ? "combobox" : undefined}
            aria-autocomplete={suggestions.length ? "list" : undefined}
            aria-expanded={suggestions.length ? shown : undefined}
            aria-controls={shown ? listId : undefined}
            aria-activedescendant={shown ? optionId(current) : undefined}
            aria-describedby={describedBy}
            aria-invalid={refused || undefined}
            pattern={pattern}
            autoComplete="off"
            autoCapitalize="none"
            spellCheck={false}
            placeholder={value.length ? undefined : placeholder}
            value={text}
            onChange={(event) => {
              const { names, rest } = splitTags(normalize(event.target.value))
              const good = names.filter((name) => matchesPattern(name, pattern))
              add(good)
              // A pasted name that is not valid stays in the field to be corrected.
              type(
                [...names.filter((name) => !good.includes(name)), rest].filter(Boolean).join(" ")
              )
              setOpen(true)
            }}
            onKeyDown={onKeyDown}
            onBlur={(event) => {
              setOpen(false)
              if (!field.current?.contains(event.relatedTarget)) commit()
            }}
            className="h-6 min-w-16 flex-1 bg-transparent px-1.5 font-mono outline-none placeholder:font-sans placeholder:text-muted-foreground"
          />
          {(value.length > 0 || text) && (
            <button
              type="button"
              aria-label={t("tagInput.clear")}
              onClick={() => {
                if (text) type("")
                else onChange([])
                input.current?.focus()
              }}
              className={cn(iconButton, "ml-auto size-6 [&_svg]:size-4")}
            >
              <RiCloseLine />
            </button>
          )}
        </div>
      </PopoverAnchor>
      <PopoverContent
        align="start"
        // A dialog's body scrolls; the list goes with the field when it scrolls out of sight.
        hideWhenDetached
        className="max-h-60 w-(--radix-popover-trigger-width) gap-0 overflow-y-auto p-1"
        // The input keeps the focus while the list is open, and a click in the list keeps it too.
        onOpenAutoFocus={(event) => event.preventDefault()}
        onCloseAutoFocus={(event) => event.preventDefault()}
        onInteractOutside={(event) => {
          if (field.current?.contains(event.target as Node)) event.preventDefault()
        }}
        onMouseDown={(event) => event.preventDefault()}
      >
        <div role="listbox" id={listId}>
          {options.map((option, index) => (
            <div
              key={option.name}
              id={optionId(index)}
              role="option"
              aria-selected={index === current}
              onPointerMove={() => setActive(index)}
              onClick={() => choose(option.name)}
              className="flex cursor-default items-center gap-2 rounded-xl px-3 py-2 select-none aria-selected:bg-foreground/10"
            >
              {option.isNew && <RiAddLine className="size-4 shrink-0 text-muted-foreground" />}
              <span className="min-w-0 flex-1 truncate font-mono">{option.name}</span>
              <span className="shrink-0 text-xs text-muted-foreground">
                {option.isNew ? t("tagInput.new") : detail?.(option.name)}
              </span>
            </div>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  )
}

export { TagInput }
