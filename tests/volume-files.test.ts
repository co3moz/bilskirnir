import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  entryNameError,
  joinPath,
  parentPath,
  pathCrumbs,
  sortEntries,
  volumeArchiveName,
  volumeArchiveUrl,
  volumeDownloadUrl,
  type VolumeEntry,
} from "../web/src/lib/volume-files"

test("volume paths join and climb without leaving the root", () => {
  assert.equal(joinPath("/", "notes.txt"), "/notes.txt")
  assert.equal(joinPath("/project", "notes.txt"), "/project/notes.txt")
  assert.equal(parentPath("/project/assets"), "/project")
  assert.equal(parentPath("/project"), "/")
  assert.equal(parentPath("/"), "/")
  assert.deepEqual(pathCrumbs("/project/assets"), [
    { name: "/", path: "/" },
    { name: "project", path: "/project" },
    { name: "assets", path: "/project/assets" },
  ])
  assert.deepEqual(pathCrumbs("/"), [{ name: "/", path: "/" }])
})

test("a new name is one path component the manager accepts", () => {
  assert.equal(entryNameError("notes.txt"), "")
  assert.equal(entryNameError("Ünïcode name"), "")
  for (const name of ["", ".", "..", "a/b", "a\\b", "tab\tname", "x".repeat(256)])
    assert.notEqual(entryNameError(name), "", JSON.stringify(name))
  // The limit counts bytes, as the file system does.
  assert.notEqual(entryNameError("ğ".repeat(128)), "")
})

test("folders come first, then files by name", () => {
  const entry = (name: string, type: VolumeEntry["type"]): VolumeEntry => ({
    name,
    type,
    size: 0,
    modified_at: 0,
    mode: 0o644,
  })
  const sorted = sortEntries([
    entry("b.txt", "file"),
    entry("src", "directory"),
    entry("a.txt", "file"),
    entry("assets", "directory"),
  ])
  assert.deepEqual(
    sorted.map((item) => item.name),
    ["assets", "src", "a.txt", "b.txt"]
  )
})

test("an archive's file name suggests a volume name the manager accepts", () => {
  assert.equal(volumeArchiveName("unity-projects.tar.gz"), "unity-projects")
  assert.equal(volumeArchiveName("Game Saves (2).TGZ"), "Game-Saves-2-")
  assert.equal(volumeArchiveName("__backup.tar"), "backup")
  // Nothing usable is left for some names; the field then stays empty.
  assert.equal(volumeArchiveName("ü.tar.gz"), "")
  assert.equal(volumeArchiveName("x.tar"), "")
  assert.equal(volumeArchiveUrl("unity-projects"), "/api/volumes/unity-projects/archive")
})

test("downloads name the volume and the path in the query", () => {
  const url = new URL(volumeDownloadUrl("my volume", "/a b/c&d.txt"), "http://localhost")
  assert.equal(url.pathname, "/api/volumes/my%20volume/files/download")
  assert.equal(url.searchParams.get("path"), "/a b/c&d.txt")
})
