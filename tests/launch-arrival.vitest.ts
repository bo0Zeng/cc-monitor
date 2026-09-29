/**
 * 设计/99 §2.2 ②「只有看见那台后端报出这个会话才说起来了；预算内没见到 ⇒ 说命令发出去了，但没看到会话起来，
 * 并给出启动器那一行的退出原话，不报成功」。
 *
 * 钉四件（期望值全是手写字面量）：① 认它：sid / 启动期令牌 / 「预期之后同目录的新 sid」三种各认对、不认错；
 * ② 见到了只说一次「起来了」、别的机器上的同名 sid 不算；③ 预算到了没见到 ⇒ 说没看到 ＋ tmux 那一屏最后几行
 * （抓不到说抓不到；直接开窗的说原话在窗口里）；④ 见到之后预算到点不再说话。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()), listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../src/tmux-control", () => ({ capturePane: vi.fn() }));

import { showActionFailureToast } from "../src/error-toast";
import { capturePane } from "../src/tmux-control";
import {
  ARRIVAL_BUDGET_MS,
  __resetArrivalsForTests,
  arrivalMatches,
  lastWords,
  noteLive,
  watchArrival,
  type ArrivalSpec,
} from "../src/launch-arrival";

const toast = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;
const capture = capturePane as unknown as ReturnType<typeof vi.fn>;
const none = new Set<string>();
const seen = (cwd: string | null, rbindToken: string | null = null) => ({ cwd, rbindToken });
const spec = (over: Partial<ArrivalSpec>): ArrivalSpec => ({
  origin: "devbox",
  match: { sid: "s1" },
  tmuxName: null,
  arrived: { title: "起来了", body: "B" },
  ...over,
});

describe("认它", () => {
  it("sid · 令牌 · 同目录的新 sid 各认对、不认错", () => {
    expect(arrivalMatches({ sid: "s1" }, "s1", seen(null), none)).toBe(true);
    expect(arrivalMatches({ sid: "s1" }, "s2", seen(null), none)).toBe(false);
    expect(arrivalMatches({ token: "t1" }, "sx", seen("/w", "t1"), none)).toBe(true);
    expect(arrivalMatches({ token: "t1" }, "sx", seen("/w", "t2"), none)).toBe(false);
    expect(arrivalMatches({ token: "" }, "sx", seen("/w", ""), none), "空令牌不是通配").toBe(false);
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
      "等了 45 秒，devbox上没有报出这个会话。tmux 会话「cc-s1」最后几行：\n$ claude --resume s1\nbash: claude: command not found\n$",
    );
  });

  it("抓不到那一屏 ⇒ 说抓不到；直接开窗 ⇒ 说原话在那个窗口里（本机说「本机」）", async () => {
    capture.mockRejectedValue(new Error("没有这个会话"));
    watchArrival(spec({ tmuxName: "cc-x" }));
    watchArrival(spec({ origin: "<local>", match: { sid: "s9" } }));
    await vi.advanceTimersByTimeAsync(ARRIVAL_BUDGET_MS + 1);
    const bodies = toast.mock.calls.map((c) => String(c[1])).sort();
    expect(bodies).toEqual([
      "等了 45 秒，devbox上没有报出这个会话。tmux 会话「cc-x」那一屏读不到：Error: 没有这个会话",
      "等了 45 秒，本机上没有报出这个会话。启动器的原话在那个终端窗口里，这边读不到。",
    ]);
  });
});
