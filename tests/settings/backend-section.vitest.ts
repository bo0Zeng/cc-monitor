/**
 * P2s-Y5 →〔`K-P1 KPY4` 08-26 **翻面**〕：**开关文案按状态说实话** + 这一区的行为。
 *
 * ## 翻面之前那条判据是什么、为什么不能留着
 *
 * 原判据是一张**禁词表**（`["后台常驻","继续运行","一直跑","常驻后台","保持运行"]`），
 * 依据是一条实测：backend 是纯 stdio 子进程，monitor 一退它 153ms 内自己走
 * ⇒ 「继续跑」是一句做不到的承诺。
 *
 * `K-P1` 之后它在 Linux 上**真脱离**了 ⇒ 在那一支上「继续跑」是**真的**，
 * 而在没脱离的那一支上它**仍然是假的**。
 * ⇒ 判据翻成「**必须出现「无人监护」这一档，且它只在真脱离那一支出现**」。
 *
 * ## ⚠⚠ 翻转时有两条必须同轮做的，不然翻过来就是安慰剂
 *
 * 1. **扩人群** —— 原来那条 `readFileSync` 的**只有一个文件**（`backend-section.ts`）、
 *    而且只剥**整行注释**。文案搬家 / 拼串 / 进一张 i18n 表 ⇒ 它**零命中地绿**。
 *    ⇒ 现在人群是 `backend-section.ts` **+** `backend-policy.ts`，而且**反过来要求**：
 *    那四句的唯一一个家是 `backend-policy.ts`，`backend-section.ts` 里一句都不许有。
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
/** 按顺序喂给 `backend_status` 的前几次读数（用完退回 `status`）。 */
let statusQueue: Record<string, unknown>[] = [];
/**
 * 〔B2〕后端 `exit-policy-read` 回的那一份（值住后端那台机器上）。`null` ⇒ 这台问不到（命令抛错）。
 * 键名与后端 `exit_policy::wire` 逐格一致：`state` / `killOnExit`（`reason` / `path` 本区不用）。
 */
let exitAnswer: Record<string, unknown> | null = null;
/** 〔C4c〕下一次「交后端写」要回的失败（`null` = 照常写）。原先用 `spyOn(commands.set_backend_exit_policy)`，那条命令退役了。 */
let failNextSet: Error | null = null;

vi.mock("../../src/ipc/commands", () => ({
  commands: {
    backend_status: (a: unknown) => {
      calls.push({ name: "backend_status", args: a });
      // ⚠ 支持「前几次还没落定」：A4 那条判据要证明它**轮询到落定**，
      // 而不是命令一返回就画一张操作前的快照。
      const next = statusQueue.shift();
      return Promise.resolve(next ?? status);
    },
    backend_start: (a: unknown) => {
      calls.push({ name: "backend_start", args: a });
      return Promise.resolve("已起");
    },
    backend_stop: (a: unknown) => {
      calls.push({ name: "backend_stop", args: a });
      return Promise.resolve("已停");
    },
    backend_machines: () => {
      calls.push({ name: "backend_machines", args: null });
      return Promise.resolve(["<local>", "甲机"]);
    },
    // 〔C4c · 第四波 4B〕「退出行为」两问改走通道：`chan_call`（op = `exit-policy-read` / `exit-policy-set`）。
    //   这里把一发 `chan_call` 译回判据里的旧叫法（`backend_exit_policy` / `set_backend_exit_policy`），
    //   回包译成后端那份字节（`ArrayBuffer`），问不到译成通道那一跳「没有控制通道」的线上形状。
    chan_call: (a: { origin: string; op: string; payload: number[] }) => {
      const body = JSON.parse(new TextDecoder().decode(Uint8Array.from(a.payload))) as Record<string, unknown>;
      const bytes = (v: unknown) => {
        const u = new TextEncoder().encode(JSON.stringify(v));
        return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
      };
      const noChannel = { err: { Hop: { idx: 1, tag: "open", reach: "NotSent", why: "Unreachable" } }, body: [] };
      if (a.op === "exit-policy-read") {
        calls.push({ name: "backend_exit_policy", args: { origin: a.origin } });
        return exitAnswer === null ? Promise.reject(noChannel) : bytes(exitAnswer);
      }
      if (a.op === "exit-policy-set") {
        const kill = body.killOnExit as boolean;
        calls.push({ name: "set_backend_exit_policy", args: { origin: a.origin, kill } });
        if (failNextSet) {
          const why = failNextSet.message;
          failNextSet = null;
          const refusal = Array.from(new TextEncoder().encode(JSON.stringify({ code: "write_failed", message: why })));
          return Promise.reject({ err: "Refused", body: refusal });
        }
        // 〔B2〕后端写完**读回**的那一份：这里就让「盘上」变成写进去的值，后续每一次现问都读到它。
        exitAnswer = { state: "chosen", killOnExit: kill, reason: null, path: "x" };
        return bytes(exitAnswer);
      }
      return Promise.reject(new Error(`判据没料到的通道问法：${a.op}`));
    },
  },
}));

