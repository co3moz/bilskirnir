import * as React from "react"
import { cn } from "cn"

import { highlight, type CodeLanguage, type TokenKind } from "@/lib/highlight"

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
 * A textarea for a script or a configuration, with the code colored. The textarea stays the
 * field people type in, so labels, validation, undo and keyboard focus work as in any other;
 * its own text is transparent over a colored copy that follows its scrolling. Lines do not wrap,
 * so a line in the copy is always the line in the field.
 */
function CodeTextarea({
  language,
  className,
  value,
  onScroll,
  ...props
}: Omit<React.ComponentProps<"textarea">, "value"> & { language: CodeLanguage; value: string }) {
  const copy = React.useRef<HTMLPreElement>(null)
  const tokens = React.useMemo(() => highlight(value, language), [value, language])
  const follow = (field: HTMLTextAreaElement) => {
    if (copy.current) {
      copy.current.style.transform = `translate(${-field.scrollLeft}px, ${-field.scrollTop}px)`
    }
  }
  return (
    <div
      data-slot="code-textarea"
      className="relative w-full rounded-xl bg-input/30 has-disabled:opacity-50"
    >
      <div aria-hidden className="pointer-events-none absolute inset-px overflow-hidden rounded-xl">
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
          "relative flex field-sizing-content min-h-16 w-full resize-none overflow-auto rounded-xl border border-input bg-transparent text-transparent caret-foreground transition-colors outline-none selection:bg-primary/25 selection:text-transparent placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed aria-invalid:border-destructive aria-invalid:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
          className
        )}
        {...props}
      />
    </div>
  )
}

export { CodeTextarea }
