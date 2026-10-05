/**
 * 分叉之后起会话的**接线**（`runForkFlow`）：推断住那台后端（`session-fork` 回复的 `launch`），这里只核接线 ——
 * 交那台 `sessions-start` 的那一项（新 sid · 带 `fresh_terminal` 与源会话 `fork_of`）、开窗 / 接回、等到才说「已分叉」、号选不了给显式选择。
 * 后端答什么由替身给（`chan_call` 那一跳），tmux / 会话一个都不起。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: () => ({ resumeCommandLocal: "", resumeCommandRemote: "" }),
}));
vi.mock("../../../src/frontend/ui/remote-config", () => ({ resolveResumeCommand: vi.fn().mockResolvedValue("") }));
vi.mock("../../../src/frontend/ui/account-prefs", () => ({ machineModels: vi.fn().mockResolvedValue({}) }));
vi.mock("../../../src/frontend/ui/fork-ask", () => ({ askForkLaunch: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-open", () => ({ openTerminal: vi.fn() }));
vi.mock("../../../src/frontend/ui/remote-launch-run", () => ({ runRemoteAttach: vi.fn() }));
vi.mock("../../../src/frontend/ui/launch-arrival", () => ({ awaitArrival: vi.fn().mockResolvedValue(true) }));

import { invoke } from "@tauri-apps/api/core";
import { awaitArrival } from "../../../src/frontend/ui/launch-arrival";
import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { runRemoteAttach } from "../../../src/frontend/ui/remote-launch-run";
import { openTerminal } from "../../../src/frontend/ui/terminal-open";
import { askForkLaunch } from "../../../src/frontend/ui/fork-ask";
import { runForkFlow } from "../../../src/frontend/ui/fork-flow";
import type { ForkLaunch } from "../../../src/frontend/ui/session-writes";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";
import { copyTableTextsIn } from "../../test-support/copy-refs.ts";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources.ts";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const SRC = "cafe0000-1111-4222-8333-444455556666";
const NEW = "deadbeef-2222-4333-8444-555566667777";

const LIVE: ForkLaunch = {
  cwd: { kind: "known", value: "/p", from: "record" },
  account: { kind: "known", value: null, from: "process" },
  terminal: { kind: "known", value: { host: "tmux", terminal: "tmux-1" }, from: "terminal_list" },
};

/** 那台答的一项（`sessions-start` 的 `results[0]`）。 */
const reply = (extra: Record<string, unknown> = {}) => ({
  sid: NEW, outcome: "done", why: null, detail: "", session: null, bus: null, cmd: null, account: null, unavailable: null, ...extra,
});

/** 后端那一跳：记下每一次 `chan_call`，`sessions-start` 照 `answer` 答；开本机终端那一条记下来。 */
let calls: [string, string, Record<string, unknown>][] = [];
let opened: unknown[] = [];
function backend(answer: () => unknown): void {
  invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
    if (cmd === "open_local_terminal") {
      opened.push(a);
      return undefined;
    }
    if (cmd !== "chan_call") return undefined;
    const c = a as ChanCallArgs;
    const args = chanArgsJson(c) as Record<string, unknown>;
    calls.push([c.origin, c.op, args]);
    if (c.op === "sessions-start") return chanReply({ results: [answer()] });
    if (c.op === "accounts-list") return chanReply({ meta: { enabled: true }, accounts: [], notice: null });
    return chanReply({});
  });
}

const fork = (origin: string, launch: ForkLaunch) =>
  runForkFlow({ origin, newSessionId: NEW, sourceSessionId: SRC, launch });
const starts = () => calls.filter(([, op]) => op === "sessions-start");
const toasts = () => vi.mocked(showActionFailureToast).mock.calls.map((c) => c[0]);

beforeEach(() => {
  vi.clearAllMocks();
  invokeMock.mockReset();
  calls = [];
  opened = [];
  vi.mocked(awaitArrival).mockResolvedValue(true);
  vi.mocked(askForkLaunch).mockResolvedValue(null);
});

