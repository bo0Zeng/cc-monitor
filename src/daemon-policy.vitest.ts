/**
 * P2s-Y4（acceptor: 机检）：策略**存得住、每台机各记各的、先推后存**。
 *
 * ## 为什么不做成「起 app 改开关重启」的 e2e
 *
 * DoD 原写「改了开关 → 重启 monitor → 策略还在」。那要真起 app（C7f 的沙箱 HOME 那套），
 * 而它验的其实是三件可以分开验的事：① 落盘的形状对不对 ② 每台机各记各的
 * ③ 启动时把盘上的值推给 Rust。三件都是纯函数/纯调用序，用例里验得更准、更快。
 *
 * ⚠ **拆开的代价如实登记**：没有任何一条用例覆盖「真的重启一次」。
 * 它们合起来**推不出**「重启后还在」——中间还隔着一个「启动时真的调了 initDaemonPolicy」，
 * 那一条由接线钉（`the_boot_path_really_pushes_the_daemon_policy`）管。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const ipc: { name: string; args: unknown }[] = [];
let stored: Record<string, unknown> = {};
/** 每次 `load_config` 的延迟（毫秒），按调用顺序取。空 = 0。 */
let loadDelays: number[] = [];

vi.mock("./ipc/commands", () => ({
  commands: {
    set_daemon_kill_on_exit: (args: unknown) => {
      ipc.push({ name: "set_daemon_kill_on_exit", args });
      return Promise.resolve();
    },
    // ⚠ **延迟是刻意的**：并发那条判据要复现「第一笔比第二笔慢」这个交错
    // （审计给的时序里，正是先发的那笔最后落盘、用陈旧 cfg 覆盖回去）。
    // 同步 resolve 的 mock 复现不出来 —— 判据第一版就是那样，去掉串行链它照样绿。
    load_config: () => {
      const d = loadDelays.shift() ?? 0;
      return new Promise<Record<string, unknown>>((r) => setTimeout(() => r(stored), d));
    },
    save_config: (a: { value: Record<string, unknown> }) => {
      stored = a.value;
      return Promise.resolve();
    },
  },
}));

import {
  readPolicy,
  killOnExit,
  setKillOnExit,
  initDaemonPolicy,
  describeDaemonHealth,
  HEALTH_CLEAN,
  HEALTH_CRASHED,
  HEALTH_LAST_MISSING,
  HEALTH_UNKNOWN,
  LOCAL_ORIGIN,
  DEFAULT_KILL_ON_EXIT,
  type DaemonHealth,
} from "./daemon-policy";

beforeEach(() => {
  ipc.length = 0;
  stored = {};
  loadDelays = [];
});

