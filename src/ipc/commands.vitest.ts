/**
 * C04a：**钉死 121 个命令名** —— Rust 侧 `#[tauri::command]` 集 ↔ `invoke_handler` 注册表 ↔
 * 包装层（键名 **与** 它传给 `invoke` 的字面量）↔ 全仓 TS 字面量调用点。
 *
 * ## 这条守卫替换的是什么
 *
 * Phase G 时全仓唯一的跨语言契约门禁是 `settings/cc-bus-hooks-section.vitest.ts` 里的一张
 * **单文件白名单**，覆盖 **3/121 个命令、1/29 个文件**。（C01 之后已不再「唯一」：
 * C01 钉了 1 个命令名 + 类型、C02 钉了 11 个事件名——Phase D 审计 J1 订正了原来那句话。）
 * 本文件把「命令名」这一维**扩到 121/121**。
 *
 * ## 成文规则（主计划 §5）：名字钉死是普遍的，类型生成是按需的
 *
 * 名字错了是运行时必错（`invoke` 直接 reject），与有没有人用返回值无关 ⇒ **全覆盖**。
 * 返回类型只在 TS 侧真消费字段时才生成 ⇒ **按需**（C03 用 `SftpStat` 立的先例）。
 *
 * ## 两条**不能写**的断言（都会假红，而假红的守卫会被人关掉）
 *
 * 1. **「每个命令都必须经过包装层」** —— C04a 只迁了 1 个模块做样板，其余 118 个仍走裸
 *    `invoke`，由 C04d 分批迁。写了就是当场假红。
 * 2. **「每个 Rust 命令都必须在 TS 侧被静态调用过」** —— **实测证否**：本轮我先扫出
 *    「7 个命令 TS 从没调过」（`sftp_delete`/`sftp_mkdir`/`sftp_rename`/4 个 `stream_*`），
 *    逐个查后发现**全都在用**，只是经**动态命令名**走的：
 *    `panel.ts:378` `this.doWrite("sftp_delete", …)`（helper 转发）·
 *    `panel.ts:485` `invoke(cmd, args)` · `session-viewer.ts:211` `invoke<number>(ipc, …)` ·
 *    `history.ts:489` `invoke(ipc, …)`。
 *    ⇒ 只做**单向**子集断言（TS 静态可见的 ⊆ Rust 集），不做反向。
 *    但**把这 7 个名字逐字钉死**（`DYNAMIC_ONLY`）：盲区本身不许静默变大。
 *
 * ## 一条容易误判的计数（Phase D 审计 J8 / 计划 §1）
 *
 * `#[tauri::command]` 属性全仓出现 **122** 次，唯一 fn 名 **121** 个（Z05 加了
 * `remote_acct_iso_shellinit`）——`bring_monitor_to_front`
 * 有 `#[cfg(windows)]` / `#[cfg(not(windows))]` 一对（`lib.rs:1376` 与 `lib.rs:1475`）。
 * 用 `Set` 去重是对的；拿 `grep -c` 复核的人会以为差了一个。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join } from "node:path";

// ── `K-H2b` `D4 阻-2`：文件末尾那一组是**行为**判据，要驱动真的 `views/history.ts`。
//    mock 骨架照 `views/history-actions.vitest.ts`（路径多一层 `../`）。
//    ⚠ 这几条 mock 是**文件级**的，但本文件其余判据全是「读源码文本 + 数命令名」，
//      一条都不经过被 mock 的那几个模块 ⇒ 它们的读数一格不动（入场/交回各打过一次全量）。
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("../views/session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_c: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runNewSessionRemote: vi.fn().mockResolvedValue(undefined),
  // ── 下面三条只为 `../tabs` 的导入面（`D5 阻-2` 那一组）——本文件不驱动远端那半。
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));

// ── `K-H2b` `D5 阻-2`：文件末尾再加一组行为判据，驱动**真的 `TabManager`**。
//    `tabs.ts` 的模块图很重（stream / cards / 渲染族），这几条 mock 是**为了让它能在
//    jsdom 里实例化**，形状照 `src/tabs.vitest.ts`（那边路径少一层 `../`）。
//    ⚠ 与上面那组同一条纪律：本文件其余判据一条都不经过被 mock 的这几个模块
//      （它们全是「读源码文本 + 数命令名」），入场/交回各打一次全量核过读数。
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../stream", () => ({
  MessageStream: class {
    contentElement = document.createElement("div");
    constructor(_root: HTMLElement) {}
    insertNode(): void {}
    batchInsert(fn: () => void): void {
      fn();
    }
    scrollToBottom(): void {}
    dispose(): void {}
  },
}));
vi.mock("../record-timeline", () => ({
  RecordTimeline: class {
    constructor(_s: unknown) {}
    insert(): void {}
    removeByElement(): void {}
    dispose(): void {}
    get size(): number {
      return 0;
    }
    get maxSeq(): number {
      return Number.NEGATIVE_INFINITY;
    }
  },
}));
vi.mock("../branch-fold", () => ({
  BranchFolder: class {
    constructor(_el: unknown) {}
    setBatchMode(): void {}
    flushPending(): void {}
    recordAdded(): void {}
    unwrapAll(): void {}
    rebuildNow(): void {}
    dispose(): void {}
  },
}));
vi.mock("../render-stream-record", () => ({
  routeMetaAndBranch: vi.fn(() => "content"),
  renderContentRecord: vi.fn(),
}));
vi.mock("../cards", () => ({
  reconcilePendingToolResults: vi.fn(() => []),
  isCompactRecord: () => false,
}));
vi.mock("../cards/subagent", () => ({ isAgentTool: () => false }));
vi.mock("../tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
  DEFAULT_EXIT_WAIT_MS: 10_000,
}));
vi.mock("../behavior", () => ({
  getBehavior: () => ({ resumeCommandLocal: "", resumeCommandRemote: "" }),
}));
vi.mock("../format", () => ({ formatTimestampSmart: () => "时间" }));

import { REPO_ROOT } from "../test-support/repo-root";
import { stripComments } from "../test-support/strip-comments";
import { commands } from "./commands";
import { invoke } from "@tauri-apps/api/core";
import { HistoryView } from "../views/history";
import { TabManager } from "../tabs";
import {
  primeLocalLaunchAccounts,
  __resetLocalLaunchSnapshotForTests,
  __resetAccountsCacheForTest,
  type Account,
} from "../accounts";

/** Rust 有、但 TS 侧**静态**看不见的命令（全部经动态命令名调用）。见头注「不能写的断言 2」。 */
/**
 * Rust 有、但 TS 侧**静态**看不见的命令。
 *
 * ★★ **C04d 批 6c 起这是空集** —— 121 个命令**全部**静态可见。
 *
 * C04a 立本文件时这里有 7 个，并据此把头注写成「已知盲区、只做单向断言」。
 * 批 6a/6b/6c 逐个查实后结论是：**那 7 个从来不是任意字符串**——
 * 两处是 `origin ? "A" : "B"` 的**两字面量三元**（`views/session-viewer.ts` / `views/history.ts`）、
 * 一处是 `doWrite(cmd, args)` 转发 helper 而**调用方传的全是字面量**（`sftp/panel.ts`）。
 * 「动态」只在于名字从一个**封闭、静态可知的集合**里选。
 *
 * 所以原计划的 `invokeDynamic(name, args)` 逃生口**没有做**（批 6a 推翻）：
 * 为一件其实是静态的事加一个 `string` 键的后门，等于亲手造一个守卫扫不到的洞。
 *
 * **保留这个空数组而不是删掉断言**：它现在钉的性质是「**不许再出现新的动态命名调用**」
 * ——哪天有人写了 `invoke(someVar, …)`，`rustOnly` 会非空、这条会红。
 */