describe("起：交那台 sessions-start 一项", () => {
  it("★ 本机：开终端那一形、那一项是新 sid ＋ fresh_terminal ＋ fork_of；拿那台渲的那一行开窗；等到了才说「已分叉」", async () => {
    backend(() => reply({ session: "p-fork-cc", cmd: "ccm --resume x" }));
    expect(await fork(LOCAL_ORIGIN, LIVE)).toBe("started");
    expect(askForkLaunch, "三格全知道 ⇒ 不弹追问小窗").not.toHaveBeenCalled();
    const [[origin, , args]] = starts();
    expect(origin).toBe(LOCAL_ORIGIN);
    expect(args.mode).toBe("window");
    expect(args.local).toBe(true);
    expect(args.items).toEqual([{ sid: NEW, cwd: "/p", account: { kind: "base" }, fresh_terminal: true, fork_of: SRC }]);
    expect(opened).toEqual([{ cmd: "ccm --resume x", cwd: "/p" }]);
    expect(vi.mocked(awaitArrival)).toHaveBeenCalledWith(
      expect.objectContaining({ origin: LOCAL_ORIGIN, match: { sid: NEW }, tmuxName: "p-fork-cc", arrived: null }),
    );
    expect(toasts()).toEqual([copyText("forkFlow.done.title")]);
  });

  it("★ 远端、源会话在终端里：后台起（tmux 那一形），起好了开一个终端接回那台铸的名字", async () => {
    backend(() => reply({ session: "work-fork-cc" }));
    expect(await fork("devbox", { ...LIVE, account: { kind: "known", value: "z", from: "process" } })).toBe("started");
    const [[origin, , args]] = starts();
    expect(origin).toBe("devbox");
    expect(args.mode).toBe("tmux");
    expect((args.items as Record<string, unknown>[])[0].account).toEqual({ kind: "named", name: "z" });
    expect(vi.mocked(runRemoteAttach)).toHaveBeenCalledWith("devbox", expect.any(String), "work-fork-cc", { quiet: true });
  });

  it("远端、源会话不在任何终端里：开终端那一形，那台渲的那一行交给终端", async () => {
    backend(() => reply({ cmd: "ccm --resume y" }));
    await fork("devbox", { ...LIVE, terminal: { kind: "known", value: { host: "none" }, from: "terminal_list" } });
    expect(starts()[0][2].mode).toBe("window");
    expect(vi.mocked(openTerminal)).toHaveBeenCalledWith("devbox", "ccm --resume y");
  });

  it("FIX4 ④：发出去了但没看到分叉出来的会话起来 ⇒ 不是 started、不说「已分叉」", async () => {
    backend(() => reply({ cmd: "ccm --resume x" }));
    vi.mocked(awaitArrival).mockResolvedValueOnce(false);
    expect(await fork(LOCAL_ORIGIN, LIVE)).toBe("failed");
    expect(toasts()).toEqual([]);
  });

  it("那台起不成 ⇒ failed，说的是单个那条会说的那一句；不开窗", async () => {
    backend(() => reply({ outcome: "failed", why: "name_taken", session: "p-fork-cc" }));
    expect(await fork(LOCAL_ORIGIN, LIVE)).toBe("failed");
    expect(toasts()).toEqual([copyText("forkFlow.runForkFlow.failed")]);
    expect(opened).toEqual([]);
  });

  it("★ 号选不了 ⇒ 不起、给显式选择；点了 ⇒ 以那个号再交一次（新 sid · fresh_terminal · fork_of 照旧）", async () => {
    let n = 0;
    backend(() =>
      n++ === 0
        ? reply({ outcome: "skipped", why: "account_unavailable", detail: "z", unavailable: { requested: "z", pinned: false, listKnown: true, alternative: "b" } })
        : reply({ cmd: "ccm --resume x" }),
    );
    expect(await fork(LOCAL_ORIGIN, { ...LIVE, account: { kind: "known", value: "z", from: "process" } })).toBe("failed");
    expect(opened).toEqual([]);
    const refusal = vi.mocked(showActionFailureToast).mock.calls.find((c) => c[0] === copyText("accountPick.refused.title"));
    expect(refusal, "没给显式选择").toBeTruthy();
    (refusal![2] as { onClick: () => void }).onClick();
    await vi.waitFor(() => expect(starts()).toHaveLength(2));
    expect((starts()[1][2].items as Record<string, unknown>[])[0]).toEqual({
      sid: NEW, cwd: "/p", account: { kind: "named", name: "b" }, fresh_terminal: true, fork_of: SRC,
    });
  });

  it("★ 用户在追问小窗里取消 ⇒ 一次都不交那台", async () => {
    backend(() => reply());
    vi.mocked(askForkLaunch).mockResolvedValueOnce(null);
    expect(await fork("devbox", { ...LIVE, account: { kind: "unknown", why: "exited" }, terminal: { kind: "unknown", why: "exited" } })).toBe("cancelled");
    expect(starts()).toEqual([]);
  });
});

