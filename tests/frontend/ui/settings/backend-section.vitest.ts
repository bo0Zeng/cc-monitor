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
 * 后端 `exit-policy-read` 回的那一份（值住后端那台机器上）。`null` ⇒ 这台问不到（命令抛错）。
 * 键名与后端 `exit_policy::wire` 逐格一致：`state` / `killOnExit`（`reason` / `path` 本区不用）。
 */
let exitAnswer: Record<string, unknown> | null = null;
/** 下一次「交后端写」要回的失败（`null` = 照常写）。原先用 `spyOn(commands.set_backend_exit_policy)`，那条命令退役了。 */
let failNextSet: Error | null = null;
/** `backend_stop` 这一趟回的结局。 */
let stopAnswer: { stopped: "graceful" | "killed" | "not_running"; pid: number | null } = { stopped: "graceful", pid: 42 };
/** `backend-log` 那一问各台答什么（按 origin；缺 ⇒ 通道失败）。 */
let logAnswers: Record<string, unknown> = {};

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
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
      return Promise.resolve(stopAnswer);
    },
    local_ccm_entry_status: (fresh?: boolean) => {
      calls.push({ name: "local_ccm_entry_status", args: fresh });
      return Promise.resolve({ ok: true, summary: "S", message: "" });
    },
    backend_machines: () => {
      calls.push({ name: "backend_machines", args: null });
      return Promise.resolve(["<local>", "甲机"]);
    },
    // 「退出行为」两问改走通道：`chan_call`（op = `exit-policy-read` / `exit-policy-set`）。
    //   这里把一发 `chan_call` 译回判据里的旧叫法（`backend_exit_policy` / `set_backend_exit_policy`），
    //   回包译成后端那份字节（`ArrayBuffer`），问不到译成通道那一跳「没有控制通道」的线上形状。
    chan_call: (a: { origin: string; op: string; payload: number[] }) => {
      const body = JSON.parse(new TextDecoder().decode(Uint8Array.from(a.payload))) as Record<string, unknown>;
      const bytes = (v: unknown) => {
        const u = new TextEncoder().encode(JSON.stringify(v));
        return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
      };
      const noChannel = { err: { Hop: { idx: 1, tag: "open", reach: "NotSent", why: "Unreachable" } }, body: [] };
      if (a.op === "resync") {
        calls.push({ name: "resync", args: { origin: a.origin, body } });
        return bytes({ added: 0, removed: 0, retagged: 1, caught_up: 0, watchers: 1, unavailable: [], uncancellable: [] });
      }
      if (a.op === "backend-log") {
        calls.push({ name: "backend_log", args: { origin: a.origin } });
        return a.origin in logAnswers ? bytes(logAnswers[a.origin]) : Promise.reject(noChannel);
      }
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
        // 后端写完**读回**的那一份：这里就让「盘上」变成写进去的值，后续每一次现问都读到它。
        exitAnswer = { state: "chosen", killOnExit: kill, reason: null, path: "x", said: kill ? EXIT_KILLS : EXIT_SELF_DIES };
        return bytes(exitAnswer);
      }
      return Promise.reject(new Error(`判据没料到的通道问法：${a.op}`));
    },
  },
}));

vi.mock("../../../../src/frontend/ui/remote-config", () => ({
  readRemoteConfig: () => Promise.resolve({ hosts: [{ label: "甲机", host: "a" }] }),
  hostKey: (h: { label: string; host: string }) => h.label.trim() || h.host,
}));

vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: () => {} }));
// 「重新对齐」做完经 Tauri 事件通知主窗口；这里没有 Tauri 运行时 ⇒ 换成空的 emit。
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async () => {}), listen: vi.fn(async () => () => {}) }));

