/**
 * 设计/99 §2.1 ⑬ 主会话裁「`test_remote_connection`：界面把表单里（未保存的）那台配置交给本机后端，后端组拨号请求、拨一次」——
 * 界面那一侧：交过去的是表单那一台 ＋ 已保存的同名那一份 ＋ 跳板那一台（问 `<local>` 的 `remote-probe`）；结局按形状严格收。
 * 〔MIG-1 收尾 · 主会话裁「进度不许倒退」〕进度边拨边推：先订 `probe-progress/<票>` 再问；握手那几行边收边交 `onStage`；
 * 结局是进度流的最后一格（可以晚于应答到）；到点没等到结局 ⇒ `ProbeStalled` 说出停在哪一段（最后收到的那一格）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

type Sink = (items: unknown[]) => void;
const fake = vi.hoisted(() => ({
  subs: [] as { origin: string; kind: string; sink: (items: unknown[]) => void; stopped: boolean }[],
  call: null as null | ((origin: string, op: string, body: Uint8Array) => Promise<Uint8Array>),
  order: [] as string[],
}));

vi.mock("../src/ipc/chan", async (orig) => {
  const real = await orig<typeof import("../src/ipc/chan")>();
  return {
    ...real,
    chan: {
      subscribe: vi.fn((origin: string, kind: string, _from: unknown, _want: number, sink: Sink) => {
        fake.order.push("subscribe");
        const rec = { origin, kind, sink, stopped: false };
        fake.subs.push(rec);
        return Promise.resolve({ want: () => {}, stop: () => (rec.stopped = true) });
      }),
      call: vi.fn((origin: string, op: string, body: Uint8Array) => {
        fake.order.push("call");
        return fake.call!(origin, op, body);
      }),
    },
  };
});

import { ChanError } from "../src/ipc/chan";
import { decodeCell, decodeProbe, probeMachine, ProbeStalled, PROBE_PROGRESS_KIND } from "../src/remote-probe";
import type { RemoteHostConfig } from "../src/remote-config";
import type { ConnectStage } from "../src/generated/ConnectStage";
import { LOCAL_ORIGIN } from "../src/backend-policy";

const host = (over: Partial<RemoteHostConfig>): RemoteHostConfig => ({
  label: "aya",
  host: "10.0.0.2",
  port: 22,
  user: "u",
  keyPath: "",
  hostKeyFingerprint: "",
  addresses: [],
  jump: "",
  resumeCommand: "",
  ...over,
});
const ok = {
  sshOk: true,
  fingerprint: "SHA256:k",
  endpoint: "10.0.0.2:22",
  backendOk: true,
  backendHello: "v=1 control=ok(3ms)",
  message: "SSH 与后端均正常。",
};
const dialing = { kind: "dialing", endpoint: "10.0.0.2:22" };
let seq = 0;
const cell = (c: unknown): unknown => ({ t: "frame", seq: seq++, body: JSON.stringify(c) });
const push = (...cells: unknown[]): void => fake.subs[0]!.sink(cells.map(cell));
const overrun = (): ChanError =>
  new ChanError({ layer: "hop", at: { idx: 1, tag: "wait" }, reach: "Sent", why: "Overrun" });

beforeEach(() => {
  fake.subs.length = 0;
  fake.order.length = 0;
  fake.call = null;
  seq = 0;
});

describe("请求与进度", () => {
  it("先订 `<local>` 的进度流再问 remote-probe：表单那一台 ＋ 已保存的同名那一份 ＋ 跳板那一台 ＋ 同一张票；阶段行边收边交", async () => {
    let body: unknown = null;
    const drawn: ConnectStage[] = [];
    fake.call = (origin, op, b) => {
      body = { origin, op, args: JSON.parse(new TextDecoder().decode(b)) };
      expect(drawn, "应答之前的阶段行此刻就该画出来了").toEqual([]);
      push({ stage: dialing });
      expect(drawn).toEqual([dialing]);
      push({ reached: "ssh" }, { reached: "hello" }, { reached: "control" }, { end: ok });
      return Promise.resolve(new TextEncoder().encode("null"));
    };
    const form = host({ host: "10.0.0.3", jump: "bastion" });
    const saved = [host({ label: "other" }), host({ label: "bastion", host: "b.example" }), host({ hostKeyFingerprint: "SHA256:old" })];
    const r = await probeMachine(form, saved, (st) => drawn.push(st));
    expect(fake.order).toEqual(["subscribe", "call"]);
    const sub = fake.subs[0]!;
    expect(sub.origin).toBe(LOCAL_ORIGIN);
    const ticket = sub.kind.slice(`${PROBE_PROGRESS_KIND}/`.length);
    expect(sub.kind).toBe(`${PROBE_PROGRESS_KIND}/${ticket}`);
    expect(body).toEqual({
      origin: LOCAL_ORIGIN,
      op: "remote-probe",
      args: { ticket, machine: form, saved: saved[2], jump: saved[1] },
    });
    expect(r).toEqual(ok);
    expect(sub.stopped, "结局到了就撤订").toBe(true);
  });

  it("结局那一格晚于应答到 ⇒ 等它", async () => {
    fake.call = () => {
      setTimeout(() => push({ end: ok }), 0);
      return Promise.resolve(new TextEncoder().encode("null"));
    };
    await expect(probeMachine(host({}), [], () => {})).resolves.toEqual(ok);
  });

  it.each([
    ["一格都没收到", [] as unknown[], { at: "start" }],
    ["握手中", [{ stage: dialing }], { at: "handshake", last: dialing }],
    ["握手过了、等 hello", [{ stage: dialing }, { reached: "ssh" }], { at: "hello" }],
    ["hello 到了、等 ping", [{ reached: "ssh" }, { reached: "hello" }], { at: "control" }],
  ])("到点没等到结局（%s）⇒ ProbeStalled 说出停在哪一段", async (_n, cells, stop) => {
    fake.call = () => {
      if (cells.length) push(...cells);
      return Promise.reject(overrun());
    };
    const err = await probeMachine(host({}), [], () => {}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ProbeStalled);
    expect((err as ProbeStalled).stop).toEqual(stop);
    expect(fake.subs[0]!.stopped).toBe(true);
  });

  it("本机后端不在 ⇒ 抛一句人话（不是 ProbeStalled）", async () => {
    fake.call = () =>
      Promise.reject(new ChanError({ layer: "hop", at: { idx: 0, tag: "open" }, reach: "NotSent", why: "Unreachable" }));
    const err = await probeMachine(host({}), [], () => {}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(Error);
    expect(err).not.toBeInstanceOf(ProbeStalled);
  });

  it("流在结局之前关了 / 丢了格 ⇒ 抛（说不清停在哪）", async () => {
    fake.call = () => {
      fake.subs[0]!.sink([{ t: "gap", fromSeq: 0, toSeq: 1 }]);
      return Promise.resolve(new TextEncoder().encode("null"));
    };
    await expect(probeMachine(host({}), [], () => {})).rejects.toThrow();
  });

  it("进度流名与 Rust `event_replay.rs::PROBE_PROGRESS_KIND` 同一个串", () => {
    const rs = readFileSync(resolve(__dirname, "..", "src/bridge/src/event_replay.rs"), "utf8");
    expect(/pub const PROBE_PROGRESS_KIND: &str = "([^"]+)";/.exec(rs)?.[1]).toBe(PROBE_PROGRESS_KIND);
  });
});

describe("严格收", () => {
  it.each([
    ["多一格", { ...ok, extra: 1 }],
    ["还带阶段行（阶段行改走进度流）", { ...ok, stages: [] }],
    ["sshOk 不是布尔", { ...ok, sshOk: "yes" }],
  ])("结局 %s ⇒ 抛", (_n, v) => {
    expect(() => decodeProbe(JSON.parse(JSON.stringify(v)))).toThrow();
  });
  it.each([
    ["两个键", { stage: dialing, reached: "ssh" }],
    ["阶段行不带 kind", { stage: {} }],
    ["认不得的段", { reached: "tmux" }],
    ["认不得的键", { progress: 1 }],
  ])("进度格 %s ⇒ 抛", (_n, v) => {
    expect(() => decodeCell(v)).toThrow();
  });
});
