/**
 * 「只有看见那台后端报出这个会话才说起来了；预算内没见到 ⇒ 说命令发出去了，但没看到会话起来，
 * 并给出启动器那一行的退出原话，不报成功」。
 *
 * 钉四件（期望值全是手写字面量）：① 认它：sid / 启动期令牌 / 「预期之后同目录的新 sid」三种各认对、不认错；
 * ② 见到了只说一次「起来了」、别的机器上的同名 sid 不算；③ 预算到了没见到 ⇒ 说没看到 ＋ tmux 那一屏最后几行
 * （抓不到说抓不到；直接开窗的说原话在窗口里）；④ 见到之后预算到点不再说话。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()), listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-reads", () => ({ previewByTmuxName: vi.fn() }));

import { emit } from "@tauri-apps/api/event";
import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { previewByTmuxName } from "../../../src/frontend/ui/terminal-reads";
import {
  ARRIVAL_BUDGET_MS,
  __resetArrivalsForTests,
  arrivalMatches,
  arrivedBody,
  lastWords,
  noteLive,
  watchArrival,
  type ArrivalSpec,
} from "../../../src/frontend/ui/launch-arrival";

const toast = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;
const capture = previewByTmuxName as unknown as ReturnType<typeof vi.fn>;
const none = new Set<string>();
const seen = (cwd: string | null) => ({ cwd });
const spec = (over: Partial<ArrivalSpec>): ArrivalSpec => ({
  origin: "devbox",
  match: { sid: "s1" },
  tmuxName: null,
  arrived: { title: "起来了", body: "B" },
  ...over,
});

describe("认它", () => {
  it("sid · 同目录的新 sid 各认对、不认错", () => {
    expect(arrivalMatches({ sid: "s1" }, "s1", seen(null), none)).toBe(true);
    expect(arrivalMatches({ sid: "s1" }, "s2", seen(null), none)).toBe(false);
    expect(arrivalMatches({ cwd: "/w/p/" }, "n1", seen("/w/p"), none)).toBe(true);
    expect(arrivalMatches({ cwd: "/w/p" }, "n1", seen("/w/q"), none)).toBe(false);
    expect(arrivalMatches({ cwd: "/w/p" }, "old", seen("/w/p"), new Set(["old"])), "预期之前就报过的不算新起的").toBe(false);
  });

  it("原话 = 那一屏最后 6 行非空行", () => {
    expect(lastWords("a\n\nb\nc\nd\ne\nf\ng\n\n")).toBe("b\nc\nd\ne\nf\ng");
    expect(lastWords("\n  \n")).toBe("");
  });
});

describe("等它", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    toast.mockReset();
    capture.mockReset();
    __resetArrivalsForTests();
  });
  afterEach(() => {
    __resetArrivalsForTests();
    vi.useRealTimers();
  });

  it("见到了只说一次；别的机器上的同名 sid 不算；见到之后预算到点不再说话", async () => {
    watchArrival(spec({}));
    noteLive("other", "s1", seen(null));
    expect(toast).not.toHaveBeenCalled();
    noteLive("devbox", "s1", seen(null));
    noteLive("devbox", "s1", seen(null));
    expect(toast.mock.calls).toEqual([["起来了", "B", { level: "info", durationMs: 6000 }]]);
    await vi.advanceTimersByTimeAsync(ARRIVAL_BUDGET_MS + 1);
    expect(toast).toHaveBeenCalledTimes(1);
  });

  it("预算到了没见到 ⇒ 没看到 ＋ tmux 那一屏最后几行", async () => {
    capture.mockResolvedValue("$ claude --resume s1\nbash: claude: command not found\n$ \n");
    watchArrival(spec({ tmuxName: "cc-s1" }));
    await vi.advanceTimersByTimeAsync(ARRIVAL_BUDGET_MS - 1);
    expect(toast).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(2);
    expect(capture).toHaveBeenCalledWith("devbox", "cc-s1");
    expect(toast.mock.calls[0][0]).toBe("命令发出去了，但没看到会话起来");
    expect(toast.mock.calls[0][1]).toBe(
      "等了 45 秒，devbox 上没有报出这个会话。tmux 会话「cc-s1」最后几行：\n$ claude --resume s1\nbash: claude: command not found\n$",
    );
  });

  it("抓不到那一屏 ⇒ 说抓不到；直接开窗 ⇒ 说原话在那个窗口里（本机说「本机」）", async () => {
    capture.mockRejectedValue(new Error("没有这个会话"));
    watchArrival(spec({ tmuxName: "cc-x" }));
    watchArrival(spec({ origin: "<local>", match: { sid: "s9" } }));
    await vi.advanceTimersByTimeAsync(ARRIVAL_BUDGET_MS + 1);
    const bodies = toast.mock.calls.map((c) => String(c[1])).sort();
    expect(bodies).toEqual([
      "等了 45 秒，devbox 上没有报出这个会话。tmux 会话「cc-x」那一屏读不到：Error: 没有这个会话",
      "等了 45 秒，本机上没有报出这个会话。启动器的原话在那个终端窗口里，这边读不到。",
    ]);
  });
});

/** 要求：「执行器回一个『等到了没有』的 promise，调用方等到才说」—— 带票的那件，主窗口等到 / 没等到都回一声。 */
describe("FIX4 ④ 带票的等", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    toast.mockReset();
    vi.mocked(emit).mockClear();
    __resetArrivalsForTests();
  });
  afterEach(() => {
    __resetArrivalsForTests();
    vi.useRealTimers();
  });
  const done = () => vi.mocked(emit).mock.calls.filter((c) => c[0] === "launch-arrival-done").map((c) => c[1]);

  it("等到了 ⇒ 回 arrived:true、`arrived: null` 时自己不说话；没等到 ⇒ 回 arrived:false 并照常说没看到", async () => {
    watchArrival(spec({ match: { sid: "s1" }, arrived: null, ticket: "T1" }));
    noteLive("devbox", "s1", seen(null));
    expect(done()).toEqual([{ ticket: "T1", arrived: true }]);
    expect(toast).not.toHaveBeenCalled();
    watchArrival(spec({ match: { sid: "s2" }, arrived: null, ticket: "T2" }));
    await vi.advanceTimersByTimeAsync(ARRIVAL_BUDGET_MS + 1);
    expect(done()).toEqual([
      { ticket: "T1", arrived: true },
      { ticket: "T2", arrived: false },
    ]);
    expect(toast.mock.calls.map((c) => c[0])).toEqual(["命令发出去了，但没看到会话起来"]);
  });

  // 要求：「文案小事：『lx上报出了这个会话』机器名与汉字无空格」。
  it("机器名以字母数字收尾 ⇒ 与后面的汉字隔一个空格；「本机」不隔", () => {
    expect(arrivedBody("lx")).toBe("lx 上报出了这个会话。");
    expect(arrivedBody("<local>")).toBe("本机上报出了这个会话。");
  });
});