/**
 * 界面零处推断：先前那个推断函数（与它的事实收集）全仓零命中（推断住那台后端 `control/fork_launch.rs`）。
 */
describe("界面零处推断", () => {
  const GONE = ["inferForkLaunch", "deriveForkSource", "collectForkSource", "slotsNeedingInput"];
  const hits = (text: string): string[] => GONE.filter((n) => new RegExp(`\\b${n}\\b`).test(text));

  it("★ 那几个名字在生产段零命中（正控：同一把尺子认得出一段写着它们的文本）", () => {
    expect(hits("const f = inferForkLaunch(input); deriveForkSource(rows)"), "尺子是瞎的").toEqual([
      "inferForkLaunch",
      "deriveForkSource",
    ]);
    const found = productionTsFiles()
      .map((f) => ({ file: f.file, names: hits(f.text) }))
      .filter((f) => f.names.length > 0);
    expect(found, "界面又长出了分叉之后起的推断 —— 推断只在那台后端").toEqual([]);
  }, SCAN_TIMEOUT_MS);
});

/**
 * E78：**「分叉完怎么起」只能有一份。**
 *
 * 此前 `collectForkSource` → `runForkFlow` → 成功 toast 这三步由两个调用点各写一遍，
 * 连文案都是逐字重复的双写点、无守卫。Phase G 审计点名：`fork-flow.ts` 自称
 * 「唯一生产接线」而真正共享的只有中段 —— **名不副实的抽象比没有抽象更坏**，
 * 它让人以为改一处就够了。
 *
 * 这条守卫按**源码结构**判，不按行为判：行为测试证明不了「没有第二份」。
 */
describe("E78：两个调用点不许各自再拼一遍", () => {
  const read = (p: string) => readFileSync(resolve(__dirname, "../../..", p), "utf8");
  const CALL_SITES = ["src/frontend/ui/tabs.ts", "src/frontend/ui/views/session-viewer.ts"];

  it("★ 成功 toast 的文案只出现在 fork-flow.ts 里", () => {
    const marker = "已从这一轮分叉并起新会话";
    // 那句话进了文案表：「一个文件说的话」= 它的源码 ＋ 它经 copyText 取的表条目。
    const spoken = (f: string): string => [read(f), ...copyTableTextsIn(read(f))].join("\n");
    expect(spoken("src/frontend/ui/fork-flow.ts"), "接线层自己得有它，否则这条守卫在守空气").toContain(marker);
    for (const f of CALL_SITES) {
      expect(spoken(f), `${f} 又自己拼了一遍成功 toast`).not.toContain(marker);
    }
  });

  it("★ 调用点不许自己调 collectForkSource（那是接线层的活）", () => {
    for (const f of CALL_SITES) {
      expect(read(f), `${f} 绕过接线层自己查事实了`).not.toContain("collectForkSource");
    }
  });

  it("★ 反向自检：读文件这条路真的通（否则上面两条恒绿）", () => {
    expect(read("src/frontend/ui/fork-flow.ts").length).toBeGreaterThan(2000);
    for (const f of CALL_SITES) {
      expect(read(f), `${f} 应当仍在调 runForkFlow`).toContain("runForkFlow");
    }
  });
});