const DYNAMIC_ONLY: string[] = [];

const WRAPPER_FILE = "src/ipc/commands.ts";

function walk(dir: string, ext: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, ext, out);
    else if (name.endsWith(ext)) out.push(p);
  }
  return out;
}

/**
 * Rust 侧命令集：**递归全仓 + 剥注释**后，取每个 `#[tauri::command…]` 之后的第一个 `fn` 名。
 *
 * - 属性写成带参形式 `#[tauri::command(rename_all = "snake_case")]`（Tauri 2 的正式功能）
 *   也要认——Phase D 审计变异 M4 实测：只认裸形式时守卫**假红**，且诊断说反了
 *   （报「注册了却找不到声明」，其实声明就在那儿）。
 * - 窗口从 400 收到 **120**：实测属性到 fn 名的最大真实距离是 **65**
 *   （`history.rs` 的 `stream_history_sessions_in_project`），400 是 6 倍余量，
 *   给「孤儿属性抓到下一个 fn 名」留了空间。120 仍有近 2 倍余量。
 */
function rustCommands(): Set<string> {
  const out = new Set<string>();
  for (const f of walk(resolve(REPO_ROOT, "src-tauri/src"), ".rs")) {
    const code = stripComments(readFileSync(f, "utf8"), "rust");
    for (const m of code.matchAll(/#\[tauri::command\b[^\]]*\]/g)) {
      const tail = code.slice(m.index, m.index + 120);
      const fn = /\bfn\s+([a-z_0-9]+)/.exec(tail);
      if (fn) out.add(fn[1]);
    }
  }
  return out;
}

/**
 * `invoke_handler(tauri::generate_handler![…])` 里注册的名字（去 module 前缀）。
 *
 * **不许用行锚**：Phase D 审计变异 M3/M3b 实测，两个注册项写在同一行、或最后一项漏尾逗号，
 * `cargo fmt --check` 都是 **rc=0**（rustfmt **不进** `generate_handler!` 的内容，
 * 我专门跑了 `cargo fmt` 看 diff——一字未动），而带行锚的守卫会**假红**并报
 * 「声明了却没注册」。注释已剥 ⇒ body 里只剩注册项，按逗号切就够。
 */
function registeredCommands(): Set<string> {
  const code = stripComments(readFileSync(resolve(REPO_ROOT, "src-tauri/src/lib.rs"), "utf8"), "rust");
  const handlers = [...code.matchAll(/generate_handler!\[/g)];
  expect(handlers, "`generate_handler![` 不是恰好一处——守卫只会守住其中一半").toHaveLength(1);
  const start = handlers[0].index;
  const end = code.indexOf("])", start);
  expect(end, "找不到 generate_handler! 的收尾 `])`").toBeGreaterThan(start);
  const body = code.slice(start + "generate_handler![".length, end);
  return new Set(
    [...body.matchAll(/([a-z_0-9:]+)\s*,?/g)]
      .map((m) => m[1].split("::").pop() as string)
      .filter((s) => s.length > 0),
  );
}

/**
 * TS 侧**字面量**命令名（剥注释后）。动态名看不见——见头注「不能写的断言 2」。
 *
 * `(?<![A-Za-z0-9_$])` 防的是 `myinvoke("x")` / `this.invoke("x")` 被误当 Tauri 的 `invoke`
 * （本仓的 `mockInvoke` 等靠大写 I 逃过，但那是运气）。泛型参数放宽成 `[\s\S]{0,200}?`，
 * 因为 `invoke<Array<{ f: (x: number) => void }>>("x")` 这种含 `(` 的泛型会让旧正则整条漏掉
 * ——方向是**假绿**（本仓今天 0 处，但别把免疫建立在「今天没人这么写」上）。
 */
function tsLiteralCommands(): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const f of walk(resolve(REPO_ROOT, "src"), ".ts")) {
    if (f.includes(".test.") || f.includes(".vitest.")) continue;
    const code = stripComments(readFileSync(f, "utf8"), "ts");
    for (const m of code.matchAll(
      /(?<![A-Za-z0-9_$])invoke\s*(?:<[\s\S]{0,200}?>)?\s*\(\s*["'`]([A-Za-z_][A-Za-z0-9_]*)["'`]/g,
    )) {
      const arr = out.get(m[1]) ?? [];
      arr.push(f.slice(REPO_ROOT.length + 1));
      out.set(m[1], arr);
    }
  }
  return out;
}

/**
 * 把包装层拆成「键名 → 该条目的源码文本」。
 *
 * 按**行首的键**切分而不是按逗号，因为 C04d 的带参条目会被 prettier 折成多行
 * （`invoke<Record<string, number>>` 里也有逗号）。
 */
function wrapperEntries(): Map<string, string> {
  const src = stripComments(readFileSync(resolve(REPO_ROOT, WRAPPER_FILE), "utf8"), "ts");
  const objStart = src.indexOf("export const commands = {");
  expect(objStart, "包装层里找不到 `export const commands = {`——守卫失效了").toBeGreaterThan(-1);
  const objEnd = src.indexOf("} as const;", objStart);
  expect(objEnd, "包装层里找不到 `} as const;`").toBeGreaterThan(objStart);
  const body = src.slice(objStart, objEnd);

  const keys = [...body.matchAll(/^\s{2}([a-z_0-9]+)\s*:/gm)];
  const out = new Map<string, string>();
  keys.forEach((m, idx) => {
    const from = m.index as number;
    const to = idx + 1 < keys.length ? (keys[idx + 1].index as number) : body.length;
    out.set(m[1], body.slice(from, to));
  });
  return out;
}

describe("C04a 命令名钉死", () => {
  it("Rust 侧「声明 = 注册」，且计数恰好 142", () => {
    const declared = rustCommands();
    const registered = registeredCommands();

    // 反向自检：真扫到了东西（不是空集在空转）
    expect(declared.size, "一个命令都没扫到——抽取器坏了").toBeGreaterThan(50);
    expect(registered.size, "注册表没扫到——正则或锚点坏了").toBeGreaterThan(50);

    const onlyDeclared = [...declared].filter((c) => !registered.has(c)).sort();
    const onlyRegistered = [...registered].filter((c) => !declared.has(c)).sort();
    expect(onlyDeclared, "这些命令声明了却没注册 ⇒ 前端调不到").toEqual([]);
    expect(onlyRegistered, "这些注册了却找不到声明 ⇒ 注册表里有死名字").toEqual([]);

    // 计数自检用等号：加/删命令必须红一次，逼人来更新这个数与包装层
    expect(declared.size, `期望恰好 145 个命令，实得 ${declared.size}`).toBe(145); //；**K-H2a +2（read_relay_credentials_status / write_relay_credentials_key）** // U8c-2c-2 +1（render_ccm_launch）；U8a-2c-pre +1（render_launch_payload）；P3t-Y2b +1（local_tmux_names）；**P4c +2（cc_bus_broadcast / cc_bus_kill，#77/#78）**；**P8a +1（list_plugin_marketplaces，#70）**；**PS1 +1（deploy_local_cc_bus）**；**PS2 +1（cc_bus_install_state）**；**K-H2b +1（relay_routing_for：界面问「这几个**本机**账号走不走中转」——只答本机是机制决定的：中转是每台机器自己的进程、注入的是回环地址，本机这一侧答不了远端那台）**
    // ⚠ 上面那句提示原本写「期望恰好 131」而断言是 136 —— 报错文案与断言值**对不上**，
    //   本件顺手订正：它会把一次真实的计数变动报成一个不存在的数。
  });

  it("包装层：键名 ⊆ Rust 集，**且每个条目的键名 == 它传给 invoke 的字面量**", () => {
    const rust = rustCommands();
    const entries = wrapperEntries();
    const keys = Object.keys(commands);

    // 反向自检：包装层非空，且文本解析出来的条目与运行时的键**逐个一致**
    expect(keys.length, "包装层是空的").toBeGreaterThan(0);
    expect([...entries.keys()].sort(), "文本解析出的条目与运行时的键不一致——解析器坏了").toEqual(
      [...keys].sort(),
    );

    const bogus = keys.filter((c) => !rust.has(c));
    expect(bogus, "包装层里这些键名 Rust 侧不存在 ⇒ 运行时必错").toEqual([]);

    // **本条是 Phase D 审计的阻塞项**：键不动、只把字面量抄成另一个真实存在的命令时，
    // `tsc` 0 错、其余守卫全绿，而运行时会调错命令（实测反例：字面量抄成 `open_log_file`，
    // 它返回 `Result<(), String>` ⇒ 设置面板拿到 null 后 render 直接崩）。
    for (const [key, text] of entries) {
      const literals = [
        ...text.matchAll(
          /(?<![A-Za-z0-9_$])invoke\s*(?:<[\s\S]{0,200}?>)?\s*\(\s*["'`]([A-Za-z_][A-Za-z0-9_]*)["'`]/g,
        ),
      ].map((m) => m[1]);
      expect(literals, `包装层条目 ${key} 应当恰好调一个字面量命令名`).toEqual([key]);
    }

    // 计数自检：C04d 每迁一个模块进来，这个数要跟着涨（红一次提醒更新）
    expect(keys.length, `包装层今天覆盖 ${keys.length} 个`).toBe(135) //；**K-H2b +1（relay_routing_for：本件把它落进包装层而不是散在 `accounts.ts` —— 落哪儿会不会红是两个不同的数：散在别处只动上面那两个，进包装层**多动这一个**）** //；**K-H2a +2（read_relay_credentials_status / write_relay_credentials_key）** // P4c +2（cc_bus_broadcast / cc_bus_kill）; // devbench F03 +3（list_skills/read_skill_file/write_skill_file）；U8a-2c-1 +1（daemon_send_into）； Z05 +1；G6 远端分叉 +1、list_remote_tmux 进包装层 +1；U8c-2c-2 +1（render_ccm_launch）；**P2s +5（set_daemon_kill_on_exit / daemon_status / daemon_start / daemon_stop / daemon_machines：每台机一个 daemon 开关，C8）** P3t-Y2b +1（local_tmux_names）；**P8a +1（list_plugin_marketplaces）**；**PS1 +1（deploy_local_cc_bus）**；**PS2 +1（cc_bus_install_state）**
  });

  // 标题里的数原先写着 112，而断言早就是 119 了（Z05 起 120；local-as-remote L3a 起 121）——**标题也是记录**，
  // 一并订正，免得下一个人拿标题当依据。
  it("TS 侧字面量命令名 ⊆ Rust 集，唯一名数 == 137，动态名盲区逐字钉死", () => {
    const rust = rustCommands();
    const used = tsLiteralCommands();

    const bogus = [...used.entries()]
      .filter(([c]) => !rust.has(c))
      .map(([c, files]) => `${c} @ ${files.join(", ")}`);
    expect(bogus, "这些命令名 Rust 侧不存在 ⇒ invoke 会被 reject").toEqual([]);

    // **等号，不是下界**：Phase D 审计变异 M6 实测，`toBeGreaterThan(50)` 只要 29 个含调用点的
    // 文件里最大的 4 个被扫到就能喂饱——把 walk 缩到 3 个子目录（可见名 112 → 81）时守卫仍全绿。
    // **C04d 批 6a：112 → 114。** 那两个 `stream_read_*` 此前藏在一个
    // `origin ? "A" : "B"` 三元里（C04a 把它记成「7 个命令 TS 静态看不见」的盲区之一），
    // 改成两次静态调用后**它们成了字面量** ⇒ 这个数会随盲区收缩而涨，最终应到 **全部**。
    // ★★ **C04d 批 6c 到 119 —— 这是里程碑：命令全部静态可见。**（Z05 +1 ⇒ 120；L3a +1 ⇒ 121；G6 远端分叉 +1 ⇒ 122；E79 本机会话账号 +1 ⇒ 123）
    // C04a 立本文件时记了「7 个命令 TS 静态看不见」这个已知盲区，并据此**刻意只做单向断言**。
    // 批 6a/6b/6c 逐个查实后发现那 7 个**从来不是任意字符串**：
    // 两处是 `origin ? "A" : "B"` 的两字面量三元（`session-viewer.ts` / `views/history.ts`）、
    // 一处是 `doWrite(cmd, args)` 转发 helper 而调用方传的全是字面量（`sftp/panel.ts`）。
    // 改成静态调用 / thunk 后**盲区归零** ⇒ 下面 `DYNAMIC_ONLY` 现在是空集。
    expect(used.size, `期望恰好 145 个字面量命令名，实得 ${used.size}`).toBe(145); //；**K-H2a +2（read_relay_credentials_status / write_relay_credentials_key）** // devbench F03 +3（skill 接入面三条） // U8c-2c-2 +1；U8a-2c-pre +1；P3t-Y2b +1（local_tmux_names）；**P4c +2**；**P8a +1（list_plugin_marketplaces）**；**PS1 +1（deploy_local_cc_bus）**；**PS2 +1（cc_bus_install_state）**；**K-H2b +1（relay_routing_for：界面问「这几个**本机**账号走不走中转」——只答本机是机制决定的：中转是每台机器自己的进程、注入的是回环地址，本机这一侧答不了远端那台）**

    // **不断言反向**（Rust ⊆ TS），但把盲区本身钉死：动态名集变了必须红一次。
    const rustOnly = [...rust].filter((c) => !used.has(c)).sort();
    expect(rustOnly, "TS 静态看不见的命令集变了——要么新增了动态名调用，要么扫描器瞎了").toEqual(
      [...DYNAMIC_ONLY].sort(),
    );
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// `K-H2b` `D1 阻-1`：**本机起会话的每一条主路都要把账号说出来**
// ═════════════════════════════════════════════════════════════════════════════
//
// ★★ 它治的是什么（`D1` 现打，PM 复核属实）：
// `tabs.ts` 那处 `invoke("resume_history_session", …)` 与 `views/history.ts` 那两处
// **一个账号都没传**，而 `history.rs` 自己的注释就写着「`fork-flow.ts` 是全仓唯一
// 给 `resume_history_session` 传 `configDir` 的」。⇒ 主路上账号恒缺席，后果两条：
// ① 起会话落到 shell rc 里那个默认号上（静默串号）；
// ② 中转那一格**永远拼不出路由键** —— 一件叫「接上注入点」的东西，主路没接。
//
// ⚠ 本条钉的是**人群**，不是「有没有一处传了」：多一条新主路而忘了传账号，当场红。
//
// 🔴🔴 **`D4 阻-2` 的订正：本组量的是「那几行字在不在」，不是「那件事发生没发生」。**
// `D4` 两刀实测（读数逐字在本文件末那一组的头注里）：把值换成 `undefined`、把写入口的
// 函数体掏空而**调用文本一字不动** —— 本组**两刀都照绿**（`1502 passed`）。
// ⇒ **本组的射程只到「人群 + 那一行字还在」**，行为那一半由本文件**末尾那一组**买
// （`K-H2b D4 阻-2：主路的账号与 pin 是**行为**判据`）。两组合起来才是那条性质，
// **单独任何一组都不够** —— 别再拿本组的绿当「账号真的传到了后端」。
//
// ⚠ 它住**本文件**而不是 `accounts.vitest.ts`：那边加一个目录遍历会撞
// `scanning-guard-registry` 的递减棘轮（「测试里做目录遍历的文件只许变少」，
// 本轮实测 11 > 上限 10）。本文件本来就在遍历，人群不多一个。
describe("K-H2b D1 阻-1：本机起会话的主路都传了账号", () => {
  /** 生产段里起本机会话的那几处调用（剥注释、跳过测试文件与包装层）。 */
  function localLaunchCallSites(): Array<{ file: string; text: string }> {
    const out: Array<{ file: string; text: string }> = [];
    for (const f of walk(resolve(REPO_ROOT, "src"), ".ts")) {
      if (f.includes(".test.") || f.includes(".vitest.")) continue;
      if (f.endsWith("/ipc/commands.ts")) continue; // 包装层是签名，不是调用点
      // ⚠ **先剥注释**：散文里逐字写着 `invoke("resume_history_session", …)` 这种句子
      //    （`launch-requests.ts` 的头注、`accounts.ts` 的说明各一处），
      //    不剥会被数成调用点 —— 本轮实测扫出 6 处而真实是 4。
      const code = stripComments(readFileSync(f, "utf8"), "ts");
      for (const m of code.matchAll(
        // ⚠ 窗口 1200 字符是**量出来的**：`fork-flow.ts` 那处调用里夹着一整段注释，
        //    400 的窗口够不到它的收尾 `})`，实测只扫到 3 处（应为 4）—— 那是**假绿**方向。
        /(?:invoke\s*\(\s*["'](resume_history_session|new_local_session)["']|commands\.(resume_history_session|new_local_session)\s*\()\s*([\s\S]{0,1200}?)\}\s*\)/g,
      )) {
        out.push({ file: f.slice(REPO_ROOT.length + 1), text: m[0] });
      }
    }
    return out;
  }

  it("★ 每一处起本机会话的调用都带 `account`（人群 = 现打出来的那几处）", () => {
    const sites = localLaunchCallSites();
    // 抽取器自检：一处都没扫到 = 正则坏了，下面整条在空转。
    expect(sites.length, "一处本机起会话的调用都没扫到 —— 抽取器坏了").toBeGreaterThan(3);
    // ⚠ 分母写下来：这是**现打**的处数，不是「所有起会话的路」。
    //   多一条新主路 ⇒ 这个数变 ⇒ 红一次，逼人回来看要不要传账号。
    expect(
      sites.length,
      `起本机会话的调用点从 4 变成了 ${sites.length}：\n${sites.map((s) => s.file).join("\n")}`,
    ).toBe(4);
    const missing = sites.filter((s) => !/\baccount\s*:/.test(s.text)).map((s) => s.file);
    expect(
      missing,
      "这些主路没把账号说出来 ⇒ ① 起会话落到 shell rc 那个默认号上（静默串号）；\n" +
        "② 中转那一格拼不出路由键（没有账号 id ⇒ 不注入）。\n" +
        "取值口只有一个：`accounts.ts::localLaunchAccountSync`。",
    ).toEqual([]);
  });

  it("★★ 本机 resume 那两条也往 pin 里写（`D3 阻-2`：写入口先前结构上只走远端）", () => {
    // 现打（`D3`，PM 复核属实）：`recordLastAccount` 的生产调用点**恰好 2**，
    // 而两处**结构上只走远端** —— `withAccount(` 的 6 个生产调用点 6/6 在 `origin` 分支内；
    // `restartWithAccount(` 的唯一调用点首行逐字 `if (tab.origin === null) return false;`。
    // ⇒ 本机的 `list_last_accounts` **恒空** ⇒ 取值口那条「pin 优先」在本机永远走不到，
    //   而那正是「参数位有、值恒空」那一形的另一半。
    for (const f of ["src/tabs.ts", "src/views/history.ts"]) {
      const code = stripComments(readFileSync(resolve(REPO_ROOT, f), "utf8"), "ts");
      expect(
        (code.match(/recordLocalLaunchAccount\(/g) ?? []).length,
        `${f} 里没有本机这条路的记账 —— 本机 pin 恒空，「pin 优先」那一支永远走不到`,
      ).toBeGreaterThan(0);
      // 不许 `await` 它（多一拍会撞那两条只放行一个微任务的 DOM 判据）。
      expect(code).not.toContain("await recordLocalLaunchAccount");
    }
  });

  it("★ 取值口只有一个（不许哪条路自己现算一个账号）", () => {
    // 三条主路走那个唯一取值口；fork 那条是**用户在小窗里显式选的**，
    // 它有自己的语义（选了账号 0 就要显式 `base`），所以不走这个口 —— 如实记，不强求。
    for (const f of ["src/tabs.ts", "src/views/history.ts"]) {
      const code = readFileSync(resolve(REPO_ROOT, f), "utf8");
      expect(
        (code.match(/localLaunchAccountSync\(/g) ?? []).length,
        `${f} 里没调那个唯一取值口`,
      ).toBeGreaterThan(0);
      // 取值是同步的，**不许**有人给它加 `await`（那会多一拍，撞两条只放行一个微任务的判据）。
      expect(code, `${f} 给那个取值口加了 await —— 主路的时序会多一拍`).not.toContain(
        "await localLaunchAccountSync",
      );
    }
    // 阴性对照：`fork-flow.ts` 那条**刻意**不走它（它是用户显式选的那一格）。
    const fork = readFileSync(resolve(REPO_ROOT, "src/fork-flow.ts"), "utf8");
    expect(fork).not.toContain("localLaunchAccountSync");
    expect(fork).toContain('{ kind: "base" }');
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// `K-H2b` `D4 阻-2`：**账号真的进了载荷、pin 真的被写进去**（行为，不是文本）
// ═════════════════════════════════════════════════════════════════════════════
//
// ★★ 它治的是什么（`D4` 两刀实测，逐字）：
// 上面那一组判据钉的是「那几行字在不在」——`(code.match(/recordLocalLaunchAccount\(/g)).length > 0`
// 与 `/\baccount\s*:/`。`D4` 用两刀证明那**买不到这件事发生没发生**：
//   · 刀 `D2k6`：`views/history.ts` 起新会话那一行的 `account` 入参换成硬写的 `undefined`
//     ⇒ **`1502 passed` 全绿**（那一行字还在，值没了）；
//     ⚠ 这里**刻意不逐字复述那个锚点** —— 复述一次，下一个照「全仓 N 处一起切」的人
//     就会把本段一起切掉（`阻-1` 那条病的形状，别在治它的这一拍里再长一次）。
//   · 刀 `A2`：`accounts.ts::recordLocalLaunchAccount` 的函数体掏空成永不生效、
//     **调用文本一个字不动** ⇒ **`1502 passed` 全绿**（本机 pin 从此恒不写）。
//
// ⇒ 本组一律走**真的 `HistoryView`**：造一行、开右键菜单、点那两条，然后
//    **取出那次 `invoke` 的第 2 个实参、直接读 `.account`**。
// ⚠ **不许用 `toHaveBeenCalledWith` 的整对象比** —— `toEqual` 语义下
//    `account: undefined` 与「没有这个键」**相等**，那两条断言对本格恒真（`D4 §D` 现打）。
//
// ⚠ **本组买不到什么**：它止于「monitor 发出去的那一发 `invoke` 载荷里有这个值」。
//    「后端真的拿它拼出了前缀」由 `history.rs` / `payload.rs` 那几条买，
//    「那一发真的走到中转」由 `KH2B1` 的端到端买。三段各买各的，别读成一段。
describe("K-H2b D4 阻-2：主路的账号与 pin 是**行为**判据（驱动真的 HistoryView）", () => {
  const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

  const DIR_A = "/h/.claude-accts/acct-a";
  const DIR_B = "/h/.claude-accts/acct-b";

  function acct(name: string, configDir: string): Account {
    return {
      name,
      email: `${name}@x.edu`,
      configDir,
      isDefault: false,
      mode: "isolated",
      exists: true,
      loggedIn: true,
    } as Account;
  }

  /**
   * 让 `invoke` 按一份**账号世界**回话。
   *
   * `accounts` 为空 ⇒ 取值口说不出账号（阴性对照那一档）。
   * ⚠ 阴性对照**不靠时序**：主路那一脚 `primeLocalLaunchAccounts()` 是不等待的，
   *   万一它抢在读值之前跑完，喂给它的也是这份空世界 ⇒ 两种排序下答案相同。
   */
  function serveAccounts(accounts: Account[], defaultName: string | null, pins: Record<string, string>): void {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_local_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          notice: null,
          meta: {
            enabled: true,
            acctsDir: "/h/.claude-accts",
            manifestPath: "/h/.claude-accts/accounts.json",
            updatedAt: null,
            sharedStore: null,
            count: accounts.length,
            error: null,
          },
          accounts,
        });
      }
      if (cmd === "load_config") return Promise.resolve({ accounts: { defaultName } });
      if (cmd === "list_last_accounts") return Promise.resolve(pins);
      return Promise.resolve(undefined);
    });
  }

  /** 把快照**用生产段那条路**填热（`primeLocalLaunchAccounts` 是不等待的 ⇒ 这里冲一轮宏任务）。 */
  async function warm(): Promise<void> {
    primeLocalLaunchAccounts();
    await new Promise((r) => setTimeout(r, 0));
  }

  function proj(): Record<string, unknown> {
    return { projectPath: "/p", projectName: "P", projectDir: "pd", sessionCount: 2, starredCount: 0, hiddenCount: 0, lastActivity: 1, hasLive: false };
  }
  function entry(): Record<string, unknown> {
    return { sessionId: "s1", projectPath: "/p", projectName: "P", aiTitle: "T", firstUserExcerpt: "x", startedAt: 1, updatedAt: 1, jsonlPath: "/p/s1.jsonl", isLive: false, messageCountApprox: 1, starred: false, hidden: false };
  }

  /** 造一行、开右键菜单、点 `label` 那一条，然后把异步链排空。 */
  async function clickRowAction(label: string): Promise<void> {
    const view = new HistoryView();
    const row = (view as unknown as { buildEntryRow(e: unknown, p: unknown): HTMLElement }).buildEntryRow(entry(), proj());
    document.body.appendChild(row);
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    const items = [...document.querySelectorAll<HTMLButtonElement>(".history-context-item")];
    const btn = items.find((b) => b.textContent === label);
    // 抽取器自检：菜单没出来 / 文案改了 ⇒ 下面整条在空转，必须红。
    expect(btn, `右键菜单里没有「${label}」—— 实得 ${JSON.stringify(items.map((b) => b.textContent))}`).toBeTruthy();
    btn!.click();
    await new Promise((r) => setTimeout(r, 0));
  }

  /** 那一发 `invoke` 的第 2 个实参（**取出来直接读字段**，不做整对象比 —— 见本组头注）。 */
  function payloadOf(cmd: string): Record<string, unknown> {
    const call = invokeMock.mock.calls.find((c) => c[0] === cmd);
    expect(call, `一次 \`${cmd}\` 都没发出去 —— 主路根本没走到，下面的断言在空转`).toBeTruthy();
    return call![1] as Record<string, unknown>;
  }

  /** 记 pin 的那一发（`recordLastAccount` → `update_history_metadata` 带 `lastAccount`）。 */
  function pinWrites(): Array<Record<string, unknown>> {
    return invokeMock.mock.calls
      .filter((c) => c[0] === "update_history_metadata")
      .map((c) => c[1] as Record<string, unknown>)
      .filter((a) => (a.patch as Record<string, unknown> | undefined)?.lastAccount !== undefined);
  }

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    __resetAccountsCacheForTest();
    __resetLocalLaunchSnapshotForTests();
    document.body.replaceChildren();
    document.querySelectorAll(".history-context-menu").forEach((n) => n.remove());
  });

  it("★★ resume 主路：载荷里的 `account` 是**那条会话的 pin**（不是常量、不是当前号）", async () => {
    // pin 指 acct-a，而**当前账号是 acct-b** ⇒ 两个值不同 ⇒ 「随便回一个」也过不了。
    serveAccounts([acct("acct-a", DIR_A), acct("acct-b", DIR_B)], "acct-b", { s1: "acct-a" });
    await warm();
    await clickRowAction("在新终端 resume");
    expect(
      payloadOf("resume_history_session").account,
      "resume 的载荷里没有那条会话上次用的账号 ——\n" +
        "① 起会话落到 shell rc 那个默认号上（静默串号）；② 中转那一格拼不出路由键。\n" +
        "⚠ 这一条是**行为**：`account:` 那行字还在、值是 `undefined` 时它必须红。",
    ).toEqual({ kind: "named", configDir: DIR_A });
  });

  it("★★ 新开主路：载荷里的 `account` 是**当前账号**（与上一条取到不同的值 ⇒ 不是常量）", async () => {
    serveAccounts([acct("acct-a", DIR_A), acct("acct-b", DIR_B)], "acct-b", { s1: "acct-a" });
    await warm();
    await clickRowAction("在该目录起新会话");
    expect(
      payloadOf("new_local_session").account,
      "起新会话的载荷里没有当前账号 —— `D4` 刀 `D2k6` 正是把这一行的值换成 `undefined`，\n" +
        "而当时全仓 `1502 passed` 全绿。",
    ).toEqual({ kind: "named", configDir: DIR_B });
  });

  it("★★ resume 之后 pin **真的被写进去**（`update_history_metadata` 带那个 sid 与那个名字）", async () => {
    serveAccounts([acct("acct-a", DIR_A), acct("acct-b", DIR_B)], "acct-b", { s1: "acct-a" });
    await warm();
    await clickRowAction("在新终端 resume");
    expect(
      pinWrites(),
      "本机 resume 之后一条 pin 都没写 —— `D4` 刀 `A2` 正是把 `recordLocalLaunchAccount`\n" +
        "的函数体掏空、**调用文本一字不动**，而当时全仓 `1502 passed` 全绿。\n" +
        "本机 pin 恒空 ⇒ 取值口那条「pin 优先」在本机永远走不到。",
    ).toEqual([{ sessionId: "s1", patch: { lastAccount: "acct-a" } }]);
  });

  it("★ 阴性对照：说不出账号 ⇒ 载荷里是 `undefined`，且**一条 pin 都不写**", async () => {
    // 空账号世界：快照冷（`beforeEach` 已 reset），而主路那一脚 prime 拿到的也是空的。
    serveAccounts([], null, {});
    await clickRowAction("在新终端 resume");
    expect(
      payloadOf("resume_history_session").account,
      "说不出账号时它不该猜一个 —— 「不表态」= 逐字节旧行为",
    ).toBeUndefined();
    expect(pinWrites(), "说不出账号却往 pin 里写了一条 —— 那是把「不知道」写成了一条 pin").toEqual([]);
    // 反空真：这一趟主路**真的走到了**（否则上面两条是「什么都没发生」的空真）。
    expect(invokeMock.mock.calls.some((c) => c[0] === "resume_history_session")).toBe(true);
  });
});

describe("K-H2b D5 阻-2：tab 栏那条本机 resume 也是**行为**判据（驱动真的 TabManager）", () => {
  // # 为什么这一组非有不可（分母写在最前）
  //
  // `localLaunchAccountSync(` 的**生产调用点恰好 3**（现打：`git ls-files -z | xargs -0 grep -Fn`
  // 排掉 `*.vitest.ts` ⇒ `src/tabs.ts:2246` · `src/views/history.ts:1666` · `:1713`，
  // 外加定义 1 处）。上一组（`D4 阻-2`）买的 4 条行为判据**全部驱动 `views/history.ts`**，
  // 而 `tabs.ts` 那一处当时只有**文本**判据。`D5` 现打三刀：
  //   · `X3a` 整个不传账号 ⇒ `2 failed`（**粗刀挡得住**）
  //   · `X3b` `localLaunchAccountSync(sid)` → `(null)`（用当前号顶替这条会话的 pin，**静默串号**）⇒ **1509 全绿**
  //   · `X3c` `recordLocalLaunchAccount(sid,…)` → `("",…)`（调用文本与两个标识符全留，pin 恒不写）⇒ **1509 全绿**
  // ⇒ **粗刀挡得住、细刀漏得掉。** 本组把那两把细刀各买一条。
  const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

  const DIR_A = "/h/.claude-accts/acct-a";
  const DIR_B = "/h/.claude-accts/acct-b";

  function acct(name: string, configDir: string): Account {
    return {
      name,
      email: `${name}@x.edu`,
      configDir,
      isDefault: false,
      mode: "isolated",
      exists: true,
      loggedIn: true,
    } as Account;
  }

  /** 与上一组同一份「账号世界」：pin 指 `acct-a`，而当前号是 `acct-b` ⇒ 两个值不同。 */
  function serveAccounts(accounts: Account[], defaultName: string | null, pins: Record<string, string>): void {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_local_accounts") {
        return Promise.resolve({
          available: true,
          error: null,
          notice: null,
          meta: {
            enabled: true,
            acctsDir: "/h/.claude-accts",
            manifestPath: "/h/.claude-accts/accounts.json",
            updatedAt: null,
            sharedStore: null,
            count: accounts.length,
            error: null,
          },
          accounts,
        });
      }
      if (cmd === "load_config") return Promise.resolve({ accounts: { defaultName } });
      if (cmd === "list_last_accounts") return Promise.resolve(pins);
      return Promise.resolve(undefined);
    });
  }

  async function warm(): Promise<void> {
    primeLocalLaunchAccounts();
    await new Promise((r) => setTimeout(r, 0));
  }

  /** 建一个**本机**（`origin === null`）归档 tab，然后走 tab 栏那条 resume 主路。 */
  async function resumeLocalTab(sid: string): Promise<TabManager> {
    document.body.replaceChildren();
    const bar = document.createElement("div");
    const root = document.createElement("div");
    document.body.append(bar, root);
    const tm = new TabManager(bar, root);
    tm.ensureTab(sid, "/home/u/p", `/p/${sid}.jsonl`, 0, null);
    tm.archiveTab(sid);
    await (tm as unknown as { resumeTab(s: string): Promise<void> }).resumeTab(sid);
    await new Promise((r) => setTimeout(r, 0));
    return tm;
  }

  function payloadOf(cmd: string): Record<string, unknown> {
    const call = invokeMock.mock.calls.find((c) => c[0] === cmd);
    expect(call, `一次 \`${cmd}\` 都没发出去 —— 主路根本没走到，下面的断言在空转`).toBeTruthy();
    return call![1] as Record<string, unknown>;
  }

  function pinWrites(): Array<Record<string, unknown>> {
    return invokeMock.mock.calls
      .filter((c) => c[0] === "update_history_metadata")
      .map((c) => c[1] as Record<string, unknown>)
      .filter((a) => (a.patch as Record<string, unknown> | undefined)?.lastAccount !== undefined);
  }

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    __resetAccountsCacheForTest();
    __resetLocalLaunchSnapshotForTests();
    document.body.replaceChildren();
  });

  it("★★ tab 栏 resume：载荷里的 `account` 是**那条会话的 pin**（不是当前号）", async () => {
    serveAccounts([acct("acct-a", DIR_A), acct("acct-b", DIR_B)], "acct-b", { t1: "acct-a" });
    await warm();
    await resumeLocalTab("t1");
    expect(
      payloadOf("resume_history_session").account,
      "tab 栏那条本机 resume 没把**这条会话上次的账号**传下去 ——\n" +
        "`D5` 刀 `X3b` 正是把这一处换成 `localLaunchAccountSync(null)`（用当前号顶替 pin），\n" +
        "当时全仓 `1509 passed` 全绿。后果是**静默串号**：切过号之后 resume 落到当前号上，\n" +
        "中转再按那个错的 id 换上**别人那一行的 key**。",
    ).toEqual({ kind: "named", configDir: DIR_A });
  });

  it("★★ tab 栏 resume 之后 pin **真的被写进去**（带那个 sid 与那个名字）", async () => {
    serveAccounts([acct("acct-a", DIR_A), acct("acct-b", DIR_B)], "acct-b", { t1: "acct-a" });
    await warm();
    await resumeLocalTab("t1");
    expect(
      pinWrites(),
      "tab 栏那条本机 resume 之后一条 pin 都没写 ——\n" +
        "`D5` 刀 `X3c` 正是把 `recordLocalLaunchAccount(sid, …)` 的第一个实参换成 `\"\"`\n" +
        "（**调用文本与两个标识符全留**，函数首行 `if (!sid || !name) return;` ⇒ 恒不写），\n" +
        "当时全仓 `1509 passed` 全绿。这条路的本机 pin 恒空 ⇒「pin 优先」在这条路上永远走不到。",
    ).toEqual([{ sessionId: "t1", patch: { lastAccount: "acct-a" } }]);
  });

  it("★ 阴性对照：说不出账号 ⇒ 载荷里是 `undefined`，且**一条 pin 都不写**", async () => {
    serveAccounts([], null, {});
    await resumeLocalTab("t1");
    expect(
      payloadOf("resume_history_session").account,
      "说不出账号时它不该猜一个 —— 「不表态」= 逐字节旧行为",
    ).toBeUndefined();
    expect(pinWrites(), "说不出账号却往 pin 里写了一条 —— 那是把「不知道」写成了一条 pin").toEqual([]);
    // 反空真：这一趟主路**真的走到了**（否则上面两条是「什么都没发生」的空真）。
    expect(invokeMock.mock.calls.some((c) => c[0] === "resume_history_session")).toBe(true);
  });
});
