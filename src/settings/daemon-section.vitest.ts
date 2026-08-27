/**
 * P2s-Y5 →〔`K-P1 KPY4` 08-26 **翻面**〕：**开关文案按状态说实话** + 这一区的行为。
 *
 * ## 翻面之前那条判据是什么、为什么不能留着
 *
 * 原判据是一张**禁词表**（`["后台常驻","继续运行","一直跑","常驻后台","保持运行"]`），
 * 依据是一条实测：daemon 是纯 stdio 子进程，monitor 一退它 153ms 内自己走
 * ⇒ 「继续跑」是一句做不到的承诺。
 *
 * `K-P1` 之后它在 Linux 上**真脱离**了 ⇒ 在那一支上「继续跑」是**真的**，
 * 而在没脱离的那一支上它**仍然是假的**。
 * ⇒ 判据翻成「**必须出现「无人监护」这一档，且它只在真脱离那一支出现**」。
 *
 * ## ⚠⚠ 翻转时有两条必须同轮做的，不然翻过来就是安慰剂
 *
 * 1. **扩人群** —— 原来那条 `readFileSync` 的**只有一个文件**（`daemon-section.ts`）、
 *    而且只剥**整行注释**。文案搬家 / 拼串 / 进一张 i18n 表 ⇒ 它**零命中地绿**。
 *    ⇒ 现在人群是 `daemon-section.ts` **+** `daemon-policy.ts`，而且**反过来要求**：
 *    那四句的唯一一个家是 `daemon-policy.ts`，`daemon-section.ts` 里一句都不许有。
 * 2. **配一个今天真会红的反向锚点** —— `the_unattended_wording_is_actually_present`：
 *    在 `K-P1` 之前的代码上，「无人监护」在这两个文件里出现 **0** 次 ⇒ 它当场红。
 *
 * ⚠ 判据的边界（照旧如实登记）：它扫的是**字面量与那个纯函数的四张脸**，
 * 逮不到「用一句意思相同但用词不同的话去承诺常驻」。那一层仍然只能靠人。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const calls: { name: string; args: unknown }[] = [];
let status: Record<string, unknown> = { channel: true, pid: 42 };
/** 按顺序喂给 `daemon_status` 的前几次读数（用完退回 `status`）。 */
let statusQueue: Record<string, unknown>[] = [];
let stored: Record<string, unknown> = {};

vi.mock("../ipc/commands", () => ({
  commands: {
    daemon_status: (a: unknown) => {
      calls.push({ name: "daemon_status", args: a });
      // ⚠ 支持「前几次还没落定」：A4 那条判据要证明它**轮询到落定**，
      // 而不是命令一返回就画一张操作前的快照。
      const next = statusQueue.shift();
      return Promise.resolve(next ?? status);
    },
    daemon_start: (a: unknown) => {
      calls.push({ name: "daemon_start", args: a });
      return Promise.resolve("已起");
    },
    daemon_stop: (a: unknown) => {
      calls.push({ name: "daemon_stop", args: a });
      return Promise.resolve("已停");
    },
    daemon_machines: () => {
      calls.push({ name: "daemon_machines", args: null });
      return Promise.resolve(["<local>", "甲机"]);
    },
    set_daemon_kill_on_exit: (a: unknown) => {
      calls.push({ name: "set_daemon_kill_on_exit", args: a });
      return Promise.resolve();
    },
    load_config: () => Promise.resolve(stored),
    save_config: (a: { value: Record<string, unknown> }) => {
      stored = a.value;
      return Promise.resolve();
    },
  },
}));

vi.mock("../remote-config", () => ({
  readRemoteConfig: () => Promise.resolve({ hosts: [{ label: "甲机", host: "a" }] }),
  hostKey: (h: { label: string; host: string }) => h.label.trim() || h.host,
}));

vi.mock("../error-toast", () => ({ showActionFailureToast: () => {} }));

import { DaemonSection } from "./daemon-section";
import {
  EXIT_KILLS,
  EXIT_SELF_DIES,
  EXIT_UNATTENDED,
  LOCAL_ORIGIN,
  describeExitBehavior,
} from "../daemon-policy";

/** 人群：这一区的用户可见文案今天住在哪几个文件里。**扩人群是翻转的一半。** */
const COPY_POPULATION = ["daemon-section.ts", "../daemon-policy.ts"] as const;

function readPopulation(): { name: string; src: string }[] {
  return COPY_POPULATION.map((rel) => ({
    name: rel,
    src: readFileSync(resolve(__dirname, rel), "utf8"),
  }));
}

