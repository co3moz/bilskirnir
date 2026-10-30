import { afterEach, test } from "bun:test"
import assert from "node:assert/strict"

import {
  closeDialog,
  confirmDiscard,
  discardQuestion,
  dismissDialog,
  hold,
  keepEditing,
  openDialog,
  openedDialog,
  whenDiscarded,
  whenLeaving,
} from "../src/lib/dialogs"

const releases: (() => void)[] = []
const held = (...args: Parameters<typeof hold>) => {
  const release = hold(...args)
  releases.push(release)
  return release
}

afterEach(() => {
  releases.splice(0).forEach((release) => release())
  closeDialog()
})

test("a dialog without changes closes when its user asks", () => {
  openDialog({ kind: "volume" })
  dismissDialog()
  assert.equal(openedDialog(), null)
  assert.equal(discardQuestion(), null)
})

test("a form with changes asks before Escape, a click outside or Cancel closes it", () => {
  const seq = openDialog({ kind: "build" })
  const release = held(seq, "changes")
  dismissDialog()
  assert.deepEqual(openedDialog(), { kind: "build" })
  assert.equal(discardQuestion()?.reason, "changes")
  // Keeping the changes leaves the form as it was.
  keepEditing()
  assert.equal(discardQuestion(), null)
  assert.deepEqual(openedDialog(), { kind: "build" })
  dismissDialog()
  confirmDiscard()
  assert.equal(openedDialog(), null)
  // Changes undone, or sent, no longer hold it.
  openDialog({ kind: "build" })
  release()
  dismissDialog()
  assert.equal(openedDialog(), null)
})

test("running uploads ask with their own question, before a form's changes", () => {
  const seq = openDialog({ kind: "upload", id: "c1" })
  held(seq, "changes")
  held(seq, "upload")
  dismissDialog()
  assert.equal(discardQuestion()?.reason, "upload")
})

test("a toast's button replaces a form with changes only once the user agrees", () => {
  const seq = openDialog({ kind: "mcp-endpoint" })
  held(seq, "changes")
  let ran = false
  whenDiscarded(() => {
    ran = true
    openDialog({ kind: "job", id: "j1" })
  })
  assert.equal(ran, false)
  assert.deepEqual(openedDialog(), { kind: "mcp-endpoint" })
  confirmDiscard()
  assert.equal(ran, true)
  assert.deepEqual(openedDialog(), { kind: "job", id: "j1" })

  // Without changes it goes ahead at once.
  whenDiscarded(() => openDialog({ kind: "job", id: "j2" }))
  assert.deepEqual(openedDialog(), { kind: "job", id: "j2" })
})

test("what a replaced dialog or the page holds does not hold the dialog open now", () => {
  const first = openDialog({ kind: "create" })
  held(first, "changes")
  // A sent form opens its job's log in its place; the form's changes went with it.
  openDialog({ kind: "job", id: "j1" })
  held(null, "upload")
  dismissDialog()
  assert.equal(openedDialog(), null)
  assert.equal(discardQuestion(), null)
})

test("closing a dialog from code drops a waiting question", () => {
  const seq = openDialog({ kind: "compose", id: "i1" })
  held(seq, "changes")
  dismissDialog()
  assert.ok(discardQuestion())
  // As signing out does.
  closeDialog()
  assert.equal(discardQuestion(), null)
  assert.equal(openedDialog(), null)
})

test("leaving a page asks for its uploads, and for its dialog's changes only when the page goes", () => {
  const seq = openDialog({ kind: "settings", id: "c1" })
  held(seq, "changes")
  const steps: string[] = []
  const leave = (within = false) =>
    whenLeaving(
      () => steps.push("left"),
      () => steps.push("stayed"),
      within
    )
  // A filter or an item opened on the same page keeps the dialog as it is.
  leave(true)
  assert.deepEqual(steps, ["left"])
  leave()
  assert.equal(discardQuestion()?.reason, "changes")
  keepEditing()
  assert.deepEqual(steps, ["left", "stayed"])
  assert.deepEqual(openedDialog(), { kind: "settings", id: "c1" })

  closeDialog()
  // The Files page uploading holds even a move to another container's files.
  const release = held(null, "upload")
  leave(true)
  assert.equal(discardQuestion()?.reason, "upload")
  confirmDiscard()
  assert.deepEqual(steps, ["left", "stayed", "left"])
  release()
  leave()
  assert.deepEqual(steps, ["left", "stayed", "left", "left"])
})
