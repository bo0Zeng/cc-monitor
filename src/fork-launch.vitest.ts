/**
 * G3：分叉起会话的参数推断。
 *
 * 最要紧的一条不是「能不能推出来」，而是 **「推不出来的时候绝不猜」** ——
 * 拿当前账号顶替会静默地用错身份跑一条对话，而界面上看不出任何异样。
 */
import { describe, it, expect } from "vitest";
import {
  inferForkLaunch,
  slotsNeedingInput,
  describeSlot,
  forkTmuxName,
} from "./fork-launch";

describe("inferForkLaunch", () => {
  it("★ 源会话已退出 → 账号是 unknown，**即使调用方把 configDir 塞了进来**", () => {
    // 这条是本模块的核心防线：调用方很可能顺手把「当前账号」传进来。
    const f = inferForkLaunch({
      sourceIsLive: false,
      sourceCwd: "/home/u/p",
      liveConfigDir: "/home/u/.claude-accts/z", // ← 陷阱：已退出时这个值不可信
      liveTmuxName: "p-cc",
    });
    expect(f.account.kind).toBe("unknown");
    expect(f.tmux.kind).toBe("unknown");
    // cwd 来自会话记录本身，历史会话照样答得出
    expect(f.cwd).toEqual({
      kind: "known",
      value: "/home/u/p",
      from: "会话记录里的 cwd",
    });
  });

  it("★ 源会话活着 → 账号 / tmux 都知道，来源要写明", () => {
    const f = inferForkLaunch({
      sourceIsLive: true,
      sourceCwd: "/home/u/p",
      liveConfigDir: "/home/u/.claude-accts/z",
      liveTmuxName: "p-cc",
    });
    expect(f.account).toEqual({
      kind: "known",
      value: "/home/u/.claude-accts/z",
      from: "源会话进程的 pidfile",
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
      from: "源会话进程的 pidfile",
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
    expect(describeSlot("account", live)).toContain("pidfile");

    const dead = inferForkLaunch({ sourceIsLive: false, sourceCwd: "/p" });
    const t = describeSlot("account", dead);
    expect(t).toContain("需要你选一次");
    expect(t).toContain("pidfile"); // 说清为什么答不出
  });
});

describe("forkTmuxName", () => {
  it("★ 必须与原会话不同 —— 同名会让新会话 attach 进原窗口，毁掉「两条都活着」", () => {
    const n = forkTmuxName("myproj-cc");
    expect(n).not.toBe("myproj-cc");
    expect(n).toBe("myproj-fork-cc");
  });

  it("撞名再往后排，且每个候选都仍与原名不同", () => {
    const taken = ["myproj-fork-cc", "myproj-fork2-cc"];
    const n = forkTmuxName("myproj-cc", taken);
    expect(n).toBe("myproj-fork3-cc");
    expect(taken).not.toContain(n);
  });

  it("保持 `<X>-cc` 后缀形状（ccm 靠它认自己的会话）", () => {
    expect(forkTmuxName("a-cc").endsWith("-cc")).toBe(true);
    // 原名没有 -cc 后缀时也补上，不产出 ccm 认不出的名字
    expect(forkTmuxName("bare")).toBe("bare-fork-cc");
  });
});