import { BACKEND_COLUMNS, BackendSection, decodeHealthFace, readBackendLog, stopSaid, stopWarning } from "../../../../src/frontend/ui/settings/backend-section";
import type { SessionAccount } from "../../../../src/frontend/ui/accounts";
import { srcDirOf } from "../../../test-support/repo-root";
import COPY_TABLE from "../../../../src/shared/copy/table.json";
// 「健康」那一格的成品金样：Rust 侧由生产的 `health_face` 现产、逐格相等（`backend_policy_tests.rs`），这里读同一份。
import HEALTH_GOLDEN from "../../../__fixtures__/backend-health.golden.json";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/backend-policy";

// 那四句由后端出成品（`exit-policy-read` 的 `said`，判定与十格穷举住 `exit_policy_tests.rs`）；
//   这里只取表里的原文当桩里后端回的那一句，判界面原样摆、不再判。
const tableZhOf = (key: string): string => (COPY_TABLE.entries as Record<string, { zh: string }>)[key]!.zh;
const EXIT_KILLS = tableZhOf("backendPolicy.exit.kills");
const EXIT_SELF_DIES = tableZhOf("backendPolicy.exit.selfDies");
const EXIT_UNATTENDED = tableZhOf("backendPolicy.exit.unattended");
const EXIT_UNREADABLE = tableZhOf("backendPolicy.exit.unreadable");

type GoldenFace = { state: string; summary: string; why: string | null; detail: string | null };
/** 金样里那几形的成品，按名字取（名字就是金样里的 `name`）。 */
const FACE: Record<string, GoldenFace> = Object.fromEntries(
  (HEALTH_GOLDEN.cases as { name: string; face: GoldenFace }[]).map((c) => [c.name, c.face]),
);

/**
 * 人群：这一区的用户可见文案今天住在哪几个文件里。**扩人群是翻转的一半。**
 * 家搬进了文案表 ⇒ 人群 +1（`table.json`），而「唯一一个家」从 `backend-policy.ts` 换成表。
 */
const TABLE_REL = "../../../shared/copy/table.json";
const COPY_POPULATION = ["backend-section.ts", "../backend-policy.ts", TABLE_REL] as const;

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
  stopAnswer = { stopped: "graceful", pid: 42 };
  exitAnswer = { state: "absent", killOnExit: false, reason: null, path: "x", said: EXIT_SELF_DIES };
  // `backend_status` 今天恒带 `health` 成品（远端那一格恒是「无记录」）；缺它的那一形单列一格判。
  status = { channel: true, pid: 42, health: FACE["无记录"] };
  statusQueue = [];
});