vi.mock("../../src/remote-config", () => ({
  readRemoteConfig: () => Promise.resolve({ hosts: [{ label: "甲机", host: "a" }] }),
  hostKey: (h: { label: string; host: string }) => h.label.trim() || h.host,
}));

vi.mock("../../src/error-toast", () => ({ showActionFailureToast: () => {} }));

import { BACKEND_COLUMNS, BackendSection } from "../../src/settings/backend-section";
import { srcDirOf } from "../test-support/repo-root";
import COPY_TABLE from "../../src/shared/copy/table.json";
import {
  EXIT_KILLS,
  EXIT_SELF_DIES,
  EXIT_UNATTENDED,
  EXIT_UNREADABLE,
  HEALTH_CLEAN,
  HEALTH_LAST_MISSING,
  HEALTH_UNKNOWN,
  HEALTH_UNKNOWN_WHY,
  LOCAL_ORIGIN,
  describeBackendHealth,
  describeExitBehavior,
  describeHealthDetail,
} from "../../src/backend-policy";

/**
 * 人群：这一区的用户可见文案今天住在哪几个文件里。**扩人群是翻转的一半。**
 * 〔CP2b〕家搬进了文案表 ⇒ 人群 +1（`table.json`），而「唯一一个家」从 `backend-policy.ts` 换成表。
 */
const TABLE_REL = "../../src/shared/copy/table.json";
const COPY_POPULATION = ["backend-section.ts", "../../src/backend-policy.ts", TABLE_REL] as const;