describe("P2s 每台机一份 daemon 策略", () => {
  it("缺省是「不结束」——用户什么都没设，就不该被杀 daemon", () => {
    expect(DEFAULT_KILL_ON_EXIT).toBe(false);
    expect(killOnExit({}, LOCAL_ORIGIN)).toBe(false);
  });

  it("★ 每台机各记各的：改甲机不动乙机", async () => {
    await setKillOnExit("甲机", true);
    await setKillOnExit("乙机", false);
    const p = readPolicy(stored);
    expect(p["甲机"]).toBe(true);
    expect(p["乙机"]).toBe(false);
    await setKillOnExit("甲机", false);
    expect(readPolicy(stored)["乙机"]).toBe(false);
    expect(readPolicy(stored)["甲机"]).toBe(false);
  });

  it("★ 存盘不许把 config 里别人的键抹掉（整份读—改—写的经典事故）", async () => {
    stored = { theme: "dark", remotes: [{ host: "a" }] };
    await setKillOnExit(LOCAL_ORIGIN, true);
    expect(stored.theme).toBe("dark");
    expect(stored.remotes).toEqual([{ host: "a" }]);
    expect(readPolicy(stored)[LOCAL_ORIGIN]).toBe(true);
  });

  it("★ 先推后存：推失败就不落盘（否则盘上写着 A 而运行中是 B）", async () => {
    const mod = await import("./ipc/commands");
    const spy = vi
      .spyOn(mod.commands, "set_daemon_kill_on_exit")
      .mockRejectedValueOnce(new Error("IPC 挂了"));
    await expect(setKillOnExit(LOCAL_ORIGIN, true)).rejects.toThrow("IPC 挂了");
    expect(readPolicy(stored)[LOCAL_ORIGIN]).toBeUndefined();
    spy.mockRestore();
  });

  it("★ 并发点击不会让 UI/运行时/盘上三方分叉（A3：读—改—写要串行）", async () => {
    // 两笔并发：先 true 后 false，且**让先发的那笔更慢** —— 那正是审计给的时序：
    // 后发的先落盘，先发的最后用陈旧 cfg 覆盖回去。
    loadDelays = [20, 0];
    const a = setKillOnExit("甲机", true);
    const b = setKillOnExit("甲机", false);
    await Promise.all([a, b]);
    expect(
      readPolicy(stored)["甲机"],
      "盘上不是最后一笔的值 —— 前一笔用陈旧 cfg 把后一笔覆盖回去了。\n" +
        "后果不是「少存一次」：下次启动 initDaemonPolicy 会把盘上那个值推回去，开关自己翻过来。",
    ).toBe(false);
    // 推给 Rust 的顺序也要与落盘顺序一致（否则运行时与盘上仍会分叉）
    const pushed = ipc
      .filter((c) => c.name === "set_daemon_kill_on_exit")
      .map((c) => (c.args as { kill: boolean }).kill);
    expect(pushed, "推送顺序与落盘顺序不一致").toEqual([true, false]);
  });

  it("启动时把盘上的每一台都推给后端", async () => {
    stored = { daemonPolicy: { [LOCAL_ORIGIN]: true, 甲机: false } };
    const p = await initDaemonPolicy();
    expect(p[LOCAL_ORIGIN]).toBe(true);
    const pushed = ipc.filter((c) => c.name === "set_daemon_kill_on_exit");
    expect(pushed).toHaveLength(2);
    expect(pushed.map((c) => (c.args as { origin: string }).origin).sort()).toEqual(
      [LOCAL_ORIGIN, "甲机"].sort(),
    );
  });

  it("形状不对的配置退回空表，不抛——开关坏了不该拖垮设置页", () => {
    expect(readPolicy({ daemonPolicy: "不是对象" })).toEqual({});
    expect(readPolicy({ daemonPolicy: [1, 2] })).toEqual({});
    expect(readPolicy({ daemonPolicy: { 甲机: "true" } })).toEqual({});
    expect(readPolicy({})).toEqual({});
  });

  it("空 origin 直接拒——放过它会造出一档谁都读不到的「全局」", async () => {
    await expect(setKillOnExit("  ", true)).rejects.toThrow("不许为空");
  });

  /**
   * ★ **接线钉**：启动路径**真的**推了一次。
   *
   * 上面那些用例全都直接调 `initDaemonPolicy`，所以「没人在启动时调它」它们**一条都逮不到**
   * ——而后果不是报错，是每台机静默退回缺省，用户设过的开关看起来还在、实际不生效。
   * 这就是仓里 F03 那个坑：「模块存在 ≠ 模块被调用」。
   *
   * 顺带钉「失败不拦启动」：那一处必须有 catch，否则策略读不到会把主界面拖垮。
   */
  it("★ 启动路径真的调了 initDaemonPolicy，且失败不拦启动", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const src = readFileSync(resolve(__dirname, "main.ts"), "utf8");
    // ★ **整行钉**（`pin_line` 那一族）：一次钉住两件事 —— 启动时真的调了它、
    // 且调用点带 `.catch`。
    //
    // ⚠ 刻意**不在整份源码上做裸子串匹配**（那个方法名不在这里写全 ——
    // `scanning-guard-registry` 数的就是那个字面量，写在散文里也会被算成一处，
    // 本轮已经被自己的注释绊倒三次了）。它在递减棘轮里，病也正是「匹配单位比事实小」：
    // 子串匹配对 `initDaemonPolicy().then(...)`（没有 catch）照样绿。
    const PINNED = "void initDaemonPolicy().catch((e) => {";
    const hit = src.split("\n").filter((l) => l.trim() === PINNED).length;
    expect(
      hit,
      `main.ts 里没有恰好一行是 \`${PINNED}\`（实得 ${hit} 行）。\n` +
        "① 一行都没有 ⇒ 启动时不推策略，每台机静默退回缺省 —— " +
        "用户设过的开关看起来还在、实际不生效，而且**不会报错**。\n" +
        "② 有但形状变了（比如去掉了 .catch）⇒ 策略读不到会把主界面拖垮，它只是附加功能。\n" +
        "改了那一行的写法，就来改这里的字面量 —— 这条摩擦是有意的。",
    ).toBe(1);
  });
});

