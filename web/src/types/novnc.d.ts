// The part of noVNC's RFB API the desktop view uses; see node_modules/@novnc/novnc/docs/API.md.
declare module "@novnc/novnc" {
  export default class RFB extends EventTarget {
    constructor(
      target: HTMLElement,
      url: string,
      options?: { shared?: boolean; wsProtocols?: string[] }
    )
    scaleViewport: boolean
    resizeSession: boolean
    showDotCursor: boolean
    focusOnClick: boolean
    viewOnly: boolean
    disconnect(): void
    focus(): void
    clipboardPasteFrom(text: string): void
  }
}