function readPopulation(): { name: string; src: string }[] {
  return COPY_POPULATION.map((rel) => ({
    name: rel,
    src: readFileSync(resolve(srcDirOf(__dirname), rel), "utf8"),
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
  failNextSet = null;
  exitAnswer = { state: "absent", killOnExit: false, reason: null, path: "x" };
  status = { channel: true, pid: 42 };
  statusQueue = [];
});

describe("P2s backend 开关区", () => {
  it("★★ 三档逐格钉死：「无人监护」必须出现，且**只在真脱离那一支**出现（KPY4①）", () => {
    // ── 四种输入组合，一格一格比。**不是「包含」而是「等于」** ——
    //    「包含」会放过「在正确那句后面又加了一句错的」。
    const on = (killOnExit: boolean, detached: boolean) =>
      describeExitBehavior({ policy: "chosen", killOnExit, detached });
    expect(on(true, false)).toBe(EXIT_KILLS);
    expect(on(false, false)).toBe(EXIT_SELF_DIES);
    expect(on(false, true)).toBe(EXIT_UNATTENDED);
    // ★ 勾上那一档**不看 `detached`** —— 两条起法都会被收：
    //   被监护的由 monitor 退出臂 `stop()`；脱离的〔B2〕由后端自己在最后一个客户走的那一刻现读、退出。
    //   ⚠ 这一格曾经是第四句「已经脱离了 ⇒ 这个勾管不到它」，那是**那个缺口的产物**；
    //   缺口补上之后它就成了假话。**这条断言就是那次订正的活体证据。**
    expect(on(true, true)).toBe(EXIT_KILLS);
    // 〔B2〕「没人选过」（文件不在）说的就是缺省那两句 —— 它**不是**「读不出来」。
    expect(
      describeExitBehavior({ policy: "absent", killOnExit: false, detached: false }),
    ).toBe(EXIT_SELF_DIES);

    // ── ★ 「无人监护」只许出现在**真脱离且没勾**那一支。
    for (const [name, text] of [
      ["EXIT_KILLS", EXIT_KILLS],
      ["EXIT_SELF_DIES", EXIT_SELF_DIES],
    ] as const) {
      expect(
        text.includes("无人监护"),
        `${name} 里出现了「无人监护」—— 那一档**没有脱离**，backend 会在 monitor 退出后很快自行退出。\n` +
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
    // ⚠ `K-P3b KP3W4`③：人群扩到 `HEALTH_*` 四句 —— 它们与那三句同一条规矩，
    //   而 `K-P3` 交付时这一格只由 Rust 那侧的 `the_backend_policy_copy_has_exactly_one_home` 看着
    //   （那份 vitest 当时不在它的写区）。两侧各有一条不是重复：
    //   Rust 那条的人群含 `backend_control.rs`，这一条的人群是前端那两份。
    // 〔B2 · E3〕人群 +1：「读不出来」那一句（`设计/01 §3.3b ⑤`，三句变四句）。
    // 〔CP2b〕HEALTH_CRASHED / HEALTH_DETAIL 两个带占位符的模板不再是导出常量（由 copyText 填），直接取表里那一格的原文。
    const tableZh = (key: string): string =>
      (COPY_TABLE.entries as Record<string, { zh: string }>)[key]!.zh;
    const literals = [
      EXIT_KILLS,
      EXIT_UNATTENDED,
      EXIT_SELF_DIES,
      EXIT_UNREADABLE,
      HEALTH_UNKNOWN,
      HEALTH_CLEAN,
      tableZh("backendPolicy.health.crashed"),
      HEALTH_LAST_MISSING,
      // 〔ST2 · 步 6〕长的那一半挪进 ⓘ / `[详情]` 之后多出来的两句，同一条规矩。
      HEALTH_UNKNOWN_WHY,
      tableZh("backendPolicy.health.detail"),
    ];
    for (const lit of literals) {
      const homes = files.filter((f) => visibleOf(f.src).includes(lit)).map((f) => f.name);
      expect(
        homes,
        `「${lit.slice(0, 16)}…」出现在 ${homes.length} 个文件里：${homes.join(" / ")}\n` +
          "★ 那几句的唯一一个家是文案表（table.json）。抄进别处 = 下一次只改一处。",
      ).toEqual([TABLE_REL]);
    }
    // 反过来：这一区必须**真的在用**那个家，而不是自己拼一份。
    const section = files.find((f) => f.name === "backend-section.ts")!.src;
    expect(
      section.includes("describeExitBehavior"),
      "`backend-section.ts` 不再调 `describeExitBehavior` —— 那它的文案是从哪来的？",
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
    ).toContain(TABLE_REL); // 〔CP2b〕家是表；backend-policy.ts 里那行单行 JSDoc 也提到它（注释，不是第二个家）
  });

  it("本机永远在第一行——它不是另一种机器，只是不走 ssh 的那一台", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const rows = [...s.element.querySelectorAll<HTMLElement>(".backend-row")];
    expect(rows.map((r) => r.dataset.origin)).toEqual([LOCAL_ORIGIN, "甲机"]);
  });

  it("每台机各查各的状态", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const asked = calls
      .filter((c) => c.name === "backend_status")
      .map((c) => (c.args as { origin: string }).origin);
    expect(asked.sort()).toEqual([LOCAL_ORIGIN, "甲机"].sort());
    expect(s.element.querySelector(".backend-row-state")?.textContent).toContain("已连上");
  });

  it("★ 那一行说的话跟着后端的 `detached` 走 —— 脱离了就说「无人监护」", async () => {
    status = { channel: true, pid: 42, detached: true };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const exit = s.element.querySelector<HTMLElement>(".backend-row-exit");
    expect(exit?.textContent).toBe(EXIT_UNATTENDED);
    expect(exit?.dataset.detached).toBe("true");
  });

  it("★★ 后端**没给** `detached`（旧后端 / 远端恒 null）⇒ 按「没脱离」算，说今天那句", async () => {
    // ⚠ 这一格是**负例**，而它是这条判据的重量所在：
    //   只有正例的话，把 `st.detached === true` 换成一个恒真表达式也照样绿 ——
    //   那正是 `P2d §0a` 翻掉的 `SSH_CONNECTION` 那一形（假信号不报错，它只是一直说是）。
    status = { channel: true, pid: 42 };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const exit = s.element.querySelector<HTMLElement>(".backend-row-exit");
    expect(exit?.textContent).toBe(EXIT_SELF_DIES);
    expect(exit?.dataset.detached).toBe("false");
    expect(
      exit?.textContent?.includes("无人监护"),
      "后端没说它脱离了，界面却替它说了「无人监护」—— 那是一句没有依据的话",
    ).toBe(false);
  });

  it("★ 机器清单问后端要，不自己算（A5：前端自己拼会与 Rust 的 origin 分叉四处）", async () => {
    new BackendSection({ headless: true });
    await flush();
    await flush();
    expect(
      calls.some((c) => c.name === "backend_machines"),
      "没调 backend_machines —— 前端又在自己拼清单了。\n" +
        "Rust 侧会对重复 label 后缀化（pi → pi (#2)）、会按 enabled 过滤、启动后新增的不注册；\n" +
        "自己拼出来的名字对不上注册表，那些行的起/停恒回「没有这台机的把手」。",
    ).toBe(true);
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const src = readFileSync(resolve(srcDirOf(__dirname), "backend-section.ts"), "utf8");
    expect(
      src.split("hostKey(").length - 1,
      "backend-section.ts 里又出现了 hostKey( —— 那正是自己拼 origin 的做法",
    ).toBe(0);
  });

  it("★ 起完之后轮询到落定，不画一张操作前的快照（A4）", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    // 起：命令返回时后端还没起来（channel:false），第三次才连上。
    statusQueue = [
      { channel: false, pid: null },
      { channel: false, pid: null },
      { channel: true, pid: 7 },
    ];
    const btns = [...s.element.querySelectorAll<HTMLButtonElement>(".backend-row button")];
    btns[0].click();
    await new Promise((r) => setTimeout(r, 400));
    expect(
      s.element.querySelector(".backend-row-state")?.textContent,
      "起完只画了一次就停手 —— 那张是操作前的快照（backend_start 只是 spawn 了监护线程就返回）",
    ).toContain("已连上");
  });

  it("★★ K-P3b：读数**另起一行**画出来，而退出那一行一个字节不变", async () => {
    // ⚠ 桩里那四个计数与 `last` 就是后端 `backend_status` 那一格的形状
    //   （键名与 Rust 侧 `Health` 逐格对齐：crashed / refused / neverStarted / misread / last）。
    const health = { crashed: 2, refused: 0, neverStarted: 1, misread: 3, last: "甲那一行" };
    status = { channel: true, pid: 42, detached: true, health };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const row = s.element.querySelector<HTMLElement>(".backend-row")!;
    expect(
      row.querySelector<HTMLElement>(".backend-row-health")?.textContent,
      "读数那一行画的不是 `describeBackendHealth(那份)` —— 界面上那句话与纯函数分叉了",
    ).toBe(describeBackendHealth(health));
    // ★ 同一拍里退出那一行仍然**等于**它自己那句 —— 读数没被接在它后面。
    //   把读数接到 `paintExit` 的串后面 ⇒ 本格与 `:116-123` 那四格一起红。
    expect(
      row.querySelector<HTMLElement>(".backend-row-exit")?.textContent,
      "退出那一行被改了 —— 读数是**另一句话**，接上去就把那四根等号一起拽红了",
    ).toBe(
      describeExitBehavior({ policy: "absent", killOnExit: false, detached: true }),
    );
  });

  it("★★ K-P3b：后端**没给** `health`（旧后端）⇒ 说「答不出来」，不说「没崩过」", async () => {
    // ⚠ 这一格是**负例**，方向与上面 `detached` 缺席那一格一致：
    //   缺席时替后端补一个「四个 0」的读数 ⇒ `describeBackendHealth` 会说「一次都没崩过」，
    //   而 `K-P3 §0-1` 逐字：「今天不是『它没崩过』，是『没有任何东西在记它崩没崩』……
    //   这两句话差得很远，不许混用」。
    status = { channel: true, pid: 42 };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const el = s.element.querySelector<HTMLElement>(".backend-row-health");
    expect(
      el?.textContent,
      "后端没给这一格，界面却说出了一个读数 —— 那个读数没有依据",
    ).toBe(HEALTH_UNKNOWN);
    expect(
      el?.textContent === "",
      "读数那一行是空的 —— 缺席不是「没什么可说」，缺席正是「答不出来」这句话本身",
    ).toBe(false);
  });

  it("★ 存不下就把勾回退——屏上写着 A 而实际是 B 比报错更坏", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    failNextSet = new Error("盘满了");
    const box = s.element.querySelector<HTMLInputElement>(".backend-row-kill input")!;
    expect(box.checked).toBe(false);
    box.checked = true;
    box.onchange?.(new Event("change"));
    await flush();
    await flush();
    expect(box.checked, "存失败了勾还留在新位置 —— 界面在骗人").toBe(false);
  });
  it("★★ 〔B2 · E3〕「读不出来」是第四句，它不是「选了默认」—— 而且它不看 `killOnExit`", () => {
    for (const detached of [false, true]) {
      for (const killOnExit of [false, true]) {
        expect(
          describeExitBehavior({
            policy: "unreadable",
            killOnExit,
            detached,
          }),
          "读不出来那一档说成了别的 —— `§3.3b ⑤`：「读不出来」与「用户选了默认」不是一回事，不许合并成一句",
        ).toBe(EXIT_UNREADABLE);
      }
    }
    expect(EXIT_UNREADABLE.includes("这不等于有人这么选过")).toBe(true);
    expect(
      EXIT_UNREADABLE.includes("无人监护"),
      "读不出来那一句承诺了常驻 —— 它不知道那台机器会怎样",
    ).toBe(false);
    const four = [EXIT_KILLS, EXIT_UNATTENDED, EXIT_SELF_DIES, EXIT_UNREADABLE];
    expect(
      new Set(four).size,
      "那四句里有两句一模一样 —— 两个状态被说成了一句",
    ).toBe(4);
  });

  it("★★ 〔S5 · V105 清账〕十二格逐格穷举：每一格都是那四句之一（「不适用」那一支已删）", () => {
    // 原来这里是判据 E4：折进前端那一档回「不适用」。那一档已放弃（`99 §1` V105），
    // 那一支与这条判据同拍删掉。留下的是它的反向那一半 —— 任何一格都得落在四句里，
    // 不许有第五种答案（`null` / 空串 / 别的句子）。
    const four = [EXIT_KILLS, EXIT_UNATTENDED, EXIT_SELF_DIES, EXIT_UNREADABLE];
    let cells = 0;
    for (const policy of ["chosen", "absent", "unreadable"] as const) {
      for (const killOnExit of [false, true]) {
        for (const detached of [false, true]) {
          const said = describeExitBehavior({ policy, killOnExit, detached });
          cells++;
          expect(
            four.includes(said),
            `（${policy}/${killOnExit}/${detached}）说了一句四句之外的话：${String(said)}`,
          ).toBe(true);
        }
      }
    }
    expect(cells, "穷举的格数不对 —— 循环坏了").toBe(12);
  });

  it("★★ 〔B2〕勾的值**问后端要**：盘上选过 true ⇒ 勾上、说「会结束它」", async () => {
    exitAnswer = {
      state: "chosen",
      killOnExit: true,
      reason: null,
      path: "x",
    };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const row = s.element.querySelector<HTMLElement>(".backend-row")!;
    const box = row.querySelector<HTMLInputElement>(".backend-row-kill input")!;
    expect(box.disabled).toBe(false);
    expect(
      box.checked,
      "后端说选过 true，勾却没勾上 —— 画的不是那台机器上的值",
    ).toBe(true);
    expect(
      row.querySelector<HTMLElement>(".backend-row-exit")?.textContent,
    ).toBe(EXIT_KILLS);
    const asked = calls
      .filter((c) => c.name === "backend_exit_policy")
      .map((c) => (c.args as { origin: string }).origin);
    expect(asked.sort(), "每台机都该各问各的").toEqual(
      [LOCAL_ORIGIN, "甲机"].sort(),
    );
  });

  it("★★ 〔B2〕读不出来 ⇒ 说第四句；问不到 ⇒ 勾禁用、那一行一个字都不说", async () => {
    exitAnswer = {
      state: "unreadable",
      killOnExit: false,
      reason: "不是 JSON",
      path: "x",
    };
    let s = new BackendSection({ headless: true });
    await flush();
    await flush();
    let row = s.element.querySelector<HTMLElement>(".backend-row")!;
    expect(
      row.querySelector<HTMLElement>(".backend-row-exit")?.textContent,
    ).toBe(EXIT_UNREADABLE);
    // 问不到（没连上 / 旧后端不认那条命令）：不替那台机器说话，也不画一个看起来能用的勾。
    exitAnswer = null;
    s = new BackendSection({ headless: true });
    await flush();
    await flush();
    row = s.element.querySelector<HTMLElement>(".backend-row")!;
    const box = row.querySelector<HTMLInputElement>(".backend-row-kill input")!;
    expect(box.disabled, "问不到那台机器的值，勾却能点 —— 点了发到哪去？").toBe(
      true,
    );
    const exit = row.querySelector<HTMLElement>(".backend-row-exit")!;
    expect(exit.textContent, "问不到还说了一句 —— 那句话没有依据").toBe("");
    expect(exit.dataset.exit).toBe("unasked");
  });

  it("★★ 〔B2〕改勾交后端写，画的是**写完读回来**的那一份", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const row = s.element.querySelector<HTMLElement>(".backend-row")!;
    const box = row.querySelector<HTMLInputElement>(".backend-row-kill input")!;
    box.checked = true;
    box.onchange?.(new Event("change"));
    await flush();
    await flush();
    await flush();
    const sets = calls.filter((c) => c.name === "set_backend_exit_policy");
    expect(
      sets.map((c) => c.args),
      "改勾没交后端写",
    ).toEqual([{ origin: LOCAL_ORIGIN, kill: true }]);
    expect(
      row.querySelector<HTMLElement>(".backend-row-exit")?.textContent,
    ).toBe(EXIT_KILLS);
    expect(box.checked).toBe(true);
  });
  it("★ 〔ST2〕那一行有 [起][停]、状态照实说「已连上」", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const row = s.element.querySelector<HTMLElement>(".backend-row")!;
    expect([...row.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["起", "停"]);
    expect(row.querySelector(".backend-row-state")?.textContent).toBe("已连上（pid 42）");
  });
});

