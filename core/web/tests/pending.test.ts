import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  catalogImageJobs,
  creationEnded,
  pendingContainers,
  pendingImages,
  pendingState,
} from "../src/lib/pending"
import type { ContainerItem, Job } from "../src/lib/types"

const job = (fields: Partial<Job> & Pick<Job, "id" | "kind">): Job => ({
  status: "running",
  created_at: 1,
  finished_at: null,
  log: "",
  ...fields,
})

const container = (id: string, state: string, name = id) =>
  ({ id, state, spec: { name } }) as unknown as ContainerItem

test("a container shows as pending from its creation job until its desktop runs", () => {
  const jobs = [
    job({ id: "j1", kind: "create", resource: "container:new", name: "Agent desktop" }),
    job({ id: "j2", kind: "create", resource: "container:saved", name: "Old name" }),
    job({ id: "j3", kind: "create", resource: "container:up", name: "Up" }),
    job({ id: "j4", kind: "create", resource: "container:done", status: "completed" }),
    job({ id: "j5", kind: "start", resource: "container:other", status: "queued" }),
  ]
  // The record is saved after the image download, before Docker has a container for it.
  const records = [container("saved", "missing", "Saved"), container("up", "running")]
  assert.deepEqual(
    pendingContainers(jobs, records).map(({ id, name, job }) => [id, name, job.id]),
    [
      ["new", "Agent desktop", "j1"],
      ["saved", "Saved", "j2"],
    ]
  )
})

test("a creation job from a manager that names nothing still gets a name", () => {
  const [entry] = pendingContainers(
    [job({ id: "j", kind: "create", resource: "container:x", status: "queued" })],
    []
  )
  assert.equal(entry.name, "Create container")
})

test("a saved image shows as pending until its record is listed", () => {
  const jobs = [
    job({ id: "b1", kind: "build", resource: "build:uuid", name: "Tools" }),
    job({ id: "s1", kind: "snapshot", resource: "container:work", name: "Before upgrade" }),
    job({ id: "i1", kind: "image-import", resource: "image:imported", name: "Moved" }),
    job({ id: "b2", kind: "build", resource: "build:other", name: "Saved already" }),
    job({ id: "b3", kind: "build", resource: "build:failed", status: "failed" }),
    job({ id: "p1", kind: "image-pull", resource: "image:ubuntu-xfce" }),
  ]
  // A build's record is saved under the job's ID before its cleanup stage ends the job.
  const images = [{ id: "b2" }, { id: "ubuntu-xfce" }]
  assert.deepEqual(
    pendingImages(jobs, images).map(({ id, name, kind }) => [id, name, kind]),
    [
      ["b1", "Tools", "build"],
      ["s1", "Before upgrade", "snapshot"],
      ["imported", "Moved", "archive"],
    ]
  )
  assert.deepEqual(
    pendingImages(jobs, [...images, { id: "imported" }, { id: "b1" }, { id: "s1" }]),
    []
  )
})

test("a catalog image row follows the newest job downloading or building it", () => {
  const found = catalogImageJobs([
    job({ id: "new", kind: "source-build", resource: "image:ubuntu-xfce" }),
    job({ id: "old", kind: "image-pull", resource: "image:ubuntu-xfce", status: "queued" }),
    job({ id: "done", kind: "image-pull", resource: "image:ubuntu-gnome", status: "completed" }),
    job({ id: "export", kind: "image-export", resource: "image:ubuntu-gnome" }),
  ])
  assert.deepEqual(
    [...found].map(([id, job]) => [id, job.id]),
    [["ubuntu-xfce", "new"]]
  )
})

test("a pending badge says whether the job waits, downloads or creates", () => {
  assert.equal(pendingState({ kind: "create", status: "queued" }), "queued")
  assert.equal(pendingState({ kind: "image-pull", status: "running" }), "downloading")
  // A container or a build whose catalog base image is still downloading.
  const download = { current: 1, total: 2, layers: 3, layers_done: 1 }
  assert.equal(
    pendingState({ kind: "create", status: "running", progress: download }),
    "downloading"
  )
  assert.equal(pendingState({ kind: "create", status: "running" }), "creating")
  const received = { current: 1, total: 2, transfer: "import" as const }
  assert.equal(
    pendingState({ kind: "image-import", status: "running", progress: received }),
    "creating"
  )
})

test("the lists are read again only when a creation job ends", () => {
  const running = [
    job({ id: "c", kind: "create" }),
    job({ id: "s", kind: "stop", resource: "container:a" }),
  ]
  const finished = (id: string) => job({ id, kind: "create", status: "completed", finished_at: 2 })
  assert.equal(creationEnded(running, running), false)
  assert.equal(creationEnded(running, [finished("c"), running[1]]), true)
  // The job list keeps the latest jobs only; one gone from it has ended too.
  assert.equal(creationEnded(running, [running[1]]), true)
  assert.equal(
    creationEnded(running, [running[0], { ...running[1], status: "completed" }]),
    false,
    "other jobs change nothing the pending entries show"
  )
  assert.equal(creationEnded([], [finished("c")]), false)
})