describe("P2s backend 开关区", () => {
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
    // 人群 +1：「读不出来」那一句（三句变四句）。
    // HEALTH_CRASHED / HEALTH_DETAIL 两个带占位符的模板不再是导出常量（由 copyText 填），直接取表里那一格的原文。
    const tableZh = (key: string): string =>
      (COPY_TABLE.entries as Record<string, { zh: string }>)[key]!.zh;
    // 健康那六句的取文口搬到了后端（`backend_policy.rs::health_face`，key 归 `rsBackendPolicy.health.*`）——
    //   前端这两份里更不许出现它们的原文（界面只排版后端给的成品）。
    const literals = [
      EXIT_KILLS,
      EXIT_UNATTENDED,
      EXIT_SELF_DIES,
      EXIT_UNREADABLE,
      tableZh("rsBackendPolicy.health.unknown"),
      tableZh("rsBackendPolicy.health.clean"),
      tableZh("rsBackendPolicy.health.crashed"),
      tableZh("rsBackendPolicy.health.lastMissing"),
      // 长的那一半挪进 ⓘ / `[详情]` 之后多出来的两句，同一条规矩。
      tableZh("rsBackendPolicy.health.unknownWhy"),
      tableZh("rsBackendPolicy.health.detail"),
    ];
    for (const lit of literals) {
      const homes = files.filter((f) => visibleOf(f.src).includes(lit)).map((f) => f.name);
      expect(
        homes,
        `「${lit.slice(0, 16)}…」出现在 ${homes.length} 个文件里：${homes.join(" / ")}\n` +
          "★ 那几句的唯一一个家是文案表（table.json）。抄进别处 = 下一次只改一处。",
      ).toEqual([TABLE_REL]);
    }
    // 反过来：这一区摆的是**后端的成品**（`said`），不是自己拼一份。
    const section = files.find((f) => f.name === "backend-section.ts")!.src;
    expect(visibleOf(section).includes("answer.said"), "`backend-section.ts` 不摆后端的 `said` —— 那它的文案是从哪来的？").toBe(true);
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
    ).toContain(TABLE_REL); // 家是表；backend-policy.ts 里那行单行 JSDoc 也提到它（注释，不是第二个家）
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

  it("★ 〔MIG-2 · ㊴〕那一行原样摆后端的 `said`：常驻那台说「无人监护」，与 monitor 的 `detached` 无关", async () => {
    status = { channel: true, pid: 42, detached: false };
    exitAnswer = { state: "absent", killOnExit: false, reason: null, path: "x", said: EXIT_UNATTENDED };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const exit = s.element.querySelector<HTMLElement>(".backend-row-exit");
    expect(exit?.textContent).toBe(EXIT_UNATTENDED);
  });

  it("★★ 〔㊴〕后端**没给** `said`（旧后端）/ 给了空串 ⇒ 当问不到：勾禁用、那一行不替它说话", async () => {
    for (const said of [undefined, ""]) {
      exitAnswer = { state: "absent", killOnExit: false, reason: null, path: "x", ...(said === undefined ? {} : { said }) };
      const s = new BackendSection({ headless: true });
      await flush();
      await flush();
      const exit = s.element.querySelector<HTMLElement>(".backend-row-exit")!;
      expect(exit.textContent, `said=${String(said)} 时界面替后端说了一句`).toBe("");
      expect(exit.dataset.exit).toBe("unasked");
      expect(s.element.querySelector<HTMLInputElement>(".backend-row-kill input")!.disabled).toBe(true);
    }
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
    // 桩里那一格就是后端 `backend_status` 的 `health` 成品（金样「崩过」那一形）。
    const health = FACE["崩过"];
    status = { channel: true, pid: 42, detached: true, health };
    exitAnswer = { state: "absent", killOnExit: false, reason: null, path: "x", said: EXIT_UNATTENDED };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const row = s.element.querySelector<HTMLElement>(".backend-row")!;
    expect(
      row.querySelector<HTMLElement>(".backend-row-health")?.textContent,
      "读数那一行画的不是后端给的那一句 —— 界面在自己说话",
    ).toBe(health.summary);
    // ★ 同一拍里退出那一行仍然**等于**它自己那句 —— 读数没被接在它后面。
    //   把读数接到 `paintExit` 的串后面 ⇒ 本格与 `:116-123` 那四格一起红。
    expect(
      row.querySelector<HTMLElement>(".backend-row-exit")?.textContent,
      "退出那一行被改了 —— 读数是**另一句话**，接上去就把那四根等号一起拽红了",
    ).toBe(EXIT_UNATTENDED);
  });

  it("★★ 〔PB1 · P5〕`health` 缺席 / 形状不对 ⇒ 只在那一格说「格式不对」，不替后端编一档，状态格照画", async () => {
    // ⚠ 原来缺席 ⇒ 画「— 无记录」—— 那是一条**前端的回落判定**（缺格当无记录）。
    //   `backend_status` 是 monitor 自己的命令、与界面同一个构建，缺格只能是程序错 ⇒ 说出来（D7 / D11），
    //   更不许补一个「四个 0」去让谁判出「没崩过」（「答不出来」与「没崩过」不许混用）。
    const badShape = (COPY_TABLE.entries as Record<string, { zh: string }>)["backend.health.badShape"]!.zh;
    const unknown = FACE["无记录"]!;
    for (const [what, health] of [
      ["缺席", undefined],
      ["旧形状（四个计数）", { crashed: 0, refused: 0, neverStarted: 0, misread: 0, last: null }],
      ["多一格", { ...unknown, crashed: 0 }],
      ["少一格", { state: unknown.state, summary: unknown.summary, why: unknown.why }],
      ["summary 不是串", { ...unknown, summary: 3 }],
      ["summary 是空串", { ...unknown, summary: "" }],
      ["why 是空串", { ...unknown, why: "" }],
      ["detail 不是串也不是 null", { ...unknown, detail: false }],
      ["state 是空串", { ...unknown, state: "" }],
    ] as const) {
      status = health === undefined ? { channel: true, pid: 42 } : { channel: true, pid: 42, health };
      const s = new BackendSection({ headless: true });
      await flush();
      await flush();
      const col = s.element.querySelector<HTMLElement>('.backend-row [data-col="health"]')!;
      const el = col.querySelector<HTMLElement>(".backend-row-health")!;
      expect(el.textContent, `「${what}」没说格式不对`).toBe(badShape);
      expect(el.dataset.health, `「${what}」还挂着一档状态`).toBeUndefined();
      expect(col.querySelectorAll("[data-health-extra]").length, `「${what}」还挂着 ⓘ / [详情]`).toBe(0);
      expect(
        s.element.querySelector(".backend-row-state")?.textContent,
        `「${what}」把状态那一格也带走了 —— 失败该落在健康那一格上`,
      ).toBe("已连上（pid 42）");
      expect(decodeHealthFace(health), `解码器收下了「${what}」`).toBeNull();
    }
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
  it("★★ 〔B2〕勾的值**问后端要**：盘上选过 true ⇒ 勾上、说「会结束它」", async () => {
    exitAnswer = {
      state: "chosen",
      killOnExit: true,
      reason: null,
      path: "x",
      said: EXIT_KILLS,
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
      said: EXIT_UNREADABLE,
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
  it("★ 〔ST2〕那一行有 [起][停]〔GAP1〕[日志]〔RESYNC〕[重新对齐]、状态照实说「已连上」", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const row = s.element.querySelector<HTMLElement>(".backend-row")!;
    expect([...row.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["起", "停", "日志", "重新对齐"]);
    expect(row.querySelector(".backend-row-state")?.textContent).toBe("已连上（pid 42）");
  });
  // 〔「机器一行『重新对齐』（上面整套）」〕按一下 ⇒ 问**那一行那台**的后端 `resync`、整机（不带 sid）。
  it("★ 〔RESYNC〕[重新对齐] 问的是那一行那台的后端、整机", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const rows = [...s.element.querySelectorAll<HTMLElement>("[data-backend-cells]")];
    const ask = (i: number) => [...rows[i].querySelectorAll("button")].find((b) => b.textContent === "重新对齐")!.click();
    calls.length = 0;
    ask(1);
    ask(0);
    await flush();
    expect(calls.filter((c) => c.name === "resync").map((c) => c.args)).toEqual([
      { origin: rows[1].dataset.backendCells, body: {} },
      { origin: rows[0].dataset.backendCells, body: {} },
    ]);
  });
  /** 主会话 09-28 裁 FIX4 ⑥（手动兜底）：「本机 PATH 探针的 5 分钟缓存在『重新对齐』时作废，不再多等」。 */
  it("FIX4 ⑥：本机那一行 [重新对齐] 作废本机 ccm 那份缓存（`fresh`）；远端那一行不碰", async () => {
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const rows = [...s.element.querySelectorAll<HTMLElement>("[data-backend-cells]")];
    const ask = (o: string) =>
      [...rows.find((r) => r.dataset.backendCells === o)!.querySelectorAll("button")].find((b) => b.textContent === "重新对齐")!.click();
    calls.length = 0;
    ask("甲机");
    await flush();
    await flush();
    expect(calls.filter((c) => c.name === "local_ccm_entry_status")).toEqual([]);
    ask("<local>");
    await flush();
    await flush();
    expect(calls.filter((c) => c.name === "local_ccm_entry_status")).toEqual([{ name: "local_ccm_entry_status", args: true }]);
  });
});

describe("〔ST2 · 第二刀 步 6〕后端开关表格式四栏：长文案进 ⓘ / [详情]", () => {
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
      expect(cells.querySelector('[data-col="ops"]')!.querySelectorAll("button").length).toBe(4); // 起 · 停 ·日志 ·重新对齐
      expect(cells.querySelector('[data-col="exit"] .backend-row-kill')).not.toBeNull();
      expect(cells.querySelector('[data-col="health"] .backend-row-health')).not.toBeNull();
    }
  });

  it("★★ 无记录 ⇒ 格子里只写「— 无记录」，那条区分进 ⓘ（`§2.2`：只换位置，不删义）", async () => {
    const face = FACE["无记录"]!;
    status = { channel: true, pid: 42, health: face };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const col = s.element.querySelector<HTMLElement>('.backend-row [data-col="health"]')!;
    expect(col.querySelector(".backend-row-health")?.textContent).toBe(face.summary);
    expect(face.summary).toBe("— 无记录");
    const why = col.querySelector<HTMLElement>('[data-health-extra="why"]');
    expect(why, "无记录那一格没有 ⓘ —— 「无记录 ≠ 没崩过」那条区分被一起扫掉了").not.toBeNull();
    expect(why!.getAttribute("aria-label")).toBe(face.why);
    expect(face.why).toContain("不等于「没崩过」");
    expect(col.querySelector('[data-health-extra="detail"]'), "无记录却给了 [详情]").toBeNull();
  });

  it("★★ 崩过 ⇒ 格子里一句短话 ＋ [详情] 分开列四个计数；账行 / markdown 一个都不上屏", async () => {
    const face = FACE["崩过"]!;
    status = { channel: true, pid: 42, health: face };
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
    expect(more.querySelector(".settings-hint")?.textContent).toBe(face.detail);
    for (const n of ["崩了 4 次", "被拒 1 次", "没起来 0 次", "读坏了 2 次"]) expect(face.detail).toContain(n);
    expect(col.querySelector('[data-health-extra="why"]'), "有记录还挂着「无记录」的 ⓘ").toBeNull();
    // 那五种里后端曾经带进来的三种：markdown · 日志行格式 · 设计论证。
    expect(col.textContent).not.toMatch(/\*\*|\[死亡账\]|origin=|下一步：|放大器/);
  });

  it("★ 重画很多遍（起完轮询到落定）⇒ 附件不累积：始终恰好一个", async () => {
    status = { channel: true, pid: 42, health: FACE["崩过但最后一次没留住"] };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    statusQueue = [{ ...status, channel: false }, { ...status, channel: false }];
    s.element.querySelector<HTMLButtonElement>('[data-col="ops"] button')!.click();
    await new Promise((r) => setTimeout(r, 400));
    const col = s.element.querySelector<HTMLElement>('.backend-row [data-col="health"]')!;
    expect(col.querySelectorAll("[data-health-extra]").length).toBe(1);
  });

  it("★★ 〔PB1 · P3〕金样每一形：后端给什么就画什么 —— 一句 · 状态 · ⓘ 在不在 · [详情] 在不在，逐格相等", async () => {
    const cases = HEALTH_GOLDEN.cases as { name: string; face: GoldenFace }[];
    expect(cases.length, "金样不是四形 —— 下面的逐形比在缩水的人群上成立").toBe(4);
    expect(new Set(cases.map((c) => c.face.state)).size, "金样没盖全三档").toBe(3);
    for (const { name, face } of cases) {
      expect(decodeHealthFace(face), `解码器收不下金样「${name}」`).toEqual(face);
      status = { channel: true, pid: 42, health: face };
      const s = new BackendSection({ headless: true });
      await flush();
      await flush();
      const col = s.element.querySelector<HTMLElement>('.backend-row [data-col="health"]')!;
      const el = col.querySelector<HTMLElement>(".backend-row-health")!;
      expect(el.textContent, `「${name}」格子里那一句`).toBe(face.summary);
      expect(el.dataset.health, `「${name}」的界面状态`).toBe(face.state);
      const why = col.querySelector<HTMLElement>('[data-health-extra="why"]');
      expect(why?.getAttribute("aria-label") ?? null, `「${name}」的 ⓘ`).toBe(face.why);
      const more = col.querySelector<HTMLElement>('[data-health-extra="detail"] .settings-hint');
      expect(more?.textContent ?? null, `「${name}」的 [详情]`).toBe(face.detail);
    }
  });
});