describe("〔ST2 · 设计/70 第二刀 步 6〕后端开关表格式四栏：长文案进 ⓘ / [详情]", () => {
  it("★ 每一行恰好四格、顺序与表头一致（状态 / 操作 / 退出行为 / 健康）", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const want = BACKEND_COLUMNS().map(([c]) => c);
    expect(want).toEqual(["state", "ops", "exit", "health"]);
    const head = s.element.querySelector<HTMLElement>('[data-backend-columns="head"]')!;
    expect([...head.children].map((c) => (c as HTMLElement).dataset.col)).toEqual(want);
    expect([...head.children].map((c) => c.textContent)).toEqual(["状态", "操作", "退出行为", "健康"]);
    const rows = [...s.element.querySelectorAll<HTMLElement>(".backend-row")];
    expect(rows.length, "一行都没有 —— 下面的逐行比在空人群上恒绿").toBe(2);
    for (const r of rows) {
      const cells = r.querySelector<HTMLElement>("[data-backend-cells]")!;
      expect(cells.dataset.backendCells).toBe(r.dataset.origin);
      expect([...cells.children].map((c) => (c as HTMLElement).dataset.col)).toEqual(want);
      // 控件各归各格：按钮在「操作」、勾在「退出行为」、读数在「健康」。
      expect(cells.querySelector('[data-col="ops"]')!.querySelectorAll("button").length).toBe(2);
      expect(cells.querySelector('[data-col="exit"] .backend-row-kill')).not.toBeNull();
      expect(cells.querySelector('[data-col="health"] .backend-row-health')).not.toBeNull();
    }
  });

  it("★★ 无记录 ⇒ 格子里只写「— 无记录」，那条区分进 ⓘ（`§2.2`：只换位置，不删义）", async () => {
    status = { channel: true, pid: 42 };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const col = s.element.querySelector<HTMLElement>('.backend-row [data-col="health"]')!;
    expect(col.querySelector(".backend-row-health")?.textContent).toBe(HEALTH_UNKNOWN);
    const why = col.querySelector<HTMLElement>('[data-health-extra="why"]');
    expect(why, "无记录那一格没有 ⓘ —— 「无记录 ≠ 没崩过」那条区分被一起扫掉了").not.toBeNull();
    expect(why!.getAttribute("aria-label")).toBe(HEALTH_UNKNOWN_WHY);
    expect(HEALTH_UNKNOWN_WHY).toContain("不等于「没崩过」");
    expect(col.querySelector('[data-health-extra="detail"]'), "无记录却给了 [详情]").toBeNull();
  });

  it("★★ 崩过 ⇒ 格子里一句短话 ＋ [详情] 分开列四个计数；账行 / markdown 一个都不上屏", async () => {
    const health = { crashed: 4, refused: 1, neverStarted: 0, misread: 2, last: "崩了，exit -1073741819" };
    status = { channel: true, pid: 42, health };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const col = s.element.querySelector<HTMLElement>('.backend-row [data-col="health"]')!;
    expect(col.querySelector(".backend-row-health")?.textContent).toBe(
      "⚠ 崩过 4 次 · 最后一次：崩了，exit -1073741819",
    );
    const more = col.querySelector<HTMLElement>('[data-health-extra="detail"]')!;
    expect(more.tagName).toBe("DETAILS");
    expect(more.querySelector("summary")?.textContent).toBe("详情");
    expect(more.textContent).toContain(describeHealthDetail(health)!);
    expect(col.querySelector('[data-health-extra="why"]'), "有记录还挂着「无记录」的 ⓘ").toBeNull();
    // `70 §2.1` 那五种里后端曾经带进来的三种：markdown · 日志行格式 · 设计论证。
    expect(col.textContent).not.toMatch(/\*\*|\[死亡账\]|origin=|下一步：|放大器/);
  });

  it("★ 重画很多遍（起完轮询到落定）⇒ 附件不累积：始终恰好一个", async () => {
    status = { channel: true, pid: 42, health: { crashed: 1, refused: 0, neverStarted: 0, misread: 0, last: "崩了，exit 3" } };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    statusQueue = [{ ...status, channel: false }, { ...status, channel: false }];
    s.element.querySelector<HTMLButtonElement>('[data-col="ops"] button')!.click();
    await new Promise((r) => setTimeout(r, 400));
    const col = s.element.querySelector<HTMLElement>('.backend-row [data-col="health"]')!;
    expect(col.querySelectorAll("[data-health-extra]").length).toBe(1);
  });
});

describe("〔ST2〕（原 describe 的收尾占位，保持文件结构）", () => {
  it("详情那一段按四个计数分开填，占位符全被填掉", () => {
    const d = describeHealthDetail({ crashed: 1, refused: 2, neverStarted: 3, misread: 4, last: null })!;
    expect(d).toContain("崩了 1 次");
    expect(d).toContain("被拒 2 次");
    expect(d).toContain("没起来 3 次");
    expect(d).toContain("读坏了 4 次");
    expect(d).not.toMatch(/\{\w+\}/);
    expect(describeHealthDetail({ crashed: 0, refused: 0, neverStarted: 0, misread: 0, last: null })).toBeNull();
  });
});
