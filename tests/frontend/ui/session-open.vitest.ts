/**
 * 起会话一个家（`session-open.ts`）：照 `landed` 开不开那一行；回包带 `arrival` ⇒ 问那台等报到（`session-arrive`），界面不自己认、不计时；
 * 失败按码挑按钮、那台那一句原样上屏；开窗失败 ⇒ 那一行进剪贴板；同一条在飞再点不重发。替身：通道 · 开窗 · 剪贴板 · 提示 · 对齐重试。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root";

const sent: { origin: string; op: string; body: Record<string, unknown> }[] = [];
let reply: unknown = null;
/** `session-arrive` 的回话。 */
let arriveReply: unknown = { kind: "arrived", sid: "s-1" };
let refusal: { code: string; message: string; data: unknown } | null = null;
/** 有值 ⇒ 那一问挂着、直到判据放行。 */
let gate: Promise<void> | null = null;

vi.mock("../../../src/comms/inward/chan", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../../src/comms/inward/chan")>();
  return {
    ...real,
    chan: {
      call: async (origin: string, op: string, body: Uint8Array) => {
        sent.push({ origin, op, body: JSON.parse(new TextDecoder().decode(body)) });
        if (gate) await gate;
        if (refusal) {
          const r = refusal;
          throw new real.ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify(r)) }, r.message, "detail-x");
        }
        return new TextEncoder().encode(JSON.stringify(op === "session-arrive" ? arriveReply : reply));
      },
    },
  };
});
const term = vi.hoisted(() => ({ openTerminal: vi.fn(), openLocalTerminal: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-open", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/terminal-open")>()),
  openTerminal: term.openTerminal,
  openLocalTerminal: term.openLocalTerminal,
}));
const clip = vi.hoisted(() => ({ writeClipboard: vi.fn() }));
vi.mock("../../../src/frontend/ui/clipboard", () => clip);
vi.mock("../../../src/frontend/ui/settings/open-settings", () => ({ openSettingsWindow: vi.fn() }));
const toasts = vi.hoisted(() => ({ toast: vi.fn(), failToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/toast", () => toasts);
const resync = vi.hoisted(() => ({ offerResyncRetry: vi.fn() }));
vi.mock("../../../src/frontend/ui/resync", () => resync);

import { openSession, setLaunchSlots } from "../../../src/frontend/ui/session-open";
import type { ArriveAsk } from "../../../src/frontend/ui/generated/ArriveAsk";
import { NoTerminalWindow } from "../../../src/frontend/ui/terminal-open";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Landed } from "../../../src/frontend/ui/generated/Landed";
import type { Opened } from "../../../src/frontend/ui/generated/Opened";
import type { SessionOpenReq } from "../../../src/frontend/ui/generated/SessionOpenReq";

const RESUME: SessionOpenReq = { what: { kind: "resume", sid: "s-1" }, local: false };

const ASK: ArriveAsk = { by: { kind: "sid", sid: "s-1" }, since: 4, session: "t-1" };
const LANDS: Landed[] = ["window", "started", "in_place", "running", "idle"];

/** 那台的成品：起了一家的（window · started · in_place）带等报到那一问，接上已有的不带（那台定，见 `session_open_tests.rs`）。 */
function opened(landed: Landed, over: Partial<Opened> = {}): Opened {
  const starts = landed === "window" || landed === "started" || landed === "in_place";
  return { landed, session: "t-1", line: `line:${landed}`, sid: "s-1", agent: "claude", cwd: "/p", account: null, note: null, prompt: null, arrival: starts ? ASK : null, ...over };
}

const arrives = (): Record<string, unknown>[] => sent.filter((s) => s.op === "session-arrive").map((s) => s.body);

/** 那台码表（生成的命令表里 `session-open` 那一行的 `code`）。 */
function openCodes(): string[] {
  const src = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/generated/TypedCommands.ts"), "utf8");
  const row = src.split("\n").find((l) => l.includes('"session-open": {'));
  if (!row) throw new Error("生成的命令表里没有 session-open");
  const code = row.slice(row.indexOf("code:"));
  return [...code.matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeEach(() => {
  vi.clearAllMocks();
  setLaunchSlots(null);
  sent.length = 0;
  reply = null;
  arriveReply = { kind: "arrived", sid: "s-1" };
  refusal = null;
  gate = null;
  term.openTerminal.mockResolvedValue(undefined);
  term.openLocalTerminal.mockResolvedValue(undefined);
  clip.writeClipboard.mockResolvedValue(undefined);
});

describe("照 landed 开不开 · 报到交那台等", () => {
  it("★ 单个：每一种都开那一行；带等报到那一问的原样交 `session-arrive`，接上已有的当场说接上（整表两向相等）", async () => {
    const got: Record<string, { opened: boolean; asked: boolean; said: boolean }> = {};
    for (const landed of LANDS) {
      vi.clearAllMocks();
      sent.length = 0;
      reply = opened(landed);
      expect(await openSession("devbox", RESUME)).toBe(true);
      await flush();
      got[landed] = {
        opened: term.openTerminal.mock.calls.length === 1 && term.openTerminal.mock.calls[0][1] === `line:${landed}`,
        asked: JSON.stringify(arrives()) === JSON.stringify([ASK]),
        said: toasts.toast.mock.calls.length === 1,
      };
    }
    expect(got).toEqual({
      window: { opened: true, asked: true, said: true },
      started: { opened: true, asked: true, said: true },
      in_place: { opened: true, asked: true, said: true },
      running: { opened: true, asked: false, said: true },
      idle: { opened: true, asked: false, said: true },
    });
  });

  it("★ 后台起的（新起）：只有 window 开", async () => {
    const opens: Record<string, boolean> = {};
    for (const landed of LANDS) {
      vi.clearAllMocks();
      reply = opened(landed);
      await openSession("devbox", { what: { kind: "new", agent: "claude", cwd: "/p" }, local: false });
      opens[landed] = term.openTerminal.mock.calls.length === 1;
    }
    expect(opens).toEqual({ window: true, started: false, in_place: false, running: false, idle: false });
  });

  it("★ 那台说没见到 ⇒ 那一句原样上屏（标题 ＋ 正文）；界面不认、不计时：起了之后零个定时器", async () => {
    vi.useFakeTimers();
    try {
      arriveReply = { kind: "missed", said: "命令已发出\n已等 45s 未报到 · 末几行\nError: x", terminal: "h-1" };
      reply = opened("started");
      const done = openSession("devbox", RESUME);
      await vi.advanceTimersByTimeAsync(0);
      await done;
      expect(vi.getTimerCount()).toBe(0);
      await vi.advanceTimersByTimeAsync(0);
      const t = toasts.toast.mock.calls.at(-1)!;
      expect([t[0], t[1], t[2]?.level]).toEqual(["命令已发出", "已等 45s 未报到 · 末几行\nError: x", "error"]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("报到了 ⇒ 说已启动；发起方要自己说的交它那个 sid", async () => {
    reply = opened("started");
    await openSession("devbox", RESUME);
    await flush();
    expect(toasts.toast.mock.calls.at(-1)?.[0]).toBe(copyText("sessionOpen.arrived.title"));
    vi.clearAllMocks();
    const onArrived = vi.fn();
    arriveReply = { kind: "arrived", sid: "s-9" };
    await openSession("devbox", RESUME, { onArrived });
    await flush();
    expect(onArrived.mock.calls[0][0]).toBe("s-9");
    expect(toasts.toast).not.toHaveBeenCalled();
  });

  it("主窗口里起的（长得出占位标签页）⇒ 交它（起在哪个目录 · 那一问原样），这里不问；本机开窗带起始目录", async () => {
    const slot = vi.fn(() => true);
    setLaunchSlots(slot);
    reply = opened("window");
    await openSession("<local>", { ...RESUME, local: true });
    await flush();
    expect(term.openLocalTerminal).toHaveBeenCalledWith("line:window", "/p");
    expect(slot.mock.calls[0]).toEqual([{ origin: "<local>", cwd: "/p", tmuxName: "t-1", arrival: ASK }]);
    expect(arrives()).toEqual([]);
  });

  it("恢复成了 ⇒ 那台附带的一句照说", async () => {
    reply = opened("running", { note: "附带的一句" });
    await openSession("devbox", RESUME);
    expect(toasts.toast.mock.calls.map((c) => c[0])).toContain("附带的一句");
  });
});

describe("失败按码挑按钮，那一句是那台的原话", () => {
  it("★ 每个码：只有选不了号 · 对齐重试两种带按钮；那一句原样上屏（标题 ＋ 正文 ＝ 那一句）", async () => {
    const withButton = new Set<string>();
    for (const code of openCodes()) {
      vi.clearAllMocks();
      refusal = { code, message: `第一行 ${code}\n第二行`, data: { field: null, unavailable: code === "account_unavailable" ? { requested: "z", pinned: false, listKnown: true, alternative: "b" } : null } };
      expect(await openSession("devbox", RESUME)).toBe(false);
      expect(term.openTerminal).not.toHaveBeenCalled();
      const t = toasts.toast.mock.calls[0] ?? resync.offerResyncRetry.mock.calls[0]?.slice(2);
      if (code === "wrong_owner") {
        withButton.add(code);
        const [, sid, head, said] = resync.offerResyncRetry.mock.calls[0];
        expect([sid, `${head}\n${said}`]).toEqual(["s-1", `第一行 ${code}\n第二行`]);
        continue;
      }
      expect(`${t[0]}\n${t[1]}`).toBe(`第一行 ${code}\n第二行`);
      if (t[2]?.action) withButton.add(code);
    }
    expect(withButton).toEqual(new Set(["account_unavailable", "wrong_owner"]));
  });

  it("选不了号：点按钮 ⇒ 点名替代号再问一次；没有替代 / 清单读不出 ⇒ 按账号 0 再问", async () => {
    for (const [u, want] of [
      [{ requested: "z", pinned: true, listKnown: true, alternative: "b" }, { kind: "named", name: "b" }],
      [{ requested: "z", pinned: true, listKnown: true, alternative: null }, { kind: "base" }],
      [{ requested: "z", pinned: false, listKnown: false, alternative: "b" }, { kind: "base" }],
    ] as const) {
      vi.clearAllMocks();
      sent.length = 0;
      refusal = { code: "account_unavailable", message: "选不了", data: { field: null, unavailable: u } };
      await openSession("devbox", RESUME);
      reply = opened("started");
      toasts.toast.mock.calls[0][2].action.run();
      await flush();
      expect(sent.map((s) => s.body.account)).toEqual([undefined, want]);
    }
  });

  it("记录已不在 ⇒ 零次开窗（标签页的「记录已不在」由那台账本重发一帧脸）", async () => {
    refusal = { code: "record_gone", message: "不在", data: { field: null, unavailable: null } };
    await openSession("devbox", RESUME);
    expect(term.openTerminal).not.toHaveBeenCalled();
  });
});

describe("开窗失败 · 在飞", () => {
  it("★ 开窗失败 ⇒ 剪贴板里是那一行", async () => {
    reply = opened("started");
    term.openTerminal.mockRejectedValue(new Error("拉不起"));
    expect(await openSession("devbox", RESUME)).toBe(false);
    expect(clip.writeClipboard).toHaveBeenCalledWith("line:started");
    expect(arrives()).toEqual([]);
  });

  it("这台找不到终端（按壳回的结局判）⇒ 照实说 ＋［设置］［复制命令］，不自动写剪贴板；只是话里带着那一句的真失败 ⇒ 照样复制", async () => {
    reply = opened("window");
    term.openLocalTerminal.mockRejectedValueOnce(new NoTerminalWindow("line:window", "setMissing"));
    await openSession("<local>", { ...RESUME, local: true });
    expect(clip.writeClipboard, "不许自动写剪贴板").not.toHaveBeenCalled();
    const [title, , opts] = toasts.toast.mock.calls[0] as [string, string, { action?: { label: string; run: () => void }[] }];
    expect(title).toBe(copyText("terminalOpen.setMissing.title"));
    expect((opts.action ?? []).map((x) => x.label)).toEqual([copyText("terminalOpen.noTerminal.settings"), copyText("terminalOpen.noTerminal.copy")]);
    vi.clearAllMocks();
    clip.writeClipboard.mockResolvedValue(undefined);
    term.openLocalTerminal.mockRejectedValueOnce(new Error(copyText("rsLaunch.posix.noTerminalWindow")));
    await openSession("<local>", { ...RESUME, local: true });
    expect(clip.writeClipboard).toHaveBeenCalledWith("line:window");
  });

  it("★ 同一条在飞再点 ⇒ 零次第二问；别的那台同一个 sid 照问", async () => {
    let open: () => void = () => {};
    gate = new Promise<void>((r) => (open = r));
    reply = opened("started");
    const first = openSession("devbox", RESUME);
    const again = openSession("devbox", RESUME);
    const other = openSession("lab", RESUME);
    await flush();
    open();
    expect(await again).toBe(false);
    await Promise.all([first, other]);
    expect(sent.filter((s) => s.op === "session-open").map((s) => s.origin)).toEqual(["devbox", "lab"]);
  });
});
