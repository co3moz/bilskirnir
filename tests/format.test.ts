import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  downloadProgress,
  containerStatus,
  imageChoices,
  imageTree,
  jobActivity,
  jobLogText,
  jobProgress,
  jobTarget,
  needsAttention,
  restartNotice,
  subfolderError,
  uploadLocations,
} from "../web/src/lib/format"
import type { ContainerItem, ContainerSetup, JobStatus, ProfileItem } from "../web/src/lib/types"

test("completed legacy deletion explains the result rather than waiting or displaying an ID", () => {
  const log = jobLogText({
    status: "completed",
    log: "",
    result: { deleted: "old-container-id", volumes_preserved: true },
  })
  assert.equal(log, "Container deleted. Named volumes were preserved.")
  assert.equal(jobLogText({ status: "queued", log: "" }), "Waiting to start…")
  assert.equal(
    jobLogText({ status: "failed", log: "" }),
    "Operation failed. No detailed log was recorded."
  )
  assert.equal(
    jobLogText({ status: "failed", log: "Ownership check failed." }),
    "Ownership check failed."
  )
})

test("deep snapshot history cannot overflow the JavaScript call stack", () => {
  const records = Array.from({ length: 20000 }, (_, i) => ({
    id: `r${i}`,
    image_id: `image${i}`,
    parent_image_id: i ? `image${i - 1}` : null,
  }))
  const tree = imageTree(records)
  assert.equal(tree.length, 20000)
  assert.equal(tree.at(-1)!.depth, 19999)
})

test("image branches follow immutable parent image IDs, not mutable tags", () => {
  const records = [
    { id: "b", image_id: "sha:b", parent_image_id: "sha:a" },
    { id: "c", image_id: "sha:c", parent_image_id: "sha:a" },
    { id: "a", image_id: "sha:a", parent_image_id: "sha:base" },
  ]
  assert.deepEqual(
    imageTree(records).map((r) => [r.id, r.depth]),
    [
      ["a", 0],
      ["b", 1],
      ["c", 1],
    ]
  )
})

test("missing parents, duplicate image IDs and cycles remain visible", () => {
  const records = [
    { id: "a", image_id: "sha:a", parent_image_id: "sha:b" },
    { id: "b", image_id: "sha:b", parent_image_id: "sha:a" },
    { id: "c", image_id: "sha:a", parent_image_id: null },
  ]
  assert.equal(imageTree(records).length, 3)
  assert.equal(new Set(imageTree(records).map((r) => r.id)).size, 3)
})

test("profile choices distinguish GNOME, XFCE and unavailable images", () => {
  const profile = (reference: string, name: string) => ({
    id: reference,
    reference,
    name,
    session: "",
    description: "",
  })
  const profiles: ProfileItem[] = [
    {
      profile: profile("xfce:24.04", "Ubuntu · Lightweight XFCE"),
      available: true,
      image_id: "sha256:x",
    },
    { profile: profile("gnome:24.04", "Ubuntu · GNOME"), available: false, image_id: null },
    {
      profile: profile("other:24.04", "Ubuntu · Other"),
      available: false,
      image_id: "sha256:wrong",
      error: "Image labels do not match the profile",
    },
  ]
  const choices = imageChoices(
    profiles,
    [
      {
        image_id: "sha256:custom",
        name: "My game",
        desktop: { profile: "", title: "Ubuntu · GNOME", session: "" },
      },
    ],
    "xfce:24.04"
  )
  assert.equal(choices.length, 4)
  // A profile that is not downloaded stays selectable: creating from it downloads the image first.
  assert.deepEqual(
    [choices[1].disabled, choices[1].label],
    [false, "Ubuntu · GNOME · downloads first"]
  )
  assert.deepEqual([choices[2].disabled, choices[2].label], [true, "Ubuntu · Other"])
  assert.equal(choices[3].label, "My game · Ubuntu · GNOME")
  assert.ok(imageChoices([], [], "wsl-desktop/ubuntu:24.04")[0].label.includes("Lightweight XFCE"))
})

test("download progress reads as bytes once the size is known and never claims a size it lacks", () => {
  assert.equal(downloadProgress(null), null)
  assert.deepEqual(downloadProgress({ current: 0, total: 0, layers: 0, layers_done: 0 }), {
    text: "Preparing download…",
  })
  // Until every remaining layer reports its size there is no honest bar, only the count so far.
  assert.deepEqual(
    downloadProgress({ current: 3 * 1024 ** 2, total: null, layers: 4, layers_done: 1 }),
    {
      text: "3.0 MiB downloaded · 1 of 4 layers",
    }
  )
  assert.deepEqual(downloadProgress({ current: 0, total: 0, layers: 2, layers_done: 2 }), {
    text: "Checking layers · 2 of 2 layers",
  })
  const running = downloadProgress({
    current: 1024 ** 3,
    total: 4 * 1024 ** 3,
    layers: 4,
    layers_done: 1,
  })
  assert.deepEqual(running, {
    value: 1024 ** 3,
    max: 4 * 1024 ** 3,
    percent: 25,
    text: "1.0 GiB of 4.0 GiB · 1 of 4 layers",
  })
  assert.equal(
    downloadProgress({ current: 10, total: 10, layers: 4, layers_done: 3 })!.text,
    "Extracting · 3 of 4 layers"
  )
})

test("the restart warning names the settings that recreate the container and what happens", () => {
  assert.equal(
    restartNotice(["audio"], true),
    "Audio recreates the container: it restarts and open applications close. Its files are kept."
  )
  assert.equal(
    restartNotice(["gpu", "mounts", "cpus"], false),
    "GPU, volumes and removing the CPU limit recreate the container: it stays stopped until you start it. Its files are kept."
  )
})