// 原来这里是「详情那一段按四个计数分开填」（喂 TS 那份 `describeHealthDetail`）。
//   `[详情]` 那一句改由后端出（`backend_policy.rs::health_face`），TS 那份删了 ⇒ 这一组退役：
//   计数分开填 · 占位符填掉由 Rust 侧金样与三档逐格判据管，这里上面那条逐形画、逐格比。

/**
 * 〔主会话裁 HX1 拍板项 3〕**停本机后端之前数一数走本机中转的活会话，>0 就先问一句、说几条会断**。
 * 守的要求：主会话 D-f 逐字「停后端时有走中转的活会话 ⇒ 先确认（说几条会断）」；「设置页『停』前 >0 就确认，说几条会断」
 * ＋ 用 `ask-dialog.ts::askConfirm`（真 app 里 `window.confirm` 从来不拦）。形状：话按表逐格相等；接线两向（答否 ⇒ 零次 `backend_stop` ·
 * 答是 ⇒ 恰好一次；一条都没有 ⇒ 不问；问不到 ⇒ 照样问；远端 ⇒ 数那台、照样问（后远端中转住那台常驻后端里）。
 */
describe("〔HX1 · D-f〕停后端之前数走中转的会话", () => {
  const row = (o: Partial<SessionAccount>): SessionAccount => ({
    pid: 1,
    sessionId: "s",
    cwd: null,
    configDir: null,
    account: null,
    bare: false,
    alive: true,
    launchId: null,
    viaRelay: false,
    ...o,
  });
  const zh = (k: string, args: Record<string, number> = {}) =>
    (COPY_TABLE.entries as Record<string, { zh: string }>)[k].zh.replace(/\{(\w+)\}/g, (_m, n: string) => String(args[n]));

  it("stopWarning 逐格：问不到照样问 · 确定几条 · 说不清的一起说 · 一条都没有不问 · 死会话不算", () => {
    expect(stopWarning(null)).toBe(zh("backend.stop.relayUnknown"));
    expect(stopWarning([])).toBeNull();
    expect(stopWarning([row({ viaRelay: false }), row({ alive: false, viaRelay: true })])).toBeNull();
    expect(stopWarning([row({ viaRelay: true }), row({ viaRelay: true }), row({ viaRelay: false })])).toBe(
      zh("backend.stop.relayConfirm", { n: 2 }),
    );
    expect(stopWarning([row({ viaRelay: true }), row({ viaRelay: null }), row({ viaRelay: undefined })])).toBe(
      zh("backend.stop.relayMaybe", { n: 1, k: 2 }),
    );
    expect(zh("backend.stop.relayConfirm", { n: 2 })).toContain("2 条");
  });

  const stopOf = (s: BackendSection, origin: string) => {
    const r = s.element.querySelector<HTMLElement>(`.backend-row[data-origin="${origin}"]`);
    const b = r ? [...r.querySelectorAll<HTMLButtonElement>("button")] : [];
    return b.find((x) => x.textContent === zh("backend.buildCells.stop"));
  };
  const until = async (ok: () => boolean) => {
    for (let i = 0; i < 100 && !ok(); i++) await new Promise((r) => setTimeout(r, 10));
  };
  const stops = () => calls.filter((c) => c.name === "backend_stop").length;

  it("接线：答否不停 · 答是停一次 · 没有走中转的不问 · 远端数那台", async () => {
    let asked: string[] = [];
    let answer = false;
    let rows: SessionAccount[] | null = [row({ viaRelay: true }), row({ viaRelay: true })];
    let sessionsAsked: string[] = [];
    const s = new BackendSection({
      headless: true,
      confirm: (m) => {
        asked.push(m);
        return answer;
      },
      sessions: (o) => {
        sessionsAsked.push(o);
        return Promise.resolve(rows);
      },
    });
    await flush();
    await flush();
    const localStop = stopOf(s, LOCAL_ORIGIN);
    expect(localStop, "本机那一行的「停」找不到 —— 下面整段空转").toBeTruthy();
    // 答否 ⇒ 问了、说了几条、没停。
    localStop!.click();
    await until(() => asked.length === 1);
    await flush();
    expect(asked).toEqual([zh("backend.stop.relayConfirm", { n: 2 })]);
    expect(stops(), "答了否还是停了").toBe(0);
    await until(() => !localStop!.disabled);
    // 答是 ⇒ 停一次。
    answer = true;
    localStop!.click();
    await until(() => stops() === 1);
    expect(stops()).toBe(1);
    await until(() => !localStop!.disabled);
    // 一条都没有 ⇒ 不问、直接停。
    asked = [];
    rows = [row({ viaRelay: false })];
    localStop!.click();
    await until(() => stops() === 2);
    expect(asked, "没有走中转的会话也问了").toEqual([]);
    await until(() => !localStop!.disabled);
    // 远端 ⇒ 数那台的会话、有走中转的就问；答否不停。
    sessionsAsked = [];
    asked = [];
    answer = false;
    rows = [row({ viaRelay: true })];
    const before = stops();
    const remoteStop = stopOf(s, "甲机");
    expect(remoteStop).toBeTruthy();
    remoteStop!.click();
    await until(() => asked.length === 1);
    await flush();
    expect(sessionsAsked, "远端的「停」数的不是那台").toEqual(["甲机"]);
    expect(asked).toEqual([zh("backend.stop.relayConfirm", { n: 1 })]);
    expect(stops(), "远端答了否还是停了").toBe(before);
  });
});

