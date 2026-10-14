/* eslint-disable @typescript-eslint/no-explicit-any -- the fakes stand in for Web Audio and WebSocket objects */
import { test } from "bun:test"
import assert from "node:assert/strict"

import { DesktopMedia } from "../web/src/lib/media"

function fixture() {
  const sockets: any[] = []
  const contexts: any[] = []
  const nodes: any[] = []
  const tracks: any[] = []
  const node = (): any => {
    const n = {
      port: { postMessage() {} },
      gain: {},
      connect() {},
      disconnect() {
        n.disconnected = true
      },
    } as any
    nodes.push(n)
    return n
  }
  const deps: any = {
    url: (s: string) => s,
    createContext() {
      const ctx: any = {
        sampleRate: 48000,
        audioWorklet: { async addModule() {} },
        async resume() {},
        async close() {
          ctx.closed = true
        },
        createMediaStreamSource: node,
        createGain: node,
      }
      contexts.push(ctx)
      return ctx
    },
    createNode: node,
    createSocket(url: string) {
      const socket: any = {
        url,
        readyState: 1,
        bufferedAmount: 0,
        close() {
          socket.closed = true
        },
        send() {},
      }
      sockets.push(socket)
      return socket
    },
    async getUserMedia() {
      const track: any = {
        stop() {
          track.stopped = true
        },
      }
      tracks.push(track)
      return { getTracks: () => [track] }
    },
  }
  const media = new DesktopMedia(deps)
  media.select("first")
  return { media, deps, sockets, contexts, nodes, tracks }
}

test("an old socket close cannot tear down a new desktop audio connection", async () => {
  const f = fixture()
  await f.media.toggleOutput()
  const oldClose = f.sockets[0].onclose
  f.media.select("second")
  await f.media.toggleOutput()
  oldClose()
  assert.equal(f.media.session!.output!.socket, f.sockets[1])
  assert.ok(!f.sockets[1].closed)
  assert.ok(f.sockets[0].closed && f.contexts[0].closed && f.nodes[0].disconnected)
})

test("microphone permission resolving after navigation stops tracks immediately", async () => {
  const f = fixture()
  let allow: (stream: unknown) => void = () => {}
  let requested: () => void = () => {}
  const pending = new Promise<void>((resolve) => {
    requested = resolve
  })
  f.deps.getUserMedia = () => {
    requested()
    return new Promise((resolve) => {
      allow = resolve
    })
  }
  const opening = f.media.toggleMic()
  await pending
  f.media.select("second")
  const track: any = {
    stop() {
      track.stopped = true
    },
  }
  allow({ getTracks: () => [track] })
  await opening
  assert.ok(track.stopped)
  assert.equal(f.sockets.length, 0)
  assert.ok(f.contexts[0].closed)
})

test("socket closure releases microphone, source, mute node and idle context", async () => {
  const f = fixture()
  await f.media.toggleMic()
  f.sockets[0].onclose()
  assert.ok(f.tracks[0].stopped && f.contexts[0].closed)
  assert.ok(f.nodes.every((n) => n.disconnected))
  assert.equal(f.media.session!.input, null)
})

test("turning output off keeps an active microphone and its shared context alive", async () => {
  const f = fixture()
  await Promise.all([f.media.toggleOutput(), f.media.toggleMic()])
  assert.equal(f.contexts.length, 1)
  f.media.stop("output")
  assert.ok(!f.contexts[0].closed && !f.tracks[0].stopped)
  f.media.close()
  assert.ok(f.contexts[0].closed && f.tracks[0].stopped)
})

test("failed worklet initialization can be retried without leaking its context", async () => {
  const f = fixture()
  const original = f.deps.createContext
  f.deps.createContext = () => {
    const ctx = original()
    ctx.audioWorklet.addModule = async () => {
      throw new Error("worklet failure")
    }
    return ctx
  }
  await assert.rejects(f.media.toggleOutput(), /worklet failure/)
  assert.ok(f.contexts[0].closed)
  f.deps.createContext = original
  await f.media.toggleOutput()
  assert.equal(f.sockets.length, 1)
})

test("microphone node setup failure releases an acquired stream", async () => {
  const f = fixture()
  f.deps.createNode = () => {
    throw new Error("node failure")
  }
  await assert.rejects(f.media.toggleMic(), /node failure/)
  assert.ok(f.tracks[0].stopped && f.contexts[0].closed && f.nodes[0].disconnected)
})
