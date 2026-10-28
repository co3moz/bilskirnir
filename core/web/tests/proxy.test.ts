import { test } from "bun:test"
import assert from "node:assert/strict"

import { parseBypass, PROXY_URL_PATTERN, proxyBody, proxyDraft } from "../src/lib/proxy"
import type { Settings } from "../src/lib/types"

const withProxy = {
  proxy: { url: "http://proxy.example:3128", username: null, bypass: [] },
} as unknown as Settings

test("a new container follows the installation's proxy where there is one", () => {
  assert.equal(proxyDraft({}, withProxy).mode, "installation")
  assert.equal(proxyDraft({}, {} as Settings).mode, "none")
  // An existing container keeps what it has, also none while the installation has a proxy.
  assert.equal(proxyDraft({ proxy: null }, withProxy).mode, "none")
  const own = proxyDraft(
    {
      proxy: {
        mode: "custom",
        url: "socks5://own.example:1080",
        username: "ana",
        bypass: ["intranet.example", "10.0.0.0/8"],
        password: true,
      },
    },
    withProxy
  )
  assert.deepEqual(own, {
    mode: "custom",
    url: "socks5://own.example:1080",
    username: "ana",
    bypass: "intranet.example, 10.0.0.0/8",
    password: "",
    stored: true,
    clear: false,
  })
})

test("the password is sent only to set a new one or to remove the kept one", () => {
  const draft = proxyDraft({
    proxy: { mode: "custom", url: "http://p.example:1", bypass: [], password: true },
  })
  assert.deepEqual(proxyBody({ ...draft, bypass: "a.example,  b.example 10.0.0.0/8" }), {
    proxy: {
      mode: "custom",
      url: "http://p.example:1",
      username: null,
      bypass: ["a.example", "b.example", "10.0.0.0/8"],
    },
  })
  assert.equal(proxyBody({ ...draft, password: "new" }).proxy_password, "new")
  assert.equal(proxyBody({ ...draft, clear: true }).proxy_password, null)
  assert.deepEqual(proxyBody({ ...draft, mode: "none", password: "ignored" }), { proxy: null })
  assert.deepEqual(proxyBody({ ...draft, mode: "installation" }), {
    proxy: { mode: "installation" },
  })
  assert.deepEqual(parseBypass(" , \n"), [])
})

test("the address pattern takes what the manager takes and the browser can compile it", () => {
  const pattern = new RegExp(`^(?:${PROXY_URL_PATTERN})$`, "v")
  for (const url of [
    "http://proxy.example:3128",
    "HTTPS://proxy.example:443/",
    "socks5://[fd00::1]:1080",
  ]) {
    assert.ok(pattern.test(url), url)
  }
  for (const url of [
    "proxy.example:3128",
    "ftp://proxy.example:21",
    "http://proxy.example",
    "http://ana:secret@proxy.example:3128",
    "http://proxy.example:3128/path",
  ]) {
    assert.ok(!pattern.test(url), url)
  }
})
