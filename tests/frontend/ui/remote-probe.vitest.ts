/**
 * 「测试连接：界面把表单里（未保存的）那台配置交给本机后端，后端组拨号请求、拨一次」——
 * 界面那一侧：交过去的是表单那一台 ＋ 已保存的同名那一份 ＋ 跳板那一台（问 `<local>` 的 `remote-probe`）。
 * 〔「进度不许倒退」〕进度边拨边推：先订 `probe-progress/<票>` 再问；每一格（后端写好的那一句 ＋ 四段）边收边交 `onCell`；
 * 结局是进度流的最后一格（可以晚于应答到）。停在哪一步、等多久算没结果由后端判（结局那一格说）。
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

vi.mock("../../../src/comms/inward/chan", async (orig) => {
  const real = await orig<typeof import("../../../src/comms/inward/chan")>();
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

import { ChanError } from "../../../src/comms/inward/chan";
import { probeMachine, PROBE_PROGRESS_KIND } from "../../../src/frontend/ui/remote-probe";
import type { RemoteHostConfig } from "../../../src/frontend/ui/remote-config";
import type { ProbeCell } from "../../../src/frontend/ui/generated/ProbeCell";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/backend-policy";
import { copyText } from "../../../src/frontend/ui/copy-table";

const host = (over: Partial<RemoteHostConfig>): RemoteHostConfig => ({
  label: "devbox",
  host: "192.0.2.2",
  port: 22,
  user: "u",
  keyPath: "",
  addresses: [],
  jump: "",
  connect: true,
  ...over,
});
const segs = { resolve: "ok", connect: "ok", auth: "ok", ready: "ok" } as const;
const ok: ProbeCell = {
  said: copyText("beProbe.test.ok"),
  tone: "plain",
  segs,
  end: { backendHello: "版本 p5o · 能用 40 项、这台做不到 0 项 · 往返 3 毫秒", backendGaps: [], detail: null },
};
const dialing: ProbeCell = { said: "连接-甲", tone: "now", segs: { ...segs, resolve: "wait", connect: "wait", auth: "wait", ready: "wait" }, end: null };
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
  it("先订 `<local>` 的进度流再问 remote-probe：表单那一台 ＋ 已保存的同名那一份 ＋ 跳板那一台 ＋ 同一张票；每一格边收边交", async () => {
    let body: unknown = null;
    const drawn: ProbeCell[] = [];
    fake.call = (origin, op, b) => {
      body = { origin, op, args: JSON.parse(new TextDecoder().decode(b)) };
      expect(drawn, "应答之前的那几格此刻就该画出来了").toEqual([]);
      push(dialing);
      expect(drawn).toEqual([dialing]);
      push(ok);
      return Promise.resolve(new TextEncoder().encode("null"));
    };
    const form = host({ host: "192.0.2.3", jump: "bastion" });
    const saved = [host({ label: "other" }), host({ label: "bastion", host: "b.example" }), host({ keyPath: "/k/old" })];
    const r = await probeMachine(form, saved, (c) => drawn.push(c));
    expect(fake.order).toEqual(["subscribe", "call"]);
    const sub = fake.subs[0]!;
    expect(sub.origin).toBe(LOCAL_ORIGIN);
    const ticket = sub.kind.slice(`${PROBE_PROGRESS_KIND}/`.length);
    expect(sub.kind).toBe(`${PROBE_PROGRESS_KIND}/${ticket}`);
    expect(body).toEqual({
      origin: LOCAL_ORIGIN,
      op: "remote-probe",
      args: { ticket, machine: form, jump: saved[1] },
    });
    expect(r).toEqual(ok);
    expect(sub.stopped, "结局到了就撤订").toBe(true);
  });

  it("结局那一格晚于应答到 ⇒ 等它", async () => {
    fake.call = () => {
      setTimeout(() => push(ok), 0);
      return Promise.resolve(new TextEncoder().encode("null"));
    };
    await expect(probeMachine(host({}), [], () => {})).resolves.toEqual(ok);
  });

  it("通道没成（本机后端不在 · 到点没回）⇒ 抛那一句人话", async () => {
    fake.call = () => {
      push(dialing);
      return Promise.reject(overrun());
    };
    const err = await probeMachine(host({}), [], () => {}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(Error);
    expect(fake.subs[0]!.stopped).toBe(true);
  });

  it("流在结局之前关了 / 丢了格 ⇒ 抛（说不清停在哪）", async () => {
    fake.call = () => {
      fake.subs[0]!.sink([{ t: "gap", fromSeq: 0, toSeq: 1 }]);
      return Promise.resolve(new TextEncoder().encode("null"));
    };
    await expect(probeMachine(host({}), [], () => {})).rejects.toThrow();
  });

  it("进度流名与 Rust `event_replay.rs::PROBE_PROGRESS_KIND` 同一个串", () => {
    const rs = readFileSync(resolve(__dirname, "../../..", "src/frontend/shell/src/event_replay.rs"), "utf8");
    expect(/pub const PROBE_PROGRESS_KIND: &str = "([^"]+)";/.exec(rs)?.[1]).toBe(PROBE_PROGRESS_KIND);
  });
});