/**
 * K-P3 KP3C 的 TS 那一半 ——〔`K-P3` `§3-5` 第四行登记的欠账，`K-P3b` 补上〕。
 *
 * 交付第一档时这份文件不在那件的写区 ⇒ TS 侧只有 Rust 那侧的**源码文本**判据
 * （`the_health_reading_branches_are_wired_into_the_typescript`，它证的是「那三支写在那儿」）。
 * 这一组是**运行时**那一半：三档逐格等号。
 */
describe("K-P3 那句读数 —— 三档逐格钉死", () => {
  const NONE: DaemonHealth = {
    crashed: 0,
    refused: 0,
    neverStarted: 0,
    misread: 0,
    last: null,
  };

  it("★★ 第一档：账上一条都没有 ⇒「答不出来」，**即使 crashed === 0**", () => {
    expect(
      describeDaemonHealth({ ...NONE }),
      "一条记录都没有的机器被说成了别的 —— §0-1 逐字：\n" +
        "「今天不是『它没崩过』，是『没有任何东西在记它崩没崩』…… 这两句话差得很远，不许混用」。",
    ).toBe(HEALTH_UNKNOWN);
    // ★ 这一格是**判准本身**：把 `seen === 0` 换成 `crashed === 0`，上面那格照样绿，
    //   而这一格当场红 —— 一台记到过 2 次读坏了的机器会被说成「答不出来」，
    //   可它明明有账。**两格一起才钉住那个判准。**
    expect(
      describeDaemonHealth({ ...NONE, misread: 2 }),
      "账上记到过事（读坏了 2 次），却仍说「答不出来」——\n" +
        "那说明第一档的判准是 `crashed === 0` 而不是「这本账上一条记录都没有」。",
    ).not.toBe(HEALTH_UNKNOWN);
  });

  it("★ 第二档：记到过事、一次崩溃都没有 ⇒「没崩过」，且带得出读坏了几次", () => {
    expect(describeDaemonHealth({ ...NONE, misread: 2 })).toBe(
      HEALTH_CLEAN.replace("{misread}", "2"),
    );
    // 「读坏了」不算它崩 —— 那是我们这一侧的读端（B1 那条错误诊断的全部内容）。
    expect(describeDaemonHealth({ ...NONE, refused: 1, misread: 0 })).toBe(
      HEALTH_CLEAN.replace("{misread}", "0"),
    );
  });

  it("★ 第三档：崩过 ⇒ 带次数与最后那一行；那一行没留住也要说出口", () => {
    expect(describeDaemonHealth({ ...NONE, crashed: 3, last: "甲那一行" })).toBe(
      HEALTH_CRASHED.replace("{crashed}", "3").replace("{last}", "甲那一行"),
    );
    expect(
      describeDaemonHealth({ ...NONE, crashed: 1, last: null }),
      "崩过、而那一行没留住 —— 这一格不许拿空串糊过去",
    ).toBe(HEALTH_CRASHED.replace("{crashed}", "1").replace("{last}", HEALTH_LAST_MISSING));
  });

  it("★ 占位符必须真的被填掉 —— 漏一个 replace 就把 `{crashed}` 端到用户眼前", () => {
    for (const h of [
      { ...NONE, misread: 5 },
      { ...NONE, crashed: 2, last: "乙" },
      { ...NONE, crashed: 2, last: null },
    ]) {
      const said = describeDaemonHealth(h);
      for (const ph of ["{crashed}", "{last}", "{misread}"]) {
        expect(said.includes(ph), `读数里还留着占位符 ${ph}：${said}`).toBe(false);
      }
    }
  });
});
