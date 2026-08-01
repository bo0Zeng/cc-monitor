/**
 * G3：分叉起会话的参数推断。
 *
 * 最要紧的一条不是「能不能推出来」，而是 **「推不出来的时候绝不猜」** ——
 * 拿当前账号顶替会静默地用错身份跑一条对话，而界面上看不出任何异样。
 */
import { describe, it, expect } from "vitest";
// 判据取自**真正的消费者**，不在测试里重抄它的正则。
import { planResumeTmux } from "./launch-requests";
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
    // Phase G：数字**追加在最后**（`…-cc-2`），与 `remote-launch.ts::pickFreshTmuxName`
    // 白纸黑字的同一条规则对齐：「让『第几个』始终是名字的末段」。原来是 `-fork2-cc`（数字在中间）。
    const taken = ["myproj-fork-cc", "myproj-fork-cc-2"];
    const n = forkTmuxName("myproj-cc", taken);
    expect(n).toBe("myproj-fork-cc-3");
    expect(taken).not.toContain(n);
  });

  it("保持 `<X>-cc` 后缀形状（ccm 靠它认自己的会话）", () => {
    expect(forkTmuxName("a-cc").endsWith("-cc")).toBe(true);
    // 原名没有 -cc 后缀时也补上，不产出 ccm 认不出的名字
    expect(forkTmuxName("bare")).toBe("bare-fork-cc");
  });

  /**
   * ★★ Phase G 审计抓出的阻塞：源会话已退出时**没有 tmux 名可继承**，调用方
   * （`fork-start.ts`）会拿 **cwd** 当基名。此前不净化 ⇒ 产出 `/home/pi/proj-fork-cc`，
   * 被 `planResumeTmux` 的 `/^[A-Za-z0-9_][A-Za-z0-9_-]*$/` 当场拒 ⇒
   * **「分叉一条已退出的远端会话」这条主路径 100% 起不来**（失败还被吞成成功 toast）。
   *
   * 这里**不重抄那条正则**（重抄就会与生产侧各写一份、再漂一次）——
   * 直接把名字喂给真正的消费者 `planResumeTmux`，不抛就是合法。
   */
  it("★★ 拿 cwd 当基名也必须产出合法名（真正的判据：planResumeTmux 收得下）", () => {
    const SID = "0473c3a0-1111-2222-3333-444455556666";
    for (const src of ["/home/pi/proj", "/tmp/e2e-remote", "/p/my proj", "/", "", "中文目录"]) {
      const n = forkTmuxName(src);
      expect(() => planResumeTmux(SID, "/p", "cc", n), `基名 ${JSON.stringify(src)} 产出了非法名 ${n}`).not.toThrow();
      expect(n.endsWith("-cc"), `${n} 丢了 -cc 形状`).toBe(true);
    }
  });

  it("净化后为空（如 cwd 是 `/`）→ 退回 session，与 deriveTmuxName 的兜底一致", () => {
    expect(forkTmuxName("/")).toBe("session-fork-cc");
    expect(forkTmuxName("")).toBe("session-fork-cc");
  });
});
