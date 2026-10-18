import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  canonicalFolder,
  defaultFolder,
  desktopFolder,
  listingProblem,
  quickFolders,
  recentFolders,
  rememberFolder,
  uploadProblem,
} from "../web/src/lib/desktop-paths"
import type { Container, Mount } from "../web/src/lib/types"

const mount = (volume: string, target: string, readonly = false): Mount => ({
  volume,
  target,
  readonly,
})

// The cases of uploads_only_accept_canonical_paths_in_writable_locations in src/tests.rs, so
// the dialog refuses what the manager would and lets through what it accepts.
const container = (mounts: Mount[] = []): Pick<Container, "spec" | "setup"> => ({
  spec: { mounts } as Container["spec"],
  setup: {
    status: "completed",
    image: "img",
    parameters: {},
    secrets: [],
    places: [{ label: "Unity project", path: "/srv/unity/MyGame" }],
  },
})
const shared = container([
  mount("projects", "/shared"),
  mount("reference", "/shared/reference", true),
])

test("uploads go anywhere in the home, a writable volume or a compose folder", () => {
  for (const path of [
    "/home/desktop",
    "/home/desktop/Downloads",
    "/home/desktop/Downloads-fake",
    "/home/desktop/.config/autostart",
    "/shared",
    "/shared/assets",
    "/srv/unity/MyGame",
    "/srv/unity/MyGame/Assets",
  ])
    assert.equal(uploadProblem(shared, path), "", path)
  for (const path of [
    "/",
    "/etc",
    "/tmp",
    "/home",
    "/home/other",
    "/home/desktopper",
    "/sharedish",
    "/shared/reference",
    "/shared/reference/sub",
    "/srv/unity",
    "/srv/unity/MyGameOld",
    "/home/desktop/Downloads/../.ssh",
    "/home/desktop/",
    "/shared//assets",
    "/shared/.",
    "relative/folder",
  ])
    assert.notEqual(uploadProblem(shared, path), "", path)
  // A read-only volume inside the home closes that folder and nothing else.
  const downloads = container([mount("downloads", "/home/desktop/Downloads", true)])
  assert.notEqual(uploadProblem(downloads, "/home/desktop/Downloads/sub"), "")
  assert.equal(uploadProblem(downloads, "/home/desktop/Documents"), "")
  // A compose's volume counts like a volume setting.
  const composed = {
    ...container(),
    setup: { ...container().setup!, mounts: [mount("u", "/opt/unity")] },
  }
  assert.equal(uploadProblem(composed, "/opt/unity/Editors"), "")
})

test("a folder parameter names a canonical folder outside the system ones", () => {
  // The cases of a_folder_is_a_canonical_desktop_folder_outside_the_system_ones.
  for (const path of [
    "/home/desktop/UnityProjects/My Game",
    "/home/desktop/.local/share/godot",
    "/srv/projects",
    "/opt/unity/projects",
    "/workspace,2026",
  ])
    assert.ok(desktopFolder(path), path)
  for (const path of [
    "relative/folder",
    "/",
    "/home",
    "/home/desktop/",
    "/home/desktop//Projects",
    "/home/desktop/../root",
    "/etc/unity",
    "/tmp/build",
    "/usr/local/projects",
    "/opt",
    "/opt/VirtualGL/x",
    "/home/desktop/two\nlines",
    "C:\\Projects",
  ])
    assert.ok(!desktopFolder(path), path)
  // Browsing may still open the root.
  assert.ok(canonicalFolder("/"))
  assert.ok(!canonicalFolder("/a\u0085b"))
})

test("the browser lists each writable place once, then what was used last", () => {
  const folders = quickFolders(
    container([
      mount("projects", "/shared"),
      mount("reference", "/shared/reference", true),
      mount("unity", "/srv/unity/MyGame"),
    ]),
    ["/shared", "/home/desktop/Pictures", "/etc"]
  )
  assert.deepEqual(
    folders.map((folder) => [folder.kind, folder.path]),
    [
      ["home", "/home/desktop"],
      ["home", "/home/desktop/Desktop"],
      ["home", "/home/desktop/Documents"],
      ["home", "/home/desktop/Downloads"],
      ["compose", "/srv/unity/MyGame"],
      ["volume", "/shared"],
      ["recent", "/home/desktop/Pictures"],
    ]
  )
  assert.equal(folders[4].label, "Unity project")
})

test("a new upload starts in the folder used last, else Downloads, else the home", () => {
  assert.equal(defaultFolder(shared, ["/etc", "/shared/assets"]), "/shared/assets")
  assert.equal(defaultFolder(shared), "/home/desktop/Downloads")
  const downloads = container([mount("downloads", "/home/desktop/Downloads", true)])
  assert.equal(defaultFolder(downloads), "/home/desktop")
})

test("recent folders are kept per container, newest first, and survive missing storage", () => {
  const saved = new Map<string, string>()
  const storage = globalThis.localStorage
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (key: string) => saved.get(key) ?? null,
      setItem: (key: string, value: string) => saved.set(key, value),
    },
  })
  try {
    for (let index = 0; index < 7; index++) rememberFolder("a", `/home/desktop/${index}`)
    rememberFolder("a", "/home/desktop/3")
    rememberFolder("b", "/shared")
    assert.deepEqual(recentFolders("a"), [
      "/home/desktop/3",
      "/home/desktop/6",
      "/home/desktop/5",
      "/home/desktop/4",
      "/home/desktop/2",
    ])
    assert.deepEqual(recentFolders("b"), ["/shared"])
    saved.set("bilskirnir.upload-folders", "not json")
    assert.deepEqual(recentFolders("a"), [])
  } finally {
    Object.defineProperty(globalThis, "localStorage", { configurable: true, value: storage })
  }
})

test("a folder that cannot be listed says why in words", () => {
  const failed = (errno: number) =>
    listingProblem(`File operation failed: [Errno ${errno}] Something: 'x'`)
  assert.match(failed(2), /does not exist yet/)
  assert.match(failed(13), /cannot open/)
  assert.match(failed(20), /is a file/)
  assert.match(failed(40), /symlink/)
  assert.equal(listingProblem("Start the container"), "Start the container")
})
