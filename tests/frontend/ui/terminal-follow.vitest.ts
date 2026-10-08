/**
 * 终端实时预览的界面那一半（`src/frontend/ui/terminal-follow.ts`）：订 → 收一屏就交 → 画完回执（两次回执至少隔 100 ms）→ 退订。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 先订流 `terminal-screen/<票>`、再发 `terminal-follow {terminal, ticket}`（同一张票），流名与壳那一侧同一个串 | 「订上」 |
 * | 收一屏 ⇒ 交给页面（与抓一屏同一份成品），再回执那一帧的序号；两次回执至少隔 100 ms，回执时补一格 credit | 「回执」 |
 * | 退订：撤流、发 `terminal-unfollow`；之后来的帧不交、排着的回执不发 | 「退订」 |
 * | 停了的几种：后端说停（gone · lost · too_big）· 流断了 · 订不上（tmux 旧 · 后端旧 · 没 tmux ⇒ 只能快照；别的 ⇒ 停了带原话） | 「停」 |
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { ChanError, type Item } from "../../../src/comms/inward/chan";
import { startFollow, TERMINAL_SCREEN_KIND, ACK_GAP_MS, type FollowPort, type FollowEvents, type FollowStop } from "../../../src/frontend/ui/terminal-follow";
import type { TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import { REPO_ROOT } from "../../test-support/repo-root";

const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/terminals.golden.json"), "utf8")) as Record<string, { reply: Record<string, unknown> }>;
const VIEW = golden["terminal-preview"].reply;

const enc = (v: unknown): Uint8Array => new TextEncoder().encode(JSON.stringify(v));
const refused = (code: string): ChanError => new ChanError({ layer: "peer", why: "refused", body: enc({ code, message: `said ${code}` }) });

interface Rig {
  port: FollowPort;
  calls: [string, string, unknown][];
  subs: { origin: string; kind: string; want: number; sink: (items: Item[]) => void; wants: number[]; stopped: boolean }[];
  screens: TerminalShot[];
  stops: FollowStop[];
  events: FollowEvents;
  failFollow: (e: unknown) => void;
}

function rig(): Rig {
  const calls: [string, string, unknown][] = [];
  const subs: Rig["subs"] = [];
  let followFail: unknown = null;
  const port: FollowPort = {
    subscribe: async (origin, kind, want, sink) => {
      const s = { origin, kind, want, sink, wants: [] as number[], stopped: false };
      subs.push(s);
      return { want: (n: number) => void s.wants.push(n), stop: () => void (s.stopped = true) };
    },
    follow: async (origin, body) => {
      calls.push([origin, "terminal-follow", body]);
      if (followFail !== null) throw followFail;
      return enc({});
    },
    ack: async (origin, body) => void calls.push([origin, "terminal-follow-ack", body]),
    unfollow: async (origin, body) => void calls.push([origin, "terminal-unfollow", body]),
  };
  const screens: TerminalShot[] = [];
  const stops: FollowStop[] = [];
  return { port, calls, subs, screens, stops, events: { screen: (s) => void screens.push(s), stop: (w) => void stops.push(w) }, failFollow: (e) => (followFail = e) };
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};
const frame = (seq: number, body: unknown): Item => ({ t: "frame", seq, body: JSON.stringify(body) });
const acks = (r: Rig): unknown[] => r.calls.filter(([, op]) => op === "terminal-follow-ack").map(([, , b]) => b);

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("订上", () => {
  it("★★ 先订流、再发订阅命令，同一张票；流名是 terminal-screen/<票>", async () => {
    const r = rig();
    startFollow("devbox", "tmux-1", r.events, r.port);
    await flush();
    expect(r.subs).toHaveLength(1);
    const ticket = r.subs[0].kind.slice(`${TERMINAL_SCREEN_KIND}/`.length);
    expect(r.subs[0].kind).toBe(`terminal-screen/${ticket}`);
    expect(ticket).not.toBe("");
    expect(r.subs[0].origin).toBe("devbox");
    expect(r.calls).toEqual([["devbox", "terminal-follow", { terminal: "tmux-1", ticket }]]);
    // 两张订阅票不同。
    startFollow("devbox", "tmux-1", r.events, r.port);
    await flush();
    expect(r.subs[1].kind).not.toBe(r.subs[0].kind);
  });
});

describe("两侧同一个串", () => {
  it("★ 流名与壳 `event_replay.rs::TERMINAL_SCREEN_KIND` 逐字相同", () => {
    const rs = readFileSync(resolve(REPO_ROOT, "src/frontend/shell/src/event_replay.rs"), "utf8");
    expect(rs).toContain(`pub const TERMINAL_SCREEN_KIND: &str = "${TERMINAL_SCREEN_KIND}";`);
  });
});

describe("回执", () => {
  it("★★ 收一屏 ⇒ 交给页面，再回执那一帧；两次回执至少隔 100 ms；回执时补一格 credit", async () => {
    const r = rig();
    startFollow("devbox", "tmux-1", r.events, r.port);
    await flush();
    const ticket = r.subs[0].kind.split("/")[1];
    r.subs[0].sink([frame(0, { seq: 1, view: VIEW })]);
    expect(r.screens).toHaveLength(1);
    expect(r.screens[0].lines[0].spans).toEqual([{ from: 0, to: 4, fg: "red", bold: true }]);
    expect(r.screens[0].atText).toBe(VIEW.captured_at_text);
    await flush();
    expect(acks(r)).toEqual([{ ticket, seq: 1 }]);
    expect(r.subs[0].wants).toEqual([1]);
    // 紧跟着又来一帧：离上次回执不到 100 ms ⇒ 先不回。
    r.subs[0].sink([frame(1, { seq: 2, view: VIEW })]);
    expect(r.screens).toHaveLength(2);
    await flush();
    expect(acks(r)).toHaveLength(1);
    vi.advanceTimersByTime(ACK_GAP_MS - 1);
    await flush();
    expect(acks(r)).toHaveLength(1);
    vi.advanceTimersByTime(1);
    await flush();
    expect(acks(r)).toEqual([{ ticket, seq: 1 }, { ticket, seq: 2 }]);
    expect(ACK_GAP_MS).toBe(100);
  });
});

describe("退订", () => {
  it("★ 撤流、发退订；之后来的帧不交、排着的回执不发", async () => {
    const r = rig();
    const f = startFollow("devbox", "tmux-1", r.events, r.port);
    await flush();
    const ticket = r.subs[0].kind.split("/")[1];
    r.subs[0].sink([frame(0, { seq: 1, view: VIEW })]);
    await flush();
    r.subs[0].sink([frame(1, { seq: 2, view: VIEW })]); // 回执排在 100 ms 后
    f.stop();
    await flush();
    expect(r.subs[0].stopped).toBe(true);
    expect(r.calls.filter(([, op]) => op === "terminal-unfollow")).toEqual([["devbox", "terminal-unfollow", { ticket }]]);
    vi.advanceTimersByTime(1000);
    await flush();
    expect(acks(r)).toEqual([{ ticket, seq: 1 }]);
    r.subs[0].sink([frame(2, { seq: 3, view: VIEW })]);
    expect(r.screens).toHaveLength(2);
    expect(r.stops, "自己退订不算「停了」").toEqual([]);
    f.stop(); // 幂等
    expect(r.calls.filter(([, op]) => op === "terminal-unfollow")).toHaveLength(1);
  });

  it("订阅命令还在路上就退订 ⇒ 照样撤流、发退订，回来的成功不再交任何东西", async () => {
    const r = rig();
    const f = startFollow("devbox", "tmux-1", r.events, r.port);
    f.stop();
    await flush();
    expect(r.subs.every((s) => s.stopped)).toBe(true);
    expect(r.calls.some(([, op]) => op === "terminal-unfollow")).toBe(true);
    expect(r.stops).toEqual([]);
  });
});

describe("停", () => {
  it("★★ 后端说停：gone · lost · too_big 各成一种；流随之撤、不再发退订", async () => {
    for (const [why, want] of [
      ["gone", { kind: "stopped", why: "gone" }],
      ["lost", { kind: "stopped", why: "lost" }],
      ["too_big", { kind: "stopped", why: "tooBig" }],
    ] as const) {
      const r = rig();
      startFollow("devbox", "tmux-1", r.events, r.port);
      await flush();
      r.subs[0].sink([frame(0, { end: why })]);
      await flush();
      expect(r.stops).toEqual([want]);
      expect(r.subs[0].stopped).toBe(true);
      expect(r.calls.some(([, op]) => op === "terminal-unfollow"), "后端已经忘了这张票").toBe(false);
    }
  });

  it("★ 流断了（gap · closed · 那台看不见了）⇒ 停了（那台断开）；读不懂的一格 ⇒ 停了（画面中断）", async () => {
    for (const [item, want] of [
      [{ t: "closed", by: { ours: "Broken" } }, { kind: "stopped", why: "offline" }],
      [{ t: "gap", fromSeq: 0, toSeq: 1 }, { kind: "stopped", why: "offline" }],
      [{ t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" }, { kind: "stopped", why: "offline" }],
      [frame(0, { seq: 1, view: { lines: "x" } }), { kind: "stopped", why: "lost" }],
    ] as const) {
      const r = rig();
      startFollow("devbox", "tmux-1", r.events, r.port);
      await flush();
      r.subs[0].sink([item as Item]);
      await flush();
      expect(r.stops).toEqual([want]);
      expect(r.subs[0].stopped).toBe(true);
    }
  });

  it("★★ 订不上：tmux 旧 · 后端旧 · 没 tmux ⇒ 只能快照（带原因）；终端不在 ⇒ 停了（不在）；通道不通 ⇒ 停了（那台断开）", async () => {
    const cases: [unknown, FollowStop][] = [
      [refused("tmux_too_old"), { kind: "snapshotOnly", why: "tmux" }],
      [new ChanError({ layer: "peer", why: "unsupported" }), { kind: "snapshotOnly", why: "old" }],
      [refused("no_tmux"), { kind: "snapshotOnly", why: "noTmux" }],
      [refused("not_known"), { kind: "stopped", why: "gone" }],
      [new ChanError({ layer: "hop", at: { idx: 1, tag: "open" }, reach: "NotSent", why: "Unreachable" }), { kind: "stopped", why: "offline" }],
      [refused("too_many_follows"), { kind: "stopped", why: "lost" }],
    ];
    for (const [err, want] of cases) {
      const r = rig();
      r.failFollow(err);
      startFollow("devbox", "tmux-1", r.events, r.port);
      await flush();
      expect(r.stops, JSON.stringify(want)).toEqual([want]);
      expect(r.subs[0].stopped).toBe(true);
    }
  });
});
