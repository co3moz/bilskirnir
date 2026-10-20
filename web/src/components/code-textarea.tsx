import * as React from "react"
import { cn } from "cn"

import { highlight, lineNumbers, type CodeLanguage, type TokenKind } from "@/lib/highlight"

const COLORS: Record<TokenKind, string | undefined> = {
  plain: undefined,
  comment: "text-code-comment italic",
  keyword: "text-code-keyword",
  command: "text-code-command",
  string: "text-code-string",
  variable: "text-code-variable",
  number: "text-code-number",
  operator: "text-code-operator",
}

// The textarea and the colored copy under it must lay text out the same way, to the pixel.
const TEXT = "px-3 py-3 font-mono text-xs whitespace-pre [tab-size:4]"

/**
 * A textarea for a script or a configuration, with the code colored and its lines numbered. The
 * textarea stays the field people type in, so labels, validation, undo and keyboard focus work as
 * in any other; its own text is transparent over a colored copy that follows its scrolling. Lines
 * do not wrap, so a line in the copy is always the line in the field and its number in the gutter.
 */
function CodeTextarea({
  language,
  className,
  value,
  onScroll,
  ...props
}: Omit<React.ComponentProps<"textarea">, "value"> & { language: CodeLanguage; value: string }) {
  const copy = React.useRef<HTMLPreElement>(null)
  const gutter = React.useRef<HTMLDivElement>(null)
  const tokens = React.useMemo(() => highlight(value, language), [value, language])
  const numbers = React.useMemo(() => lineNumbers(value), [value])
  const follow = (field: HTMLTextAreaElement) => {
    if (copy.current) {
      copy.current.style.transform = `translate(${-field.scrollLeft}px, ${-field.scrollTop}px)`
    }
    // The numbers move up and down only, so they stay in view when the field scrolls sideways.
    if (gutter.current) gutter.current.style.transform = `translateY(${-field.scrollTop}px)`
  }
  return (
    <div
      data-slot="code-textarea"
      className="flex w-full overflow-hidden rounded-xl border border-input bg-input/30 transition-colors has-disabled:opacity-50 has-[textarea:focus-visible]:border-ring has-[textarea:focus-visible]:ring-[3px] has-[textarea:focus-visible]:ring-ring/50 has-[textarea[aria-invalid=true]]:border-destructive has-[textarea[aria-invalid=true]]:ring-[3px] has-[textarea[aria-invalid=true]]:ring-destructive/20 dark:has-[textarea[aria-invalid=true]]:border-destructive/50 dark:has-[textarea[aria-invalid=true]]:ring-destructive/40"
    >
      <div
        aria-hidden
        className="shrink-0 overflow-hidden border-r border-input bg-muted/40 py-3 pr-2 pl-3 text-right font-mono text-xs text-muted-foreground select-none"
      >
        <div ref={gutter} className="min-w-[2ch] whitespace-pre">
          {numbers}
        </div>
      </div>
      <div className="relative min-w-0 flex-1">
        <div aria-hidden className="pointer-events-none absolute inset-0 overflow-hidden">
          <pre ref={copy} className={cn(TEXT, "m-0 w-max text-foreground")}>
            {tokens.map((token, index) =>
              COLORS[token.kind] ? (
                <span key={index} className={COLORS[token.kind]}>
                  {token.text}
                </span>
              ) : (
                token.text
              )
            )}
            {/* A last empty line still takes its height, as it does in the field. */}
            {value.endsWith("\n") && " "}
          </pre>
        </div>
        <textarea
          data-slot="textarea"
          wrap="off"
          spellCheck={false}
          autoCapitalize="off"
          autoComplete="off"
          autoCorrect="off"
          value={value}
          onScroll={(event) => {
            follow(event.currentTarget)
            onScroll?.(event)
          }}
          onInput={(event) => follow(event.currentTarget)}
          className={cn(
            TEXT,
            "relative block field-sizing-content min-h-16 w-full resize-none overflow-auto bg-transparent text-transparent caret-foreground outline-none selection:bg-primary/25 selection:text-transparent placeholder:text-muted-foreground disabled:cursor-not-allowed",
            className
          )}
          {...props}
        />
      </div>
    </div>
  )
}

export { CodeTextarea }
