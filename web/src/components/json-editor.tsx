import * as React from "react"
import { cn } from "cn"

import { CodeTextarea, CodeView } from "@/components/code-textarea"
import { t } from "@/lib/i18n"
import { editorHeight, modelUri } from "@/lib/json-editor"
import type { JsonSchema } from "@/lib/schema-form"

type Setup = typeof import("./monaco-setup")
type Editor = ReturnType<Setup["monaco"]["editor"]["create"]>

let loading: Promise<Setup> | undefined
let loaded: Setup | undefined

/**
 * Monaco, once its chunk has loaded. Until then, after it failed, and on a touch screen, which
 * Monaco does not support, the fields fall back to `CodeTextarea` and `CodeView`.
 */
function useMonaco(): Setup | undefined {
  // Asked when each field appears, so a laptop that turned into a tablet gets the plain field.
  const [touch] = React.useState(() => window.matchMedia("(pointer: coarse)").matches)
  const [setup, setSetup] = React.useState(() => (touch ? undefined : loaded))
  React.useEffect(() => {
    if (setup || touch) return
    let live = true
    loading ??= import("./monaco-setup").then((module) => (loaded = module))
    loading.then(
      (module) => live && setSetup(module),
      () => (loading = undefined)
    )
    return () => {
      live = false
    }
  }, [setup, touch])
  return setup
}

type Common = {
  value: string
  /** What the field is, for screen readers. */
  label: string
  /** The schema the text is checked against, with completion and hovers from it. */
  schema?: JsonSchema
  /** The least and the most the field grows to, in pixels; it scrolls past the most. */
  minHeight?: number
  maxHeight?: number
}

/** JSON to edit: Monaco with the schema's completion, hovers and checks, once it has loaded. */
export function JsonEditor({
  onChange,
  onSubmit,
  placeholder,
  invalid,
  ...common
}: Common & {
  onChange(value: string): void
  /** Ctrl+Enter, which Monaco would otherwise take for a new line. */
  onSubmit?(): void
  placeholder?: string
  invalid?: boolean
}) {
  const setup = useMonaco()
  if (!setup)
    return (
      <CodeTextarea
        language="json"
        value={common.value}
        onChange={(event) => onChange(event.target.value)}
        aria-label={common.label}
        aria-invalid={invalid || undefined}
        placeholder={placeholder}
        style={{ minHeight: common.minHeight }}
      />
    )
  return (
    <Monaco
      setup={setup}
      {...common}
      onChange={onChange}
      onSubmit={onSubmit}
      placeholder={placeholder}
      invalid={invalid}
    />
  )
}

/** JSON to read, with its objects and arrays folding; Monaco once it has loaded. */
export function JsonView({ className, ...common }: Common & { className?: string }) {
  const setup = useMonaco()
  if (!setup)
    return (
      <CodeView language="json" value={common.value} label={common.label} className={className} />
    )
  return <Monaco setup={setup} {...common} readOnly className={className} />
}

function Monaco({
  setup,
  value,
  label,
  schema,
  minHeight = 36,
  maxHeight = 384,
  readOnly = false,
  onChange,
  onSubmit,
  placeholder,
  invalid,
  className,
}: Common & {
  setup: Setup
  readOnly?: boolean
  onChange?(value: string): void
  onSubmit?(): void
  placeholder?: string
  invalid?: boolean
  className?: string
}) {
  const host = React.useRef<HTMLDivElement>(null)
  const editor = React.useRef<Editor | null>(null)
  const id = React.useId()
  const uri = modelUri(id.replace(/[^A-Za-z0-9]/g, ""))
  // What the editor reads when it is made and when it calls back, kept current without
  // making it again; the effects below pass later changes on.
  const props = { value, onChange, onSubmit, minHeight, maxHeight, readOnly, label, placeholder }
  const latest = React.useRef(props)
  React.useEffect(() => {
    latest.current = props
  })

  React.useEffect(() => {
    const { monaco } = setup
    const first = latest.current
    const model = monaco.editor.createModel(first.value, "json", monaco.Uri.parse(uri))
    const created = monaco.editor.create(host.current!, {
      model,
      readOnly: first.readOnly,
      domReadOnly: first.readOnly,
      ariaLabel: first.label,
      placeholder: first.placeholder,
      automaticLayout: true,
      fixedOverflowWidgets: true,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      wordWrap: "on",
      lineNumbers: first.readOnly ? "off" : "on",
      lineNumbersMinChars: 2,
      lineDecorationsWidth: 6,
      glyphMargin: false,
      folding: true,
      renderLineHighlight: "none",
      overviewRulerLanes: 0,
      overviewRulerBorder: false,
      hideCursorInOverviewRuler: true,
      stickyScroll: { enabled: false },
      fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-mono"),
      fontSize: 12,
      lineHeight: 18,
      padding: { top: 8, bottom: 8 },
      tabSize: 2,
      wordBasedSuggestions: "off",
      quickSuggestions: { strings: true, other: true, comments: false },
      // A field that does not scroll lets the page scroll under the wheel.
      scrollbar: {
        alwaysConsumeMouseWheel: false,
        verticalScrollbarSize: 8,
        horizontalScrollbarSize: 8,
      },
    })
    editor.current = created
    // The field grows with its text and shows a scrollbar only once the text is taller.
    let scrolls: boolean | undefined
    const fit = () => {
      const { minHeight: least, maxHeight: most } = latest.current
      const content = created.getContentHeight()
      if (host.current) host.current.style.height = `${editorHeight(content, least, most)}px`
      if (scrolls !== content > most) {
        scrolls = content > most
        created.updateOptions({ scrollbar: { vertical: scrolls ? "auto" : "hidden" } })
      }
    }
    const resize = created.onDidContentSizeChange(fit)
    fit()
    const changes = model.onDidChangeContent(() => latest.current.onChange?.(model.getValue()))
    created.addAction({
      id: "bilskirnir.submit",
      label: t("inspector.run.keys"),
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter],
      run: () => latest.current.onSubmit?.(),
    })
    return () => {
      changes.dispose()
      resize.dispose()
      created.dispose()
      model.dispose()
      editor.current = null
    }
  }, [setup, uri])

  // A value set from outside, such as Clear or the form, replaces the text.
  React.useEffect(() => {
    const model = editor.current?.getModel()
    if (model && model.getValue() !== value) model.setValue(value)
  }, [value])

  React.useEffect(() => {
    editor.current?.updateOptions({
      readOnly,
      domReadOnly: readOnly,
      ariaLabel: label,
      placeholder,
    })
  }, [readOnly, label, placeholder])

  // Schemas are compared as text: a caller may make an equal one on every render.
  const schemaText = schema ? JSON.stringify(schema) : ""
  React.useEffect(() => {
    setup.setSchema(uri, schemaText ? JSON.parse(schemaText) : undefined)
    return () => setup.setSchema(uri, undefined)
  }, [setup, uri, schemaText])

  return (
    <div
      ref={host}
      data-slot="json-editor"
      style={{ height: minHeight }}
      className={cn(
        "w-full min-w-0 overflow-hidden rounded-xl border border-input bg-input/30 transition-colors focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50",
        invalid &&
          "border-destructive ring-[3px] ring-destructive/20 dark:border-destructive/50 dark:ring-destructive/40",
        className
      )}
    />
  )
}
