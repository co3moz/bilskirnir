/* eslint-disable @typescript-eslint/no-explicit-any -- the fakes stand in for XMLHttpRequest and File */
import { test } from "bun:test"
import assert from "node:assert/strict"

import { FileUploads } from "../web/src/lib/uploads"

function fixture() {
  const requests: any[] = []
  let unauthorized = 0
  const uploads = new FileUploads({
    createRequest: () => {
      const request: any = {
        upload: {},
        headers: {},
        open(_method: string, url: string) {
          request.url = url
        },
        setRequestHeader(key: string, value: string) {
          request.headers[key] = value
        },
        send(file: unknown) {
          request.file = file
        },
        abort() {
          request.onabort()
        },
        complete(status: number, result: unknown) {
          request.status = status
          request.responseText = JSON.stringify(result)
          request.onload()
        },
      }
      requests.push(request)
      return request
    },
    onUnauthorized: () => unauthorized++,
  })
  return {
    uploads,
    requests,
    get unauthorized() {
      return unauthorized
    },
  }
}
const files = (...list: { name: string; size: number }[]) => list as any[]
const options = { target: "containers/container", destination: "/shared/project", replace: false }
const tick = () => new Promise((resolve) => setImmediate(resolve))

test("files upload sequentially with encoded names, CSRF and progress", async () => {
  const { uploads, requests } = fixture()
  uploads.select(files({ name: "file & #.txt", size: 10 }, { name: "empty.txt", size: 0 }), 100)
  const done = uploads.start(options)
  assert.equal(requests.length, 1)
  assert.equal(
    new URL(requests[0].url, "http://localhost").searchParams.get("name"),
    "file & #.txt"
  )
  assert.equal(
    new URL(requests[0].url, "http://localhost").pathname,
    "/api/containers/container/files"
  )
  assert.equal(requests[0].headers["X-Bilskirnir-Request"], "1")
  requests[0].upload.onprogress({ loaded: 5 })
  assert.equal(uploads.items[0].loaded, 5)
  requests[0].complete(200, { path: "/shared/project/file & #.txt" })
  await tick()
  assert.equal(requests.length, 2)
  requests[1].complete(200, { path: "/shared/project/empty.txt" })
  await done
  assert.equal(uploads.running, false)
  assert.ok(uploads.items.every((item) => item.status === "completed"))
})

test("a volume receives uploads at its own files route", async () => {
  const { uploads, requests } = fixture()
  uploads.select(files({ name: "notes.txt", size: 3 }), 100)
  const done = uploads.start({ target: "volumes/project", destination: "/", replace: true })
  const url = new URL(requests[0].url, "http://localhost")
  assert.equal(url.pathname, "/api/volumes/project/files")
  assert.equal(url.searchParams.get("destination"), "/")
  assert.equal(url.searchParams.get("replace"), "true")
  requests[0].complete(200, { path: "/notes.txt" })
  await done
  assert.equal(uploads.items[0].path, "/notes.txt")
})

test("cancellation aborts the active file and does not send the remaining files", async () => {
  const { uploads, requests } = fixture()
  uploads.select(files({ name: "first", size: 5 }, { name: "second", size: 5 }), 100)
  const done = uploads.start(options)
  uploads.cancel()
  await done
  assert.equal(requests.length, 1)
  assert.equal(uploads.running, false)
  assert.ok(uploads.items.every((item) => item.status === "cancelled"))
})

test("session expiry stops the queue and oversized selection sends no requests", async () => {
  const f = fixture()
  assert.throws(() => f.uploads.select(files({ name: "big", size: 101 }), 100), /smaller/)
  f.uploads.select(files({ name: "first", size: 5 }, { name: "second", size: 5 }), 100)
  const done = f.uploads.start(options)
  f.requests[0].complete(401, { error: "Sign in required" })
  await done
  assert.equal(f.unauthorized, 1)
  assert.equal(f.requests.length, 1)
})