// T6「停」的结局说出来：`{stopped: graceful | killed | not_running}` 三个词各一句，落在那一行上（不只进 console）。
// 守的要求：`4d-lanes.md` `### STOP`逐字「monitor 发一次远端 exec、按结局出声」· 「强杀 / 没停掉出声」。
describe("〔STOP〕停的结局在机器页那一行说一句", () => {
  const zh = (k: string, args: Record<string, string> = {}) =>
    (COPY_TABLE.entries as Record<string, { zh: string }>)[k].zh.replace(/\{(\w+)\}/g, (_m, n: string) => String(args[n]));
  it("stopSaid 三个词三句、互不相同、带 pid；没在跑那句不带", () => {
    const g = stopSaid({ stopped: "graceful", pid: 7 });
    const k = stopSaid({ stopped: "killed", pid: 7 });
    const n = stopSaid({ stopped: "not_running", pid: null });
    expect([g, k, n]).toEqual([
      zh("backend.stopSaid.graceful", { pid: "7" }),
      zh("backend.stopSaid.killed", { pid: "7" }),
      zh("backend.stopSaid.notRunning"),
    ]);
    expect(new Set([g, k, n]).size).toBe(3);
    expect(g).toContain("7");
    expect(k).toContain("7");
  });

  it("接线：点「停」⇒ 那一行说后端回的那个结局；再点一次换成新结局", async () => {
    const s = new BackendSection({ headless: true, confirm: () => true, sessions: () => Promise.resolve([]) });
    await flush();
    await flush();
    const r = s.element.querySelector<HTMLElement>(`.backend-row[data-origin="${LOCAL_ORIGIN}"]`);
    const said = () => r?.querySelector<HTMLElement>("[data-stop-said]")?.textContent ?? null;
    const stop = [...(r?.querySelectorAll<HTMLButtonElement>("button") ?? [])].find(
      (x) => x.textContent === zh("backend.buildCells.stop"),
    );
    expect(stop, "本机那一行的「停」找不到").toBeTruthy();
    expect(said(), "没停过就该空着").toBe("");
    // 停之后状态落到「没连上」⇒ 轮询当场落定、按钮放开（否则第二下点在禁用的按钮上）。
    status = { ...status, channel: false };
    for (const a of [
      { stopped: "killed" as const, pid: 9 },
      { stopped: "not_running" as const, pid: null },
    ]) {
      stopAnswer = a;
      stop!.click();
      for (let i = 0; i < 100 && said() !== stopSaid(a); i++) await new Promise((x) => setTimeout(x, 10));
      expect(said()).toBe(stopSaid(a));
      for (let i = 0; i < 100 && stop!.disabled; i++) await new Promise((x) => setTimeout(x, 10));
    }
  });
});