test("uploads never target a read-only mount, nor Downloads under one", () => {
  const mount = (volume: string, target: string, readonly: boolean) => ({
    volume,
    target,
    readonly,
  })
  assert.deepEqual(
    uploadLocations({
      mounts: [mount("assets", "/assets", true), mount("work", "/work", false)],
    }).map((location) => location.target),
    ["/home/desktop/Downloads", "/work"]
  )
  // A read-only volume over the home folder covers Downloads too.
  assert.deepEqual(uploadLocations({ mounts: [mount("home", "/home/desktop", true)] }), [])
  assert.deepEqual(uploadLocations({ mounts: [mount("dl", "/home/desktop/Downloads", true)] }), [])
  assert.equal(uploadLocations({ mounts: [mount("docs", "/home/desktop/Doc", true)] }).length, 1)
})

test("an upload subfolder stays inside its destination", () => {
  assert.equal(subfolderError(""), "")
  assert.equal(subfolderError("assets/textures"), "")
  for (const folder of ["/etc", "a/../..", "a//b", "./a", "a\\b", "a/"])
    assert.notEqual(subfolderError(folder), "", folder)
})

test("job activity counts finished jobs per local day and leaves out running ones", () => {
  const now = new Date(2026, 9, 14, 12)
  const at = (days: number, hour = 10) => new Date(2026, 9, 14 - days, hour).getTime() / 1000
  const job = (status: JobStatus, created: number, finished: number | null = created) => ({
    status,
    created_at: created,
    finished_at: finished,
  })
  const points = jobActivity(
    [
      job("completed", at(0)),
      job("failed", at(0)),
      job("interrupted", at(1)),
      // A job that started yesterday counts on the day it finished.
      job("completed", at(1, 23), at(0, 1)),
      job("running", at(0), null),
      job("queued", at(0), null),
      job("completed", at(7)),
    ],
    7,
    now
  )
  assert.equal(points.length, 7)
  assert.equal(points[0].date, "2026-10-08")
  assert.deepEqual(points.at(-1), { date: "2026-10-14", completed: 2, failed: 1 })
  assert.deepEqual(points.at(-2), { date: "2026-10-13", completed: 0, failed: 1 })
  assert.equal(
    points.reduce((sum, point) => sum + point.completed + point.failed, 0),
    4
  )
})

test("a job names what it worked on, even after that container was deleted", () => {
  const containers = new Map([["e1", "Unity workspace"]])
  const profiles = new Map([["ubuntu-gnome", "Ubuntu 24.04 · GNOME"]])
  const target = (resource: string | null, result?: Record<string, unknown>) =>
    jobTarget({ resource, result }, containers, profiles)
  assert.equal(target("container:e1"), "Unity workspace")
  assert.equal(target("container:gone", { deleted: "gone", name: "Old QA" }), "Old QA")
  assert.equal(target("container:0123456789abcdef"), "Container 01234567")
  // Jobs recorded before the rename still find their container.
  assert.equal(target("environment:e1"), "Unity workspace")
  assert.equal(target("profile:ubuntu-gnome"), "Ubuntu 24.04 · GNOME")
  assert.equal(target("profile:retired"), "retired")
  assert.equal(target(null), "")
  const images = new Map([["i1", "Unity snapshot"]])
  assert.equal(jobTarget({ resource: "image:i1" }, containers, profiles, images), "Unity snapshot")
  assert.equal(
    jobTarget({ resource: "image:new", result: { name: "Imported" } }, containers, profiles),
    "Imported"
  )
  assert.equal(
    jobTarget({ resource: "image:0123456789ab" }, containers, profiles),
    "Image 01234567"
  )
})

test("archive jobs count bytes; image downloads keep counting layers", () => {
  assert.deepEqual(jobProgress({ transfer: "export", current: 0 }), {
    text: "Docker is preparing the image…",
  })
  assert.deepEqual(jobProgress({ transfer: "export", current: 3 * 1024 ** 3 }), {
    text: "3.0 GiB exported",
  })
  assert.deepEqual(jobProgress({ transfer: "import", current: 1024 ** 3, total: 4 * 1024 ** 3 }), {
    value: 1024 ** 3,
    max: 4 * 1024 ** 3,
    percent: 25,
    text: "1.0 GiB of 4.0 GiB received",
  })
  assert.deepEqual(
    jobProgress({ current: 0, total: 0, layers: 0, layers_done: 0 }),
    downloadProgress({ current: 0, total: 0, layers: 0, layers_done: 0 })
  )
  assert.equal(jobProgress(null), null)
})

test("a running container shows its compose setup until the setup completes", () => {
  const item = (status?: ContainerSetup["status"], last_error?: string): ContainerItem => ({
    id: "e",
    container: "wsld-e",
    source_image_id: "sha256:a",
    created_at: 0,
    spec: {} as ContainerItem["spec"],
    last_error,
    setup: status ? { status, image: "i", parameters: {}, secrets: [] } : null,
    state: "running",
    health: "healthy",
    status: "Up",
  })
  assert.equal(containerStatus(item()).key, "running")
  assert.equal(containerStatus(item("pending")).key, "setting_up")
  assert.equal(containerStatus(item("running")).key, "setting_up")
  assert.equal(containerStatus(item("completed")).key, "running")
  assert.equal(containerStatus(item("failed", "The compose setup failed")).key, "setup_failed")
  // A setup that never started because recreating the container failed is not in progress.
  assert.equal(containerStatus(item("pending", "Docker: no such image")).key, "setup_failed")
  assert.equal(needsAttention(item("failed")), true)
  assert.equal(needsAttention(item("completed")), false)
  // Until the desktop answers its health check, it is starting, whatever the setup says.
  assert.equal(containerStatus({ ...item("pending"), health: "starting" }).key, "created")
})