/** 只看**用户可见的字符串字面量**：剥掉整行注释（内部词汇与解释性散文不管）。 */
function visibleOf(src: string): string {
  return src
    .split("\n")
    .filter((l) => !l.trim().startsWith("//") && !l.trim().startsWith("*"))
    .join("\n");
}

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  calls.length = 0;
  stored = {};
  status = { channel: true, pid: 42 };
  statusQueue = [];
});

describe("P2s daemon 开关区", () => {
  it("★★ 三档逐格钉死：「无人监护」必须出现，且**只在真脱离那一支**出现（KPY4①）", () => {
    // ── 四种输入组合，一格一格比。**不是「包含」而是「等于」** ——
    //    「包含」会放过「在正确那句后面又加了一句错的」。
    expect(describeExitBehavior({ killOnExit: true, detached: false })).toBe(EXIT_KILLS);
    expect(describeExitBehavior({ killOnExit: false, detached: false })).toBe(EXIT_SELF_DIES);
    expect(describeExitBehavior({ killOnExit: false, detached: true })).toBe(EXIT_UNATTENDED);
    // ★ 勾上那一档**不看 `detached`** —— 退出钩子对两条起法都收
    //   （`lib.rs` 的 `RunEvent::Exit`：被监护的走 `stop()`，脱离的走 `stop_local_backend()`）。
    //   ⚠ 这一格曾经是第四句「已经脱离了 ⇒ 这个勾管不到它」，那是**那个缺口的产物**；
    //   缺口补上之后它就成了假话。**这条断言就是那次订正的活体证据。**
    expect(describeExitBehavior({ killOnExit: true, detached: true })).toBe(EXIT_KILLS);

    // ── ★ 「无人监护」只许出现在**真脱离且没勾**那一支。
    for (const [name, text] of [
      ["EXIT_KILLS", EXIT_KILLS],
      ["EXIT_SELF_DIES", EXIT_SELF_DIES],
    ] as const) {
      expect(
        text.includes("无人监护"),
        `${name} 里出现了「无人监护」—— 那一档**没有脱离**，daemon 会在 monitor 退出后很快自行退出。\n` +
          "把它说成「无人监护地继续跑」是一句做不到的承诺，正是 P2s-Y5 当初立禁令要防的东西。",
      ).toBe(false);
      for (const w of ["继续跑", "后台常驻", "继续运行", "一直跑", "保持运行"]) {
        expect(text.includes(w), `${name} 里出现了「${w}」—— 同上，那一档它不会继续跑`).toBe(false);
      }
    }
    // ── ★★ 而真脱离那一支**必须**说出来（K14：这是本裁定的一半，不许只做常驻不做这句话）。
    expect(
      EXIT_UNATTENDED.includes("无人监护"),
      "真脱离那一档没说「无人监护」——`DECISIONS` `K14` 逐字：" +
        "「第一档必须在 UI 上如实说『继续跑，无人监护』，这是本裁定的一半，不许只做常驻不做这句话」。",
    ).toBe(true);
    expect(EXIT_UNATTENDED.includes("继续跑")).toBe(true);
  });

  it("★★ 人群扩到两个文件：那四句只许有**一个家**（KPY4②）", () => {
    const files = readPopulation();
    // 抽取器自检：读不到就下面全空转。
    for (const f of files) {
      expect(f.src.length, `${f.name} 读出来只有 ${f.src.length} 字节 —— 人群坏了，本条在空转`)
        .toBeGreaterThan(500);
    }
    const literals = [EXIT_KILLS, EXIT_UNATTENDED, EXIT_SELF_DIES];
    for (const lit of literals) {
      const homes = files.filter((f) => visibleOf(f.src).includes(lit)).map((f) => f.name);
      expect(
        homes,
        `「${lit.slice(0, 16)}…」出现在 ${homes.length} 个文件里：${homes.join(" / ")}\n` +
          "★ 那四句的唯一一个家是 `daemon-policy.ts`。抄进别处 = 下一次只改一处。",
      ).toEqual(["../daemon-policy.ts"]);
    }
    // 反过来：这一区必须**真的在用**那个家，而不是自己拼一份。
    const section = files.find((f) => f.name === "daemon-section.ts")!.src;
    expect(
      section.includes("describeExitBehavior"),
      "`daemon-section.ts` 不再调 `describeExitBehavior` —— 那它的文案是从哪来的？",
    ).toBe(true);
  });

  it("★★ 反向锚点：这两个文件里今天必须真的有「无人监护」（KPY4③）", () => {
    // ⚠ 这一条在 `K-P1` **之前**的代码上是**红**的：那时「无人监护」在人群里出现 0 次。
    //   它就是本轮翻转的活体证据 —— 少了它，上面那几条在一份「文案全被删光」的树上照样绿。
    const hits = readPopulation()
      .map((f) => ({ name: f.name, n: visibleOf(f.src).split("无人监护").length - 1 }))
      .filter((x) => x.n > 0);
    expect(
      hits.map((x) => x.name),
      "人群里一处「无人监护」都没有 —— 常驻做了、而界面没说，那正是 `K14` 点名不许的那一半。",
    ).toEqual(["../daemon-policy.ts"]);
  });

  it("本机永远在第一行——它不是另一种机器，只是不走 ssh 的那一台", async () => {
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    const rows = [...s.element.querySelectorAll<HTMLElement>(".daemon-row")];
    expect(rows.map((r) => r.dataset.origin)).toEqual([LOCAL_ORIGIN, "甲机"]);
  });

  it("每台机各查各的状态", async () => {
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    const asked = calls
      .filter((c) => c.name === "daemon_status")
      .map((c) => (c.args as { origin: string }).origin);
    expect(asked.sort()).toEqual([LOCAL_ORIGIN, "甲机"].sort());
    expect(s.element.querySelector(".daemon-row-state")?.textContent).toContain("已连上");
  });

  it("★ 那一行说的话跟着后端的 `detached` 走 —— 脱离了就说「无人监护」", async () => {
    status = { channel: true, pid: 42, detached: true };
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    const exit = s.element.querySelector<HTMLElement>(".daemon-row-exit");
    expect(exit?.textContent).toBe(EXIT_UNATTENDED);
    expect(exit?.dataset.detached).toBe("true");
  });

  it("★★ 后端**没给** `detached`（旧后端 / 远端恒 null）⇒ 按「没脱离」算，说今天那句", async () => {
    // ⚠ 这一格是**负例**，而它是这条判据的重量所在：
    //   只有正例的话，把 `st.detached === true` 换成一个恒真表达式也照样绿 ——
    //   那正是 `P2d §0a` 翻掉的 `SSH_CONNECTION` 那一形（假信号不报错，它只是一直说是）。
    status = { channel: true, pid: 42 };
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    const exit = s.element.querySelector<HTMLElement>(".daemon-row-exit");
    expect(exit?.textContent).toBe(EXIT_SELF_DIES);
    expect(exit?.dataset.detached).toBe("false");
    expect(
      exit?.textContent?.includes("无人监护"),
      "后端没说它脱离了，界面却替它说了「无人监护」—— 那是一句没有依据的话",
    ).toBe(false);
  });

  it("★ 机器清单问后端要，不自己算（A5：前端自己拼会与 Rust 的 origin 分叉四处）", async () => {
    new DaemonSection({ headless: true });
    await flush();
    await flush();
    expect(
      calls.some((c) => c.name === "daemon_machines"),
      "没调 daemon_machines —— 前端又在自己拼清单了。\n" +
        "Rust 侧会对重复 label 后缀化（pi → pi (#2)）、会按 enabled 过滤、启动后新增的不注册；\n" +
        "自己拼出来的名字对不上注册表，那些行的起/停恒回「没有这台机的把手」。",
    ).toBe(true);
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const src = readFileSync(resolve(__dirname, "daemon-section.ts"), "utf8");
    expect(
      src.split("hostKey(").length - 1,
      "daemon-section.ts 里又出现了 hostKey( —— 那正是自己拼 origin 的做法",
    ).toBe(0);
  });

  it("★ 起完之后轮询到落定，不画一张操作前的快照（A4）", async () => {
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    // 起：命令返回时 daemon 还没起来（channel:false），第三次才连上。
    statusQueue = [
      { channel: false, pid: null },
      { channel: false, pid: null },
      { channel: true, pid: 7 },
    ];
    const btns = [...s.element.querySelectorAll<HTMLButtonElement>(".daemon-row button")];
    btns[0].click();
    await new Promise((r) => setTimeout(r, 400));
    expect(
      s.element.querySelector(".daemon-row-state")?.textContent,
      "起完只画了一次就停手 —— 那张是操作前的快照（daemon_start 只是 spawn 了监护线程就返回）",
    ).toContain("已连上");
  });

  it("★ 存不下就把勾回退——屏上写着 A 而实际是 B 比报错更坏", async () => {
    const s = new DaemonSection({ headless: true });
    await flush();
    await flush();
    const mod = await import("../ipc/commands");
    const spy = vi
      .spyOn(mod.commands, "set_daemon_kill_on_exit")
      .mockRejectedValueOnce(new Error("盘满了"));
    const box = s.element.querySelector<HTMLInputElement>(".daemon-row-kill input")!;
    expect(box.checked).toBe(false);
    box.checked = true;
    box.onchange?.(new Event("change"));
    await flush();
    await flush();
    expect(box.checked, "存失败了勾还留在新位置 —— 界面在骗人").toBe(false);
    spy.mockRestore();
  });
});