// 「远端后端的诊断要有读者」：机器页那一行点「日志」⇒ 经那台后端的只读面（`backend-log`）取回来摆出来。
describe("〔GAP1〕每台一行的「日志」：问的是那一台、摆的是它回的那份", () => {
  it("★ 点远端那一行的「日志」⇒ 问那一台的 backend-log，头一行是路径与大小，正文原样；再点收起", async () => {
    logAnswers = {
      甲机: { path: "/h/.cc-monitor/logs/backend/stderr.log", size: 2048, text: "WARN 打标失败\n", truncated: true },
    };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const row = [...s.element.querySelectorAll<HTMLElement>(".backend-row")].find((r) => r.dataset.origin === "甲机")!;
    const btn = [...row.querySelectorAll("button")].find((b) => b.textContent === "日志")!;
    btn.click();
    await flush();
    await flush();
    expect(calls.filter((c) => c.name === "backend_log").map((c) => c.args)).toEqual([{ origin: "甲机" }]);
    const box = row.querySelector<HTMLElement>("[data-backend-log]")!;
    expect(box.dataset.backendLog).toBe("甲机");
    expect(box.querySelector(".settings-hint")?.textContent).toBe(
      "/h/.cc-monitor/logs/backend/stderr.log（2.0 KB） 只显示了最后一段",
    );
    expect(box.querySelector("pre")?.textContent).toBe("WARN 打标失败\n");
    btn.click();
    await flush();
    expect(row.querySelector("[data-backend-log]"), "再点一下该收起").toBeNull();
  });

  it("没落文件（path: null）⇒ 只一句说清；问不到 ⇒ 说取不回、带原因", async () => {
    logAnswers = { "<local>": { path: null, size: 0, text: "", truncated: false } };
    const s = new BackendSection({ headless: true });
    await flush();
    await flush();
    const [local, remote] = [...s.element.querySelectorAll<HTMLElement>(".backend-row")];
    for (const r of [local, remote]) [...r.querySelectorAll("button")].find((b) => b.textContent === "日志")!.click();
    await flush();
    await flush();
    expect(local.querySelector("[data-backend-log] .settings-hint")?.textContent).toBe("这台的后端没有把输出写进文件");
    expect(remote.querySelector("[data-backend-log] .settings-hint")?.textContent).toMatch(/^取不回这台后端的日志：/);
    expect(readBackendLog({ path: "/p", size: 1, text: "x" }), "缺 truncated ⇒ 形状不对").toBeNull();
  });
});
