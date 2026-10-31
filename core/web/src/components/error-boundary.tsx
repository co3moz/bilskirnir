import * as React from "react"
import { RiAlertLine, RiRefreshLine } from "@remixicon/react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { isChunkError, reloadForChunk } from "@/lib/chunk-reload"
import { t } from "@/lib/i18n"

type Props = {
  children: React.ReactNode
  /** `page` fills the window in place of everything; `section` stands in for one part of a page. */
  variant?: "page" | "section"
  className?: string
}
type State = { error: unknown }

const message = (error: unknown) => (error instanceof Error ? error.message : String(error))

/**
 * Shows what failed in place of a part that threw while rendering, rather than a blank page. A
 * chunk that failed to load, as after a dashboard update, reloads the page once by itself.
 */
export class ErrorBoundary extends React.Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: unknown): State {
    return { error: error ?? new Error("Unknown error") }
  }

  componentDidCatch(error: unknown) {
    reloadForChunk(error)
  }

  render() {
    const { error } = this.state
    if (error === null) return this.props.children
    const chunk = isChunkError(error)
    const title = chunk ? t("errors.updated") : t("errors.title")
    const detail = chunk ? t("errors.updatedHint") : message(error)
    const actions = (
      <>
        {!chunk && (
          <Button variant="outline" size="sm" onClick={() => this.setState({ error: null })}>
            {t("errors.retry")}
          </Button>
        )}
        <Button size="sm" onClick={() => location.reload()}>
          <RiRefreshLine data-icon="inline-start" />
          {t("errors.reload")}
        </Button>
      </>
    )
    if (this.props.variant === "section")
      return (
        <div className={this.props.className}>
          <Alert variant="destructive">
            <RiAlertLine />
            <AlertTitle>{title}</AlertTitle>
            <AlertDescription className="flex flex-col gap-3">
              <span className="break-words">{detail}</span>
              <span className="flex flex-wrap gap-2">{actions}</span>
            </AlertDescription>
          </Alert>
        </div>
      )
    return (
      <Empty className="min-h-svh bg-sidebar">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <RiAlertLine />
          </EmptyMedia>
          <EmptyTitle>{title}</EmptyTitle>
          <EmptyDescription className="break-words">{detail}</EmptyDescription>
        </EmptyHeader>
        <EmptyContent className="flex-row justify-center">{actions}</EmptyContent>
      </Empty>
    )
  }
}
