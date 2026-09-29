/**
 * G3：分叉起会话的参数推断。
 *
 * 最要紧的一条不是「能不能推出来」，而是 **「推不出来的时候绝不猜」** ——
 * 拿当前账号顶替会静默地用错身份跑一条对话，而界面上看不出任何异样。
 */
import { describe, it, expect } from "vitest";
import { copyText } from "../src/copy-table";
import {
  inferForkLaunch,
  slotsNeedingInput,
  describeSlot,
} from "../src/fork-launch";

describe("inferForkLaunch", () => {
  it("★ 源会话已退出 → 账号是 unknown，**即使调用方把 configDir 塞了进来**", () => {
    // 这条是本模块的核心防线：调用方很可能顺手把「当前账号」传进来。
    const f = inferForkLaunch({
      sourceIsLive: false,
      sourceCwd: "/home/u/p",
      liveConfigDir: "/home/u/.claude-alt/z", // ← 陷阱：已退出时这个值不可信
      liveTmuxName: "p-cc",
    });
    expect(f.account.kind).toBe("unknown");
    expect(f.tmux.kind).toBe("unknown");
    // cwd 来自会话记录本身，历史会话照样答得出
    expect(f.cwd).toEqual({
      kind: "known",
      value: "/home/u/p",
      from: "会话记录",
    });
  });

  it("★ 源会话活着 → 账号 / tmux 都知道，来源要写明", () => {
    const f = inferForkLaunch({
      sourceIsLive: true,
      sourceCwd: "/home/u/p",
      liveConfigDir: "/home/u/.claude-alt/z",
      liveTmuxName: "p-cc",
    });
    expect(f.account).toEqual({
      kind: "known",
      value: "/home/u/.claude-alt/z",
      from: "原会话的进程",
    });
    expect(f.tmux.kind === "known" && f.tmux.value).toBe(true);
  });

  it("★ 账号 0（configDir 为 null）是「知道」，不是「不知道」", () => {
    // 结构性判据：configDir 缺席 = 账号 0。它是一个**确定的答案**。
    const f = inferForkLaunch({
      sourceIsLive: true,
      sourceCwd: "/p",
      liveConfigDir: null,
    });
    expect(f.account).toEqual({
      kind: "known",
      value: null,
      from: "原会话的进程",
    });
  });

  it("活着但查不到账号（undefined）→ unknown，与「账号 0」区分开", () => {
    const f = inferForkLaunch({ sourceIsLive: true, sourceCwd: "/p" });
    expect(f.account.kind).toBe("unknown");
  });

  it("活着且不在 tmux 里 → known(false)，不是 unknown", () => {
    const f = inferForkLaunch({
      sourceIsLive: true,
      sourceCwd: "/p",
      liveConfigDir: null,
      liveTmuxName: "",
    });
    expect(f.tmux).toEqual({ kind: "known", value: false, from: "tmux 会话清单" });
  });

  it("没有 cwd（空串也算没有）→ unknown", () => {
    expect(inferForkLaunch({ sourceIsLive: true, sourceCwd: "   " }).cwd.kind).toBe(
      "unknown",
    );
    expect(inferForkLaunch({ sourceIsLive: true }).cwd.kind).toBe("unknown");
  });
});

describe("slotsNeedingInput", () => {
  it("已退出的会话要问账号与 tmux，不问 cwd", () => {
    const f = inferForkLaunch({ sourceIsLive: false, sourceCwd: "/p" });
    expect(slotsNeedingInput(f)).toEqual(["account", "tmux"]);
  });

  it("★ 活着且信息齐全 → 一次都不用问（否则每次分叉都弹窗，功能就废了）", () => {
    const f = inferForkLaunch({
      sourceIsLive: true,
      sourceCwd: "/p",
      liveConfigDir: null,
      liveTmuxName: "p-cc",
    });
    expect(slotsNeedingInput(f)).toEqual([]);
  });
});

describe("describeSlot", () => {
  it("知道的说「跟原会话一致」并带来源；不知道的说清为什么要问", () => {
    const live = inferForkLaunch({
      sourceIsLive: true,
      sourceCwd: "/p",
      liveConfigDir: null,
    });
    expect(describeSlot("account", live)).toContain("跟原会话一致");
    expect(describeSlot("account", live)).toContain("原会话的进程");

    const dead = inferForkLaunch({ sourceIsLive: false, sourceCwd: "/p" });
    const t = describeSlot("account", dead);
    // 说清为什么答不出（〔CP2b〕CP1 裁：不说 pidfile，说后果）。〔FIX2 · 99 §2.1 ㉛②〕按文案键断言、不钉原文。
    expect(t).toBe(
      copyText("forkLaunch.slot.ask", { label: copyText("forkLaunch.slot.account"), why: copyText("forkLaunch.exited.account") }),
    );
  });
});

// 〔FIX4 · `设计/90 §3` J7〕这里原来一组 `forkTmuxName`（分叉会话名 `<源名>-fork-cc` ＋ 避让）：它随派生 ＋ 避让搬进后端
// （`control/ccm/plan.rs::fork_tmux_base`，帧命令 `tmux-name-mint {forkOf}`）。五条期望（与源名不同 · 撞了往后排 · `-cc` 形状 ·
// 拿 cwd 当源也建得出来 · 空 ⇒ `session-fork-cc`）原样搬进 `tests/backend/control/ccm/plan_tests.rs::the_fork_base_differs_from_its_source_and_is_always_a_legal_new_name`
// （「建得出来」那一格的消费者换成真正判新建名的 `validate_tmux_name`）。