/** 从设置窗起的会话：主窗口说的那一句（起来了 / 没看到）也交回发起方那扇窗（主窗口常被设置窗挡着，WIN4 那一形「一声不出」）。 */
describe("发起方在别的窗口 ⇒ 那一句也交回那扇窗", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    toast.mockReset();
    capture.mockReset();
    vi.mocked(emit).mockClear();
    __resetArrivalsForTests();
  });
  afterEach(() => {
    __resetArrivalsForTests();
    vi.useRealTimers();
  });
  const echoed = () => vi.mocked(emit).mock.calls.filter((c) => c[0] === "launch-arrival-said").map((c) => c[1]);

  it("没看到 ⇒ 交回 settings；起来了 ⇒ 交回 settings；发起方就是主窗口 / 没说 ⇒ 不交", async () => {
    capture.mockResolvedValue("ccm: 无法进入目录: /home/u/nope\n");
    watchArrival(spec({ tmuxName: "nope-cc", from: "settings" }));
    await vi.advanceTimersByTimeAsync(ARRIVAL_BUDGET_MS + 1);
    expect(echoed()).toEqual([
      {
        to: "settings",
        title: "命令发出去了，但没看到会话起来",
        body: "等了 45 秒，devbox 上没有报出这个会话。tmux 会话「nope-cc」最后几行：\nccm: 无法进入目录: /home/u/nope",
        level: "error",
        durationMs: 15000,
      },
    ]);
    vi.mocked(emit).mockClear();
    watchArrival(spec({ match: { sid: "s2" }, from: "settings" }));
    noteLive("devbox", "s2", seen(null));
    expect(echoed()).toEqual([{ to: "settings", title: "起来了", body: "B", level: "info", durationMs: 6000 }]);
    vi.mocked(emit).mockClear();
    watchArrival(spec({ match: { sid: "s3" }, from: "main" }));
    watchArrival(spec({ match: { sid: "s4" } }));
    noteLive("devbox", "s3", seen(null));
    noteLive("devbox", "s4", seen(null));
    expect(echoed()).toEqual([]);
  });
});
