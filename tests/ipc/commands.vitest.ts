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
import { resolve, join, sep } from "node:path";
import ts from "typescript";

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
vi.mock("../../src/views/session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    constructor(_c: () => void) {}
    load(): void {}
    dispose(): void {}
  },
}));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../src/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runNewSessionRemote: vi.fn().mockResolvedValue(undefined),
  // ── 下面三条只为 `../tabs` 的导入面（`D5 阻-2` 那一组）——本文件不驱动远端那半。
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));

// ── `K-H2b` `D5 阻-2`：文件末尾再加一组行为判据，驱动**真的 `TabManager`**。
//    `tabs.ts` 的模块图很重（stream / cards / 渲染族），这几条 mock 是**为了让它能在
//    jsdom 里实例化**，形状照 `tests/tabs.vitest.ts`（那边路径少一层 `../`）。
//    ⚠ 与上面那组同一条纪律：本文件其余判据一条都不经过被 mock 的这几个模块
//      （它们全是「读源码文本 + 数命令名」），入场/交回各打一次全量核过读数。
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../src/stream", () => ({
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
vi.mock("../../src/record-timeline", () => ({
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
vi.mock("../../src/branch-fold", () => ({
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
vi.mock("../../src/render-stream-record", () => ({
  routeMetaAndBranch: vi.fn(() => "content"),
  renderContentRecord: vi.fn(),
}));
vi.mock("../../src/cards", () => ({
  reconcilePendingToolResults: vi.fn(() => []),
  isCompactRecord: () => false,
}));
vi.mock("../../src/cards/subagent", () => ({ isAgentTool: () => false }));
vi.mock("../../src/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../src/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../../src/account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
  DEFAULT_EXIT_WAIT_MS: 10_000,
}));
vi.mock("../../src/behavior", () => ({
  getBehavior: () => ({ resumeCommandLocal: "", resumeCommandRemote: "" }),
}));
vi.mock("../../src/format", () => ({ formatTimestampSmart: () => "时间" }));

import { REPO_ROOT } from "../test-support/repo-root";
import { stripComments } from "../test-support/strip-comments";
import { commands } from "../../src/ipc/commands";
import { invoke } from "@tauri-apps/api/core";
import { HistoryView } from "../../src/views/history";
import { TabManager } from "../../src/tabs";
import type { TabSessionActions } from "../../src/tab-session-actions";
import {
  primeLocalLaunchAccounts,
  __resetLocalLaunchSnapshotForTests,
  __resetAccountsCacheForTest,
  type Account,
} from "../../src/accounts";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";

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

/**
 * 两个「期望的计数」——**报文（`it` 标题 + `expect` 的诊断）与断言共用同一个值**〔`K-R4`〕。
 *
 * ## 为什么要立成常量，而不是各写各的字面量
 *
 * 本文件此前是这条病的活体：`it` 标题写「计数恰好 **142**」而断言是 `.toBe(144)`；
 * 另一条标题写「唯一名数 == **137**」而断言同样是 `.toBe(144)`。
 * 判据红了，人照标题去查 142 / 137 —— **查的是一个不存在的事实**。
 * 根因是「一个值装了两件事」〔`K13`〕：标题里那个数是**写的时候手抄的**，
 * 断言里那个数是**跟着代码走的**，两者之间没有任何东西钉住它们相等。
 * ⇒ 把两份拷贝合成一个值：改了断言，标题与诊断**结构上不可能不跟着变**。
 *
 * ## 为什么是两个常量而不是一个
 *
 * 今天两个数都是 144，而且本文件的两条子集断言（`bogus` 空 + `rustOnly == DYNAMIC_ONLY`）
 * 合起来钉的正是「两个集合相等」⇒ 它们**structurally** 同值。
 * 但它们量的是**两件事**（Rust 侧声明的命令集 · TS 侧静态可见的字面量命令名集），
 * 合成一个常量就是反过来再犯一次「一个值装了两件事」。
 *
 * ⚠ 改这两个数之前先读上面那句：加/删命令时**两个都要动**，
 *   只动一个会被那两条子集断言当场逮住。
 */
/** Rust 侧 `#[tauri::command]` 声明（= `invoke_handler` 注册）的唯一命令名个数。 */
// 🔴 〔`设计/50` 删用量 09-18〕**151 → 147**：`aggregate_usage_all` /  〔散文墓碑〕
// `aggregate_remote_usage_all` / `account_usage` / `account_usage_local` 四条同拍退役  〔散文墓碑〕
//（用量 ②③ 两轴整轴不做了）。**这是本常量第一次往下走** —— 命令数变小要跟着改三个数：
// 本常量 · `TS_LITERAL_COMMAND_COUNT` · 下面包装层覆盖数（141 → 137）。
const RUST_COMMAND_COUNT = 143; // **〔RM1c · 第四波〕+1（panorama_call：代码全景经那台机器的后端走；落进了包装层，所以本文件那**三个**数一起 +1）** // **〔RM1a · 第四波〕+1（relay_ensure：让远端那台上有一个中转在跑）** // **〔合并 C4a〕−1（「某会话属哪个账号」两条退役 −2、`chan_call` ＋1；与 SE2 ＋1 / S4 −1 按两边增量相加 ⇒ 141，跑出来核过）** // **〔SE2〕+1（find_in_session：会话内查找，后端 `--find-in-session`；落进了包装层，所以本文件那**三个**数一起 +1）** // **〔第四波 S4〕−1（sftp_copy：零流量复制随门禁 `f3-copy` 那一格退役；它在包装层里，所以本文件那**三个**数一起 −1）** // **〔F7c 收尾 09-24〕−12（池子那十二条 sftp_* 命令随老面板与窗口改走通道删了；都在包装层里，所以本文件那**三个**数一起 −12）** // **〔B2 · 条 66〕+1（推生效值那一条命令退役 −1，`backend_exit_policy` / `set_backend_exit_policy` 进 +2：「退出行为」那个值搬到后端那台机器上，前端改走问 / 交写两条；都落进了包装层，所以本文件那**三个**数一起 +1）** // **〔合并 AL1〕+2（aliases_render / aliases_read / aliases_install 进，write_account_aliases 退役；都在包装层里 ⇒ 三个数一起 +2）** // **〔PN1b 选图〕+2（panorama_diagram_kinds / panorama_diagram，都 Local；归已有能力 `panorama.code-graph`）** // **〔A3 第二波〕+2（check_local_acct_iso / local_acct_iso_shellinit；与 SE1/U3b 合并时按两边增量相加）** // **〔SE1〕+1（list_user_inputs：大纲的数据源，后端 `--list-user-inputs`；落进了包装层）** // **〔U3b〕+1（replay_keep_tail_only：接上骨架的会话重放缓冲只留尾巴；落进了包装层，所以本文件那**三个**数一起 +1）** // **〔`设计/10` 骨架 · 子步 3〕+2（read_session_index / read_session_range：`--read-session-from-offset` 在 monitor 侧的两个调用点；都落进了包装层，所以本文件那**三个**数一起 +2）** // **〔`设计/60 §4 戊` · `24e` 第二刀 · 09-20〕+1（open_file_window：原生文件管理窗口那条入口；它落进了包装层，所以本文件那**三个**数一起 +1）** // **〔步 12·C 收尾 · 09-20〕−2（`origin` 归一的最后两对：退役 `write_remote_mcp_server` / `remove_remote_mcp_server`。两条退役的**都在包装层里**，所以本文件那**三个**数一起 −2）** // **〔`设计/60 §5.4c` · 09-20〕+1（sftp_chmod：`SETSTAT` 改权限那一条；它落进了包装层，所以本文件那**三个**数一起 +1）** // **〔步 12·C · 09-20〕−5（`origin` 归一：5 对同义双份合成一条带 origin 的 ⇒ 退役 `create_remote_branch_session` / `delete_remote_history_session` / `stream_remote_history_sessions` / `stream_read_remote_session` / `list_remote_mcp_project_dirs`。五条退役的**都在包装层里**，所以本文件那**三个**数一起 −5）** // **〔步 23b · 09-20〕+1（sftp_copy：零流量复制那一条；它落进了包装层，所以本文件那**三个**数一起 +1）**
// **K-R109 +1**（render_local_attach：本机后端产 `ccm attach <名>` 那一句，`R61` 裁定三；
// 它与 `generate_handler!` 那一行、`parity_ledger::LEDGER` 那一行**必须同一拍**）。
// **K-R49 +1**（write_account_aliases〔散文墓碑〕：加了账号就把 `zcc` / `bcc` 那条命令落盘；〔AL1〕已退役）。
// 增量账（谁把这个数推上去的）：**K-H2a +2**（read_apikey_credentials_status /
// write_apikey_credentials_key）；U8c-2c-2 +1（render_ccm_launch）；
// U8a-2c-pre +1（render_launch_payload）；P3t-Y2b +1（local_tmux_names）；
// **P4c +2**（cc_bus_broadcast / cc_bus_kill，#77/#78）；**P8a +1**（list_plugin_marketplaces，#70）；
// **PS1 +1**（deploy_local_cc_bus）；**PS2 +1**（cc_bus_install_state）；
// **K-H2b +1**（apikey_routing_for：界面问「这几个**本机**账号走不走 apikey 端点改写」）。

/** TS 侧**字面量** `invoke("…")` 里出现过的唯一命令名个数。 */
const TS_LITERAL_COMMAND_COUNT = 143; // **〔RM1c · 第四波〕+1（panorama_call 落进包装层）** // **〔RM1a · 第四波〕+1（relay_ensure 落进包装层）** // **〔合并 C4a〕−1（同上）** // **〔SE2〕+1（find_in_session：会话内查找，后端 `--find-in-session`；落进了包装层，所以本文件那**三个**数一起 +1）** // **〔第四波 S4〕−1（sftp_copy：零流量复制随门禁 `f3-copy` 那一格退役；它在包装层里，所以本文件那**三个**数一起 −1）** // **〔F7c 收尾 09-24〕−12（池子那十二条 sftp_* 命令随老面板与窗口改走通道删了；都在包装层里，所以本文件那**三个**数一起 −12）** // **〔B2 · 条 66〕+1（推生效值那一条命令退役 −1，`backend_exit_policy` / `set_backend_exit_policy` 进 +2：「退出行为」那个值搬到后端那台机器上，前端改走问 / 交写两条；都落进了包装层，所以本文件那**三个**数一起 +1）** // **〔合并 AL1〕+2（同上）** // **〔PN1b 选图〕+2（panorama_diagram_kinds / panorama_diagram，都 Local；归已有能力 `panorama.code-graph`）** // **〔A3 第二波〕+2（check_local_acct_iso / local_acct_iso_shellinit；与 SE1/U3b 合并时按两边增量相加）** // **〔SE1〕+1（list_user_inputs：大纲的数据源，后端 `--list-user-inputs`；落进了包装层）** // **〔U3b〕+1（replay_keep_tail_only：接上骨架的会话重放缓冲只留尾巴；落进了包装层，所以本文件那**三个**数一起 +1）** // **〔`设计/10` 骨架 · 子步 3〕+2（read_session_index / read_session_range：`--read-session-from-offset` 在 monitor 侧的两个调用点；都落进了包装层，所以本文件那**三个**数一起 +2）** // **〔`设计/60 §4 戊` · `24e` 第二刀 · 09-20〕+1（open_file_window：原生文件管理窗口那条入口；它落进了包装层，所以本文件那**三个**数一起 +1）** // **〔步 12·C 收尾 · 09-20〕−2（`origin` 归一的最后两对：退役 `write_remote_mcp_server` / `remove_remote_mcp_server`。两条退役的**都在包装层里**，所以本文件那**三个**数一起 −2）** // **〔`设计/60 §5.4c` · 09-20〕+1（sftp_chmod：`SETSTAT` 改权限那一条；它落进了包装层，所以本文件那**三个**数一起 +1）** // **〔步 12·C · 09-20〕−5（`origin` 归一：5 对同义双份合成一条带 origin 的 ⇒ 退役 `create_remote_branch_session` / `delete_remote_history_session` / `stream_remote_history_sessions` / `stream_read_remote_session` / `list_remote_mcp_project_dirs`。五条退役的**都在包装层里**，所以本文件那**三个**数一起 −5）** // `设计/50` −4，见 `RUST_COMMAND_COUNT` 上面那段 // **〔步 23b · 09-20〕+1（sftp_copy：零流量复制那一条；它落进了包装层，所以本文件那**三个**数一起 +1）**
// **K-R109 +1**（render_local_attach —— 它落进了包装层，所以这个数也 +1）。
// **K-R49 +1**（write_account_aliases〔散文墓碑〕，同上 —— 它落进了包装层，所以 `keys.length` 那个数也 +1）。
// 增量账：**K-H2a +2**（同上）；devbench F03 +3（skill 接入面三条）；U8c-2c-2 +1；
// U8a-2c-pre +1；P3t-Y2b +1（local_tmux_names）；**P4c +2**；
// **P8a +1**（list_plugin_marketplaces）；**PS1 +1**（deploy_local_cc_bus）；**PS2 +1**（cc_bus_install_state）；
// **K-H2b +1**（apikey_routing_for，同上 —— 它落进了包装层，所以 `keys.length` 那个数也 +1）。

/**
 * `K-R122`（09-14）：**吐出来的路径一律用 `/` 分隔，跟这台机器的 `path.sep` 无关。**
 *
 * 🔴 它治的是一条**判据自己的病**，不是产品的病。云端 `Frontend typecheck + build` 那个 job
 * 跑在 **windows runner** 上，`join()` 给回的是 `src\ipc\local-tmux-name.ts`；
 * 而本文件下游三处都拿**正斜杠字面量**去认路
 *（`endsWith("/ipc/commands.ts")` · `endsWith("/ipc/local-tmux-name.ts")` · `endsWith("/remote-launch.ts")`），
 * 反斜杠那一份一条都剔不掉 ⇒ 「铸名只有一个算法口」那条实得
 * `["src\\ipc\\local-tmux-name.ts", "src\\remote-launch.ts", "src\\tabs.ts"]`、期望 `["src/tabs.ts"]`
 * ⇒ vitest `1 failed | 1725 passed`。**产品一个字节没问题，红的是量它的那把尺子。**
 *
 * ⚠ **规范化落在这一个点上，不写第二份平台分支**：`split(sep).join("/")` 在 Linux 上
 * `sep === "/"` ⇒ 恒等（是 no-op，不是「另一条路」），在 Windows 上把 `\` 换成 `/`。
 * 长度不变 ⇒ 下游那三处 `f.slice(REPO_ROOT.length + 1)` 一个字都不用改。
 * ⚠ 只规范**吐出去的叶子**；递归仍拿本机形态的 `p` 下探（`readdirSync` 两种都吃）。
 */
function walk(dir: string, ext: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, ext, out);
    else if (name.endsWith(ext)) out.push(p.split(sep).join("/"));
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
  for (const f of walk(resolve(REPO_ROOT, "src/bridge/src"), ".rs")) {
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
  const code = stripComments(readFileSync(resolve(REPO_ROOT, "src/bridge/src/lib.rs"), "utf8"), "rust");
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
  // ★ 标题里的数从 `RUST_COMMAND_COUNT` 渲染，不再手抄〔`K-R4`〕——
  //   `it` 标题是 vitest 失败输出的第一行，**它就是报文**。
  it(`Rust 侧「声明 = 注册」，且计数恰好 ${RUST_COMMAND_COUNT}`, () => {
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
    expect(declared.size, `期望恰好 ${RUST_COMMAND_COUNT} 个命令，实得 ${declared.size}`).toBe(
      RUST_COMMAND_COUNT,
    );
    // ⚠ 这句提示曾两次与断言值对不上（写「131」而断言 136；标题写「142」而断言 144）——
    //   **两次都是手抄**。`K-R4` 把它换成从 `RUST_COMMAND_COUNT` 渲染：
    //   改断言那个值，标题与这句提示**必须**跟着变，没有第二份拷贝可以馊。
    // ⚠ 合并 `K-H2b` 时这里撞了一次：分支那侧还是「写死 145 + 一长串手抄增量账」的老形状。
    //   **取的是主干这一侧的形状，只把常量从 144 抬到 145**（增量账挪去常量定义旁边）——
    //   反过来（保住分支那侧）等于把 `K-R4` 刚拆掉的那份拷贝又装回来。
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
    expect(keys.length, `包装层今天覆盖 ${keys.length} 个`).toBe(143) // **〔RM1c · 第四波〕+1（panorama_call 落进包装层，所以本文件那**三个**数一起 +1）** // **〔RM1a · 第四波〕+1（relay_ensure 落进包装层，所以本文件那**三个**数一起 +1）** // **〔合并 C4a〕子步 2 ＋10（裸 invoke 那十条补进包装层）、子步 3 −1（同 `RUST_COMMAND_COUNT`）⇒ 与 SE2 ＋1 / S4 −1 相加 = 141 == Rust 命令数** // **〔SE2〕+1（find_in_session：会话内查找，后端 `--find-in-session`；落进了包装层，所以本文件那**三个**数一起 +1）** // **〔第四波 S4〕−1（sftp_copy：零流量复制随门禁 `f3-copy` 那一格退役；它在包装层里，所以本文件那**三个**数一起 −1）** // **〔F7c 收尾 09-24〕−12（池子那十二条 sftp_* 命令随老面板与窗口改走通道删了；都在包装层里，所以本文件那**三个**数一起 −12）** // **〔B2 · 条 66〕+1（推生效值那一条命令退役 −1，`backend_exit_policy` / `set_backend_exit_policy` 进 +2：「退出行为」那个值搬到后端那台机器上，前端改走问 / 交写两条；都落进了包装层，所以本文件那**三个**数一起 +1）** // **〔合并 AL1〕+2（同上）** // **〔PN1b 选图〕+2（panorama_diagram_kinds / panorama_diagram，都 Local；归已有能力 `panorama.code-graph`）** // **〔A3 第二波〕+2（check_local_acct_iso / local_acct_iso_shellinit；与 SE1/U3b 合并时按两边增量相加）** // **〔SE1〕+1（list_user_inputs：大纲的数据源，后端 `--list-user-inputs`；落进了包装层）** // **〔U3b〕+1（replay_keep_tail_only：接上骨架的会话重放缓冲只留尾巴；落进了包装层，所以本文件那**三个**数一起 +1）** // **〔`设计/10` 骨架 · 子步 3〕+2（read_session_index / read_session_range：`--read-session-from-offset` 在 monitor 侧的两个调用点；都落进了包装层，所以本文件那**三个**数一起 +2）** // **〔`设计/60 §4 戊` · `24e` 第二刀 · 09-20〕+1（open_file_window：原生文件管理窗口那条入口；它落进了包装层，所以本文件那**三个**数一起 +1）** // **〔步 12·C 收尾 · 09-20〕−2（`origin` 归一的最后两对：退役 `write_remote_mcp_server` / `remove_remote_mcp_server`。两条退役的**都在包装层里**，所以本文件那**三个**数一起 −2）** // **〔`设计/60 §5.4c` · 09-20〕+1（sftp_chmod：`SETSTAT` 改权限那一条；它落进了包装层，所以本文件那**三个**数一起 +1）** // **〔步 12·C · 09-20〕−5（`origin` 归一：5 对同义双份合成一条带 origin 的 ⇒ 退役 `create_remote_branch_session` / `delete_remote_history_session` / `stream_remote_history_sessions` / `stream_read_remote_session` / `list_remote_mcp_project_dirs`。五条退役的**都在包装层里**，所以本文件那**三个**数一起 −5）** // ／／ **〔步 23b · 09-20〕+1（sftp_copy：零流量复制那一条；它落进了包装层，所以本文件那**三个**数一起 +1）** //；**`设计/50` −4（那四条用量命令的声明整块删了）** //；**K-R109 +1（render_local_attach）—— ⚠ 本文件里跟着新命令走的是**三个**数，不是两个：`RUST_COMMAND_COUNT` · `TS_LITERAL_COMMAND_COUNT` · 这一个。派工单只点了前两个** //；**K-R69 +1（local_ccm_entry_status）**//；**K-R49 +1（write_account_aliases）**//；**K-H2b +1（apikey_routing_for：本件把它落进包装层而不是散在 `accounts.ts` —— 落哪儿会不会红是两个不同的数：散在别处只动上面那两个，进包装层**多动这一个**）** //；**K-H2a +2（read_apikey_credentials_status / write_apikey_credentials_key）** // P4c +2（cc_bus_broadcast / cc_bus_kill）; // devbench F03 +3（list_skills/read_skill_file/write_skill_file）；U8a-2c-1 +1（backend_send_into）； Z05 +1；G6 远端分叉 +1、list_remote_tmux 进包装层 +1；U8c-2c-2 +1（render_ccm_launch）；**P2s +5（set_backend_kill_on_exit / backend_status / backend_start / backend_stop / backend_machines：每台机一个后端开关，C8）** P3t-Y2b +1（local_tmux_names）；**P8a +1（list_plugin_marketplaces）**；**PS1 +1（deploy_local_cc_bus）**；**PS2 +1（cc_bus_install_state）**；**`K-R135` +3（ccm_user_path_status / ccm_user_path_add / ccm_user_path_remove：用户级 PATH 那一格，`R85`/`R87`/`R88`）**
  });

  // 标题里的数原先写着 112，而断言早就是 119 了（Z05 起 120；local-as-remote L3a 起 121）——**标题也是记录**，
  // 一并订正，免得下一个人拿标题当依据。
  // ⚠ **那次订正只改了拷贝，没拆掉「两份拷贝」这个结构** ⇒ 它又馊了一次：
  //   09-01 现打，标题写 137 而断言是 144。`K-R4` 改成从 `TS_LITERAL_COMMAND_COUNT` 渲染。
  it(`TS 侧字面量命令名 ⊆ Rust 集，唯一名数 == ${TS_LITERAL_COMMAND_COUNT}，动态名盲区逐字钉死`, () => {
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
    expect(
      used.size,
      `期望恰好 ${TS_LITERAL_COMMAND_COUNT} 个字面量命令名，实得 ${used.size}`,
    ).toBe(TS_LITERAL_COMMAND_COUNT);

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
      `起本机会话的调用点从 5 变成了 ${sites.length}：\n${sites.map((s) => s.file).join("\n")}`,
    ).toBe(5); // 〔`A3` 第二波〕4 → 5：`account-restart-local.ts`（本机换号重启的 resume 那一跳；账号是用户点的那个，带着）
    const missing = sites.filter((s) => !/\baccount\s*:/.test(s.text)).map((s) => s.file);
    expect(
      missing,
      "这些主路没把账号说出来 ⇒ ① 起会话落到 shell rc 那个默认号上（静默串号）；\n" +
        "② 中转那一格拼不出路由键（没有账号 id ⇒ 不注入）。\n" +
        "取值口只有一个：`accounts.ts::localLaunchAccountSync`。",
    ).toEqual([]);
  });

  // ═══════════════════════════════════════════════════════════════════════
  // `K-R46`：**每一条 `resume_history_session` 都要把 tmux 名说出来**
  //
  // 病（09-10 现打，分母在下面）：后端**故意**拒绝自己铸名
  // （`history.rs` 的 `NO_TMUX_NAME`）⇒ 前端不传 = 会话不进具名容器。
  // 三个 `resume_history_session` 调用点里**只有 `tabs.ts` 那一处传了**，
  // `views/history.ts`（历史页 + 搜索卡片）与 `fork-flow.ts`（分叉本机起）都没传。
  //
  // ⚠ **`new_local_session` 不在这个分母里，而那不是漏掉**：Rust 侧
  //   `history.rs::new_local_session` 的签名里**根本没有 `tmux_name` 这一格**
  //   （函数体给 `launch_local` 的第五个实参硬写 `None`）⇒ 前端传了也没人收。
  //   补它要同一拍改 `src/bridge/`，**不在 `K-R46` 写区**，已随本件上报。
  //   ⇒ 本条的分母是**带得了这个参数的那几处**，不是「全部起会话的路」。
  //
  // 🔴 **本条只买「那一行字在不在 + 人群」**（与上一条同病，`D4` 两刀证过）：
  //   把值换成恒 `null`、或把铸名口掏空，本条**照绿**。
  //   值真的被铸出来、且真的避让了，由 `views/history-actions.vitest.ts` 与
  //   `fork-flow.vitest.ts` 那两组**行为**判据买。两段合起来才是那条性质。
  it("★ 每一处 `resume_history_session` 都带 `tmuxName`（分母 = 带得了这个参数的那几处）", () => {
    const sites = localLaunchCallSites();
    const resumeSites = sites.filter((s) => s.text.includes("resume_history_session"));
    // 抽取器自检：分成两族之后任一族空掉 = 上面那个正则坏了，下面在空转。
    expect(
      resumeSites.length,
      `\`resume_history_session\` 的调用点从 4 变成了 ${resumeSites.length}：\n` +
        resumeSites.map((s) => s.file).join("\n"),
    ).toBe(4); // 〔`A3` 第二波〕3 → 4：`account-restart-local.ts`（带 `tmuxName` —— 复用被 kill 让出来的旧名）
    expect(
      sites.length - resumeSites.length,
      "`new_local_session` 的调用点数变了 —— 它今天没有 `tmux_name` 参数位（Rust 侧签名里就没有），" +
        "变了要回来看是不是后端也开了那一格",
    ).toBe(1);
    const missing = resumeSites.filter((s) => !/\btmuxName\b/.test(s.text)).map((s) => s.file);
    expect(
      missing,
      "这些路没把 tmux 会话名传下去 ⇒ 后端 `render_local_ccm` 早退（`NO_TMUX_NAME`）⇒\n" +
        "如实降级回旧路 ⇒ 起出来的会话**不在具名 tmux 容器里**，于是 `list_local_tmux`\n" +
        "那一族（右键「杀死会话（kill tmux …）」/「就地 resume（复用空 tmux …）」）对它\n" +
        "一条都给不出来。名字只许过 `remote-launch.ts::mintTmuxName`（全仓唯一铸造口），\n" +
        "算法口住 `ipc/local-tmux-name.ts`。",
    ).toEqual([]);
  });

  it("★ 铸名只有一个算法口（不许哪条路自己现查一遍 `list_local_tmux` 再拼）", () => {
    // ⚠ **分母 3，今天 2/3 走口、1/3 内联** —— `src/tabs.ts` 那条 tab 栏 resume 自己
    //   写着同样的六行，而 `src/tabs.ts` 不在 `K-R46` 的写区 ⇒ 收不进来，如实钉住现状。
    //   这个 1 只许变小、不许变大：多一条内联的就红。
    const inline: string[] = [];
    for (const f of walk(resolve(REPO_ROOT, "src"), ".ts")) {
      if (f.includes(".test.") || f.includes(".vitest.")) continue;
      if (f.endsWith("/ipc/local-tmux-name.ts")) continue; // 算法口本体
      if (f.endsWith("/remote-launch.ts")) continue; // `mintSessionTmuxName` 的定义处
      const code = stripComments(readFileSync(f, "utf8"), "ts");
      // ⚠ 用**整个标识符**做匹配单位（`\b` + 收尾括号），不是裸子串 —— 那正是
      //   `scanning-guard-registry.vitest.ts` 那条递减棘轮盯的东西。
      //   第一版在这里对语料变量做了一次裸的存在性子串判断，门禁当场把上限 8 顶到 9。
      //   ⚠⚠ **连这条注释都不许把那个写法逐字抄下来** —— 那个棘轮扫的是**原始源码**、
      //     不剥注释，散文里写一遍就照样被数进去（本轮实测：改成正则之后仍红 1 处，
      //     红的就是这句注释里那份逐字副本）。本文件 `:440` 那条头注记的是同一族病。
      if (/\bmintSessionTmuxName\s*\(/.test(code)) inline.push(f.slice(REPO_ROOT.length + 1));
    }
    inline.sort();
    // 〔U2 · 第三波〕那条 tab 栏 resume 随会话动作整块搬进了 `src/tab-session-actions.ts`，
    //   六行内联逐字随行 ⇒ 名单里的住址换了，条数仍是 1（没收掉，也没多）。
    expect(
      inline,
      "本机铸名自己写了一遍的地方变了。算法口是 `src/ipc/local-tmux-name.ts`；\n" +
        "`src/tab-session-actions.ts`（原 `src/tabs.ts` 那条 tab 栏 resume）是 `K-R46` 收不进来的那一处，收掉它要另立一件。",
    ).toEqual(["src/tab-session-actions.ts"]);
  });

  it("★★ 本机 resume 那两条也往 pin 里写（`D3 阻-2`：写入口先前结构上只走远端）", () => {
    // 现打（`D3`，PM 复核属实）：`recordLastAccount` 的生产调用点**恰好 2**，
    // 而两处**结构上只走远端** —— `withAccount(` 的 6 个生产调用点 6/6 在 `origin` 分支内；
    // `restartWithAccount(` 的唯一调用点首行逐字 `if (tab.origin === null) return false;`。
    // ⇒ 本机的 `list_last_accounts` **恒空** ⇒ 取值口那条「pin 优先」在本机永远走不到，
    //   而那正是「参数位有、值恒空」那一形的另一半。
    // 〔U2〕tab 栏那条本机 resume 从 `src/tabs.ts` 搬到了 `src/tab-session-actions.ts`（逐字随行）。
    for (const f of ["src/tab-session-actions.ts", "src/views/history.ts"]) {
      const code = stripComments(readFileSync(resolve(REPO_ROOT, f), "utf8"), "ts");
      expect(
        (code.match(/recordLocalLaunchAccount\(/g) ?? []).length,
        `${f} 里没有本机这条路的记账 —— 本机 pin 恒空，「pin 优先」那一支永远走不到`,
      ).toBeGreaterThan(0);
      // 不许 `await` 它（多一拍会撞那两条只放行一个微任务的 DOM 判据）。
      expect(code).not.toContain("await recordLocalLaunchAccount");
    }
  });

  // ═══════════════════════════════════════════════════════════════════════
  // `K-P5h` `KP5HD3`：**回填的那一跳挂在会话出生那条事件上**
  //
  // 🔴🔴 **本条与上面那组同病：它量的是「那一行字在不在」，不是「那件事发生没发生」。**
  // `main.ts` **一个 export 都没有**（它是入口模块）⇒ 那一跳在本仓今天**没有任何办法
  // 用行为判据驱动**（`session-accounts-poll.ts` 的头注为同一个理由把三条性质搬出了 `main.ts`）。
  // ⇒ 本条**只买两件事**：① 那一跳还接在那条事件上；② 它没有偷偷变成一个定时器。
  //   把 `resolvePendingLocalLaunches` 的函数体掏空、或把它接到一条错的事件上，
  //   **本条照绿** —— 别把它读成「回填真的会被触发」。
  //   反查那一跳本身的行为判据在 `accounts.vitest.ts` 与 `views/history-actions.vitest.ts`。
  // ═══════════════════════════════════════════════════════════════════════
  it("★ `K-P5h`：待回填由「会话出生」那条事件触发，**不是**由一个新定时器触发", () => {
    const code = stripComments(readFileSync(resolve(REPO_ROOT, "src/main.ts"), "utf8"), "ts");
    // ① 那一跳还在，且**在 `onSessionStarted` 这个处理器里**（不是随便哪儿调一次）。
    const handler = code.split("onSessionStarted:")[1] ?? "";
    expect(
      handler.length,
      "`main.ts` 里没有 `onSessionStarted` 处理器了 —— 抽取器坏了，下面那条会零命中地绿",
    ).toBeGreaterThan(50);
    expect(
      handler.slice(0, 800),
      "回填那一跳没有接在 `session-started` 上 ⇒ 起完新会话之后再也没人来问，\n" +
        "整条「拿 token 反查 sid」在生产上不会发生（而它的单测照样全绿）。",
    ).toContain("resolvePendingLocalLaunches");
    // ② 🔴 **不许在这条路上开一个新的周期唤醒** —— 本项目有一条已交付的性质是
    //    「判活不靠定时轮询（内核一有事就通知）」，回填在这里起表就是开倒车。
    //    ⚠ 这一格钉的是**本仓这一处**，全仓的调度点由 `polling_registry` 那两条管。
    expect(handler.slice(0, 800)).not.toContain("setInterval");
    expect(handler.slice(0, 800)).not.toContain("setTimeout");
  });

  it("★ 取值口只有一个（不许哪条路自己现算一个账号）", () => {
    // 三条主路走那个唯一取值口；fork 那条是**用户在小窗里显式选的**，
    // 它有自己的语义（选了账号 0 就要显式 `base`），所以不走这个口 —— 如实记，不强求。
    // 〔U2〕tab 栏那条本机 resume 从 `src/tabs.ts` 搬到了 `src/tab-session-actions.ts`（逐字随行）。
    for (const f of ["src/tab-session-actions.ts", "src/views/history.ts"]) {
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
        "⚠ 这一条是**行为**：`account:` 那行字还在、值是 `undefined` 时它必须红。\n" +
        "🔴 `K-R53`：**名字也必须在里面** —— 后端那条 ccm 路只会 `--account <名字>`，\n" +
        "   只给目录 = 这条主路结构上到不了后端那条路，必然落第二实现。",
    ).toEqual({ kind: "named", configDir: DIR_A, name: "acct-a" });
  });

  it("★★ 新开主路：载荷里的 `account` 是**当前账号**（与上一条取到不同的值 ⇒ 不是常量）", async () => {
    serveAccounts([acct("acct-a", DIR_A), acct("acct-b", DIR_B)], "acct-b", { s1: "acct-a" });
    await warm();
    await clickRowAction("在该目录起新会话");
    expect(
      payloadOf("new_local_session").account,
      "起新会话的载荷里没有当前账号 —— `D4` 刀 `D2k6` 正是把这一行的值换成 `undefined`，\n" +
        "而当时全仓 `1502 passed` 全绿。\n" +
        "🔴 `K-R53`：名字也必须在里面（理由同上一条）。",
    ).toEqual({ kind: "named", configDir: DIR_B, name: "acct-b" });
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
    tm.ensureTab(sid, "/home/u/p", `/p/${sid}.jsonl`, 0, LOCAL_ORIGIN);
    tm.archiveTab(sid);
    // 〔S4 · 第四波〕会话动作住 `tab-session-actions.ts`；`TabManager` 上不再留同名转交 ⇒ 直接指向新家。
    await (tm as unknown as { actions: TabSessionActions }).actions.resumeTab(sid);
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
        "账号层再按那个错的 id 换上**别人那一行的 key**。\n" +
        "🔴 `K-R53`：名字也必须在里面 —— 后端那条 ccm 路只会 `--account <名字>`。",
    ).toEqual({ kind: "named", configDir: DIR_A, name: "acct-a" });
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

// ══════════════════════════════════════════════════════════════════════════
//  `设计/05 §8` 步 2：**`origin` 去 `null` 化 —— 入方向那一半的机检形态**
//
// `§8` 给步 2 的独立验证逐字是「**`tsc` 就能验**」。这一节把那句话从「今天跑了一趟
// `tsc` 没红」变成一条**站着的判据** —— 两者的差别是：`tsc` 只在有人真写下 `null`
// 那一刻才响，而下面这两条在**类型本身重新装得下 `null`** 的那一刻就响。
//
// 🔴 **两条，而且刻意异源** —— 少任何一条，另一条都能被静默绕过：
//
// | 条 | 人群从哪来 | 谁写的 | 它一个人守不住什么 |
// |---|---|---|---|
// | ① 参数面 | `src/ipc/commands.ts` 里每一处 `origin` 参数的**类型文本** | 加命令的那个人 | 有人往 Rust 里加回 `Origin::Unspecified` ⇒ 生成物变回 `null \| string` ⇒ 那 8 处写着 `Origin` 的参数**一个字不改**就又装得下 `null`，而本条照绿 |
// | ② 生成物面 | `src/generated/Origin.ts` 的**正文** | `ts-rs` 从 Rust 的 `origin::Origin` 生成 | 有人手写一处 `origin: string \| null` ⇒ 生成物一个字不动，而本条照绿 |
//
// ⚠ **它们买不到**：① 换个名字传同一件事（`host: string | null`）——
//   词是 `origin`，改名就出人群；② **出方向**那一半（`JsonlRecord` /
//   `RemoteHealthPayload` 一族仍是 `origin: string | null`，`null` = 本机）——
//   那几份住 `src/bridge/src/{bridge,history,search}.rs` 与 `lib.rs`，
//   不在步 2 的写区里，逐份读数在交回件里。**别把这一节读成「全仓没有 `null` 了」。**
// ══════════════════════════════════════════════════════════════════════════

/** `origin` 参数现打的处数。**恒等**，不是地板 —— 地板在「变少」方向是瞎的。 */
const ORIGIN_PARAM_COUNT = 49; // 〔ST3 · 第四波 4B〕+1（drift_ledger_report 带一个 `origin: Origin`：「未识别的数据」按机器分，问哪台答哪台）； // 〔RM1c · 第四波〕+1（panorama_call 带一个 `origin: Origin`：全景按 origin 问那台后端）；// 〔RW1 · 第四波 09-24〕+3（list_skills / read_skill_file / write_skill_file 各带一个 `origin: Origin`：远端项目的收件箱也能编辑，读写经那台机器的后端）； // 〔RM1a · 第四波〕+5（`read_apikey_credentials_status` / `write_apikey_credentials_key` / `apikey_routing_for` / `relay_ensure` / `config_surface_report` 各带一个 `origin: Origin`：账号层那份文件 · 中转 · 足迹都按机器）；// 〔合并 C4a〕+3（子步 2：list_remote_accounts · list_remote_session_accounts · check_account_trust ＋3；子步 3：远端会话账号那条退役 −1、`chan_call` ＋1）；与 RM1b ＋2 / SE2 ＋1 相加，跑出来核过；// 〔RM1b · 第四波〕+1（list_plugin_marketplaces 带一个 `origin: Origin`：插件市场按 origin 问那台后端）；// 〔RM1b · 第四波〕+1（get_session_tasks 带一个 `origin: Origin`：任务列表按 origin 问那台后端）；// 〔SE2〕+1（find_in_session 带一个 `origin: Origin`）；// 〔B2 · 条 66〕+1（推生效值那一条退役 −1，`backend_exit_policy` / `set_backend_exit_policy` 各带一个 `origin` +2）； 〔SE1〕+1（list_user_inputs 带一个 `origin: Origin`）； 〔`设计/10` 骨架 · 子步 3〕+2（read_session_index / read_session_range 各带一个 `origin: Origin`）

/**
 * 从一段包装层文本里摘出每一处 `origin` 参数：`{ optional, type }`。
 *
 * ⚠ 刻意只认 `origin` 这个**完整的词**（`\b`）：`originLabel` 不算。
 * ⚠ 类型文本取到第一个 `;` / `,` / `)` / `}` / 换行 为止 —— 跨行的类型它看不见，
 *   而那一档由下面那条阳性对照兜着（它证明识别器认得出三种「装得下 null」的写法）。
 */
function originParamsIn(chunk: string): Array<{ optional: boolean; type: string }> {
  const out: Array<{ optional: boolean; type: string }> = [];
  for (const m of chunk.matchAll(/\borigin(\??)\s*:\s*([^;,)\n}]+)/g)) {
    out.push({ optional: m[1] === "?", type: m[2].trim() });
  }
  return out;
}

/** 这一处 `origin` 参数**装得下**「没说」吗（`null` / `undefined` / 可选）。 */
function admitsNothingSaid(p: { optional: boolean; type: string }): boolean {
  return p.optional || /\b(null|undefined)\b/.test(p.type);
}

describe("`设计/05 §8` 步 2：origin 去 null 化（入方向）", () => {
  it(`★★ ① 参数面：${ORIGIN_PARAM_COUNT} 处 origin 参数，**零处**装得下「没说」`, () => {
    const entries = wrapperEntries();
    const found: Array<{ cmd: string; optional: boolean; type: string }> = [];
    for (const [cmd, chunk] of entries) {
      for (const p of originParamsIn(chunk)) found.push({ cmd, ...p });
    }
    // 反空真①：人群**恒等**。塌成 0（切分器坏了 / 包装层搬家）与「全都合规」
    // 在终端上一模一样；涨了而没人看见，则是一条新命令悄悄进来了。
    expect(
      found.length,
      `origin 参数现打 ${found.length} 处，钉的是 ${ORIGIN_PARAM_COUNT} 处。\n` +
        "真加/删了一条带 origin 的命令就来改这个数（连着改，别攒着）；\n" +
        "★ 掉到 0 多半是 `wrapperEntries()` 的切分口径变了 —— 那时下面那条会拿空集比出绿。",
    ).toBe(ORIGIN_PARAM_COUNT);
    // ★ 有牙的那条。
    const offenders = found.filter(admitsNothingSaid);
    expect(
      offenders.map((o) => `${o.cmd}: origin${o.optional ? "?" : ""}: ${o.type}`),
      "这几处 origin 参数**在类型上装得下「没说」**：\n" +
        "`设计/05 §8` 步 2 逐字「`origin` 去 `null` 化 —— 本机也带 origin」\n" +
        '⇒ 本机是一个**具名**的 origin（逐字 `LOCAL_ORIGIN` = `"<local>"`），\n' +
        "   `INVARIANTS §40` 逐字「本地 ＝ 不走 ssh 的远端」。\n" +
        "⇒ 出路是把那一处改成 `Origin`（生成物）并在调用点送 `?? LOCAL_ORIGIN`，\n" +
        "   **不是**把这条判据放宽。`§8` 还逐字警告过「2 在 5 之前（否则 `call` 的\n" +
        "   第一个参数还得容忍 `null`）」—— 放宽这一条就是把那件事又还回去。",
    ).toEqual([]);
  });

  it("★ ① 的阳性对照：识别器认得出三种「装得下没说」的写法，也认得出两种干净写法", () => {
    // 人群为零违例的日子里，这一条才是上面那格真正跑过的东西。
    for (const bad of [
      "  x: (args: { origin: string | null }) => 0,",
      "  y: (args: { origin?: string }) => 0,",
      "  z: (args: { origin: string | undefined }) => 0,",
    ]) {
      const ps = originParamsIn(bad);
      expect(ps.length, `识别器在 ${bad} 里一处 origin 参数都没摘到`).toBe(1);
      expect(ps.some(admitsNothingSaid), `识别器没认出 ${bad} 装得下「没说」`).toBe(true);
    }
    for (const good of [
      "  a: (args: { origin: Origin }) => 0,",
      "  b: (args: { origin: string; kill: boolean }) => 0,",
    ]) {
      const ps = originParamsIn(good);
      expect(ps.length, `识别器在 ${good} 里一处 origin 参数都没摘到`).toBe(1);
      expect(ps.some(admitsNothingSaid), `干净写法 ${good} 被判成违例 —— 假红比不查更坏`).toBe(
        false,
      );
    }
    // 匹配单位不许比事实小：`originLabel` 不是 `origin`。
    expect(originParamsIn("  c: (args: { originLabel: string | null }) => 0,")).toEqual([]);
  });

  it("★★ ② 生成物面：`Origin.ts` 的正文是 `string`，**装不下** `null`", () => {
    const gen = readFileSync(resolve(REPO_ROOT, "src/generated/Origin.ts"), "utf8");
    // 抽取器自检：真读到了那份生成物（不是空文件、不是指错了地方）。
    expect(gen, "读进来的不是 ts-rs 的生成物 —— 路径指错了地方").toContain(
      "generated by [ts-rs]",
    );
    const code = stripComments(gen, "ts");
    const decl = code.match(/export type Origin\s*=\s*([^;]+);/);
    expect(
      decl,
      "`Origin.ts` 里找不到 `export type Origin = …;` —— 生成物的形状变了",
    ).not.toBeNull();
    // ★ 有牙的那条：**恒等**，不是「不含 null」。
    //   写成 `.not.toContain("null")` 的话，`string | undefined` 或
    //   `string | Record<string, never>` 之类照样过 —— 那正是本仓治的「判据比事实松」。
    expect(
      (decl as RegExpMatchArray)[1].trim(),
      "`Origin` 的生成物正文不是 `string`。\n" +
        "🔴 它上一拍是 `null | string`（`Origin::Unspecified(())` 那个变体的线上形状），\n" +
        "   `设计/05 §8` 步 2 把那个变体**退役**了（处置与理由逐条写在\n" +
        "   `src/bridge/src/origin.rs` 头注里）。\n" +
        "⇒ 这一行变回带 `null` 的形状，只有一个原因：**Rust 那侧把变体加回去了**。\n" +
        "   那时全仓 8 处写着 `origin: Origin` 的参数会**一个字不改**地又装得下 `null`，\n" +
        "   而上面那条参数面判据照绿 —— 本条就是为这一形立的。",
    ).toBe("string");
  });
});

// ══════════════════════════════════════════════════════════════════════════
/**
 * 〔C4a · `设计/05 §8` 步 2 的 TS 那一半〕**TS 侧没有一处 origin 装得下 `null`。**
 *
 * # 它治什么
 *
 * 步 2 之前 TS 侧「本机」有两种写法在同一张类型里并存：`null`（tab / 设置共用 store / 分叉流…）
 * 与 `"<local>"`（`LOCAL_ORIGIN`，与 Rust 跨语言对拍）。两者之间靠散在各处的 `?? LOCAL_ORIGIN` /
 * `origin === null` / `origin ? … : …` 互相翻译 —— 基线上现打 **37 处** `origin…: … null`
 * （手写 36 ＋ 生成物 1）。`§8` 逐字「⚠ 2 在 5 之前（否则 `call` 的第一个参数还得容忍 `null`）」。
 *
 * `commands.vitest.ts` 那一节（「origin 去 null 化（入方向）」）只管**包装层的参数**；本条管**全部 TS**
 * （含 `src/generated/`）：字段 · 参数 · 变量 · 函数返回 · `new Set<…>` / `new Map<…>` 的元素类型。
 *
 * # 口径：量的是**声明**，不是文本
 *
 * 用 TypeScript 自己的解析器（`ts.createSourceFile`）走语法树，不用正则：`{ origin: x }`（对象字面量的值）
 * 与 `{ origin: string | null }`（类型）在文本上长得一样，只有语法树分得开。
 * 人群 = 名字里带 `origin`（大小写不敏感，`original…` 除外）、**带类型标注**的声明；
 * 违例 = 那段类型里出现 `null` 字面量类型。
 *
 * # 判据（零命中带正控；人群恒等）
 *
 * 1. 违例集合 == [`PENDING`]（两向）—— 今天只剩生成物一处，住址与解锁条件写在表里；
 * 2. 人群条数**恒等**（不是地板）—— 塌成 0 与「全都合规」在终端上一模一样；
 * 3. 识别器阳性对照五形 ＋ 阴性对照三形（`originalType` 不算、值不算、干净类型不算）；
 * 4. 真语料锚点：`Tab.origin`（`tab-model.ts`）与 `pickPrimaryOrigin` 的返回（`account-chip.ts`）
 *    必须在人群里 —— 证明扫的是真树。
 *
 * # 买不到（写死，别读宽）
 *
 * - `origin?: string`（可选 = 缺省）**不在违例里**：出方向生成物（`HistoryProject` / `JsonlLinePayload` /
 *   `SessionHits` 一族，Rust 侧 `skip_serializing_if`）用「缺省 = 本机」，那是 Rust 出方向的事，
 *   在 TS 这边只在消费处经 `ipc/origin.ts::originFromWire` 收成一个表示。
 * - 换个名字装同一件事（`host: string | null`）—— 词是 `origin`，改名就出人群。
 * - `accounts.ts` 自己那个 `LOCAL_ORIGIN = "__local__"`（账号面的缓存键）**仍在**：
 *   `backend_policy_tests.rs::the_two_same_named_local_origin_constants_stay_deliberately_different`
 *   逐字钉着「两者刻意不同、合并是一次设计变更」，那条判据不在本拍写区 —— 登记给主会话拍板。
 */

/**
 * 还装得下 `null` 的那几处 —— `(仓相对路径 → 为什么今天还在、什么时候能摘)`。**不是豁免清单**：
 * 它与盘上的违例两向相等，修好了不摘 ⇒ 红；新长一处 ⇒ 红。
 */
const PENDING: Record<string, string> = {
  "src/generated/RemoteHealthPayload.ts::RemoteHealthPayload.origin":
    "Rust 出方向 `bridge.rs::RemoteHealthPayload.origin` 仍是 `Option<String>`（头注自认「`None` 理论不该出现」，" +
    "五个发射点现打全是 `Some(host)`）。改成 `String` 要动那五个发射点，而它们全住 `ssh_source.rs` —— " +
    "第四波 SR1a 的写区（单一常驻后端正在重写它）。解锁：SR1a 合并后同拍把字段改成 `String`、" +
    "五处 `Some(x.clone())` 改成 `x.clone()`、重生成绑定，再摘这一行。",
};

/** 基线之后现打的人群条数（带类型标注、名字带 origin 的声明）。 */
const POPULATION = 228; // 〔合并 ST3〕212 ＋ RL1 1 ＋ RM1c 11 ＋ ST3 4 ⇒ 228 // 〔ST3 · 第四波 4B〕+4：包装层 `drift_ledger_report` 的 `args.origin` ＋1 · 生成物 `DriftLedgerReport.origin` ＋1 · `drift-ledger-section.ts` 里 ST2 那两处（`onMachineChanged(origin)` / `applyMachine(origin)`）退场 −2、新进 `machineName(origin)` / `formatReport(…, origin)` / `answersFor(…, origin)` / `DriftLedgerSection.lastOrigin` ＋4（全是 `Origin`，不可空；跑出来核过）； 〔合并 RW1〕+4：`commands.ts` 的 `list_skills` / `read_skill_file` / `write_skill_file` 各一处 `origin: Origin` ＋ `views/inbox-view.ts::InboxView.origin`（远端项目的收件箱也能编辑）；  // 〔RM1c · 第四波〕+11（全是 `Origin`、零可空）：`commands.ts` 包装层 `panorama_call` 的 args ×1 · `panorama/api.ts` 的 `RepoAt.origin` · `remote()` 的 at 形参 · `diagramKinds(origin)` ×3 · `views/panorama.ts` 的 `loadedOrigin` / `origin` 两个字段 ＋ `showRepo` / `switchRepo` 两个形参 ×4 · `panorama/diagram-view.ts` 的 `kindsOrigin` 字段 ＋ `ensureKinds` / `repoChanged` 里两个 `const origin: Origin` ×3；合并时按两边增量相加、跑出来核过 // 〔RL1〕+1（`remote-launch-run.ts::withRelayEndpoint` 的 `origin: string`；包装层 `relay_ensure` → `relay_endpoint_for_launch` 换名不换数）// 〔合并 RW1〕+4：`commands.ts` 的 `list_skills` / `read_skill_file` / `write_skill_file` 各一处 `origin: Origin` ＋ `views/inbox-view.ts::InboxView.origin`（远端项目的收件箱也能编辑）； // 〔合并 RM1a〕+7（RM1a 新代码里带类型的 origin 声明，全是 `Origin`：`commands.ts` 包装层 `write_apikey_credentials_key` 的 args 形状（多行写法那一处）· `read_apikey_credentials_status` / `apikey_routing_for` / `relay_ensure` / `config_surface_report` 四条的 `args.origin` · 生成物 `ConfigSurfaceReport.origin` · `accounts-section.ts` 里 `pendingKeys` 的值形状；不可空那条判据照旧只剩 PENDING 一处，跑出来核过）// 〔合并主线 a0b9a8e0〕+20（主线新代码里带类型的 origin 声明，全都已是 `Origin` / `string`：ST2 的 backend-section 四处 · config-surface-section 三处 · drift-ledger 两处 · remote-section 两处；RM1b 的 tasks-panel 三处；UP1 的 grid-monitor 两处 ＋ 包装层 RM1b / SE2 带 origin 的三条；S4 拆掉 tabs.ts 的 fetchTmuxFresh / killRemoteTmux 转交 −2 —— 跑出来核过）； 〔C4a 子步 3〕+3：`chan.ts::chan.call` 的 `origin` · 包装层 `chan_call` 的 `origin`（远端会话账号那条的 `origin` 随它退役 −1）· `history-search.ts` 的 `parseSessionHitsLines(…, origin)` 与 `origins` 那一格；// 〔C4a 子步 2〕+3：包装层新进的 list_remote_accounts / list_remote_session_accounts / check_account_trust 各带一个 `origin: Origin`

interface Decl {
  /** `文件::宿主.名字`（宿主 = 外层接口 / 类 / 类型别名 / 函数名；顶层是 `<top>`）。 */
  key: string;
  /** 类型标注的原文。 */
  type: string;
  /** 类型里有没有 `null` 字面量类型。 */
  nullable: boolean;
}

const isOriginName = (name: string): boolean => /origin/i.test(name) && !/^original/i.test(name);

function containsNull(node: ts.Node): boolean {
  if (node.kind === ts.SyntaxKind.NullKeyword) return true;
  if (ts.isLiteralTypeNode(node) && node.literal.kind === ts.SyntaxKind.NullKeyword) return true;
  return ts.forEachChild(node, (c) => (containsNull(c) ? true : undefined)) ?? false;
}

function nameOf(n: ts.Node | undefined): string | null {
  if (!n) return null;
  if (ts.isIdentifier(n) || ts.isPrivateIdentifier(n) || ts.isStringLiteral(n)) return n.text;
  return null;
}

/** 外层宿主的名字（给键用，免得同名字段在不同接口里撞成一个）。 */
function hostOf(node: ts.Node): string {
  for (let p: ts.Node | undefined = node.parent; p; p = p.parent) {
    if (
      ts.isInterfaceDeclaration(p) ||
      ts.isClassDeclaration(p) ||
      ts.isTypeAliasDeclaration(p) ||
      ts.isFunctionDeclaration(p) ||
      ts.isMethodDeclaration(p) ||
      ts.isMethodSignature(p)
    ) {
      const n = nameOf(p.name);
      if (n) return n;
    }
  }
  return "<top>";
}

/** 一份 TS 源码里「名字带 origin、带类型标注」的全部声明。 */
function originDecls(rel: string, src: string): Decl[] {
  const sf = ts.createSourceFile(rel, src, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const out: Decl[] = [];
  const push = (node: ts.Node, name: string, type: ts.Node): void => {
    out.push({
      key: `${rel}::${hostOf(node)}.${name}`,
      type: type.getText(sf),
      nullable: containsNull(type),
    });
  };
  const visit = (node: ts.Node): void => {
    // 字段 / 参数 / 变量 / 类属性：名字带 origin 且有类型标注。
    if (
      (ts.isPropertySignature(node) ||
        ts.isPropertyDeclaration(node) ||
        ts.isParameter(node) ||
        ts.isVariableDeclaration(node)) &&
      node.type
    ) {
      const name = nameOf(node.name);
      if (name && isOriginName(name)) push(node, name, node.type);
    }
    // 函数 / 方法的返回类型：函数名**以 origin 收尾**（`pickPrimaryOrigin`）才是「回一个 origin」；
    // `findHostByOrigin` / `resolveRemoteConfigByOrigin` 是「按 origin 找别的东西」，回的不是 origin。
    if (
      (ts.isFunctionDeclaration(node) || ts.isMethodDeclaration(node) || ts.isMethodSignature(node)) &&
      node.type
    ) {
      const name = nameOf(node.name);
      if (name && /origins?$/i.test(name) && !/byorigins?$/i.test(name)) push(node, `${name}()`, node.type);
    }
    // 函数类型别名 `type X = (origin: …) => …` 的参数已由 isParameter 覆盖。
    // `const origins = new Set<…>()` / `new Map<…>()`：元素类型写在类型实参上，不在标注里。
    if (ts.isVariableDeclaration(node) && !node.type && node.initializer) {
      const name = nameOf(node.name);
      const init = node.initializer;
      if (name && isOriginName(name) && ts.isNewExpression(init) && init.typeArguments) {
        for (const ta of init.typeArguments) push(node, `${name}<>`, ta);
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return out;
}

/** 全部前端 TS（含生成物）。复用本文件的 `walk`（**不另起一个遍历**：`scanning-guard-registry` 的棘轮数着
 *  「做目录遍历的测试文件」，另起一份就多一个）；两个 Rust 工程住 `src/` 下，里面没有前端 TS，照样排掉。 */
let originCorpusMemo: Decl[] | null = null;
function originCorpus(): Decl[] {
  // 解析全树要几秒；本节四条共用一份（每条各解析一遍在负载高时会撞 vitest 的 5 秒上限）。
  if (originCorpusMemo) return originCorpusMemo;
  const all: Decl[] = [];
  for (const f of walk(resolve(REPO_ROOT, "src"), ".ts")) {
    const rel = f.slice(REPO_ROOT.length + 1);
    if (rel.startsWith("src/bridge/") || rel.startsWith("src/backend/")) continue;
    all.push(...originDecls(rel, readFileSync(f, "utf8")));
  }
  originCorpusMemo = all;
  return all;
}

describe("〔C4a〕TS 侧 origin 去 null（`设计/05 §8` 步 2，全 TS ＋ 生成物）", { timeout: 30_000 }, () => {
  it("★★ 装得下 `null` 的 origin 声明 == 登记的待办（两向；今天只剩生成物一处）", () => {
    const found = originCorpus()
      .filter((d) => d.nullable)
      .map((d) => d.key)
      .sort();
    expect(
      found,
      "这几处 origin 在类型上装得下 `null`：\n" +
        "本机是一个**具名**的 origin（`LOCAL_ORIGIN` = `\"<local>\"`，住 `backend-policy.ts`，与 Rust 跨语言对拍）；\n" +
        "「是不是本机」只经 `src/ipc/origin.ts` 判（`isLocalOrigin` / `isRemoteOrigin`），\n" +
        "Rust 出方向的「缺省 = 本机」只在 `originFromWire` 那一处收成一个表示。\n" +
        "⇒ 出路是把那一处改成 `Origin`，**不是**把它登记进 PENDING。",
    ).toEqual(Object.keys(PENDING).sort());
  });

  it("★ 人群恒等（不是地板）：名字带 origin、带类型标注的声明现打条数", () => {
    const n = originCorpus().length;
    expect(
      n,
      `人群现打 ${n} 条，钉的是 ${POPULATION} 条。真加/删了一处带类型的 origin 声明就来改这个数（写清多了/少了哪几处）；\n` +
        "★ 掉到 0 多半是 `walk` / 解析口径坏了 —— 那时上一条会拿空集比出绿。",
    ).toBe(POPULATION);
  });

  it("★ 真语料锚点：两处已知声明在人群里，而且都不装 null", () => {
    const all = originCorpus();
    const tab = all.find((d) => d.key === "src/tab-model.ts::Tab.origin");
    expect(tab, "`Tab.origin` 不在人群里 —— 识别器没扫到真树").toBeDefined();
    expect(tab?.nullable).toBe(false);
    const cur = all.find((d) => d.key === "src/account-chip.ts::<top>.pickPrimaryOrigin()");
    expect(cur, "`pickPrimaryOrigin()` 的返回类型不在人群里 —— 返回类型那一支在空转").toBeDefined();
    expect(cur?.nullable).toBe(false);
  });

  it("★ 识别器阳性对照五形 ＋ 阴性对照三形", () => {
    // 反例：「按 origin 找别的东西」回 `X | null` 是合法的（找不到），不是 origin 装 null。
    expect(
      originDecls("probe.ts", "function findHostByOrigin(o: Origin): Host | null { return null; }").filter(
        (d) => d.nullable,
      ),
      "`…ByOrigin()` 回的不是 origin，被当成 origin 判了",
    ).toEqual([]);
    const bad = [
      "interface A { origin: string | null }",
      "function f(origin: string | null): void {}",
      "class C { private wantedOrigin: string | null = null; }",
      "export function pickPrimaryOrigin(): string | null { return null; }",
      "const origins = new Set<string | null>();",
    ];
    for (const src of bad) {
      const ds = originDecls("probe.ts", src);
      expect(ds.length, `识别器在 ${src} 里一处 origin 声明都没摘到`).toBeGreaterThan(0);
      expect(ds.some((d) => d.nullable), `识别器没认出 ${src} 装得下 null`).toBe(true);
    }
    const good = [
      "interface B { originalType: string | null }", // 不是 origin
      "const x = { origin: null };", // 值，不是类型
      "interface D { origin: Origin; remote: string | null }", // 干净；`remote` 不在人群
    ];
    for (const src of good) {
      expect(
        originDecls("probe.ts", src).filter((d) => d.nullable),
        `干净写法 ${src} 被判成违例 —— 假红比不查更坏`,
      ).toEqual([]);
    }
  });
});

// ══════════════════════════════════════════════════════════════════════════
//  〔C4a · 第四波 · 子步 2〕**裸 `invoke` 只在包装层** —— 零命中带正控
//
// 基线 `3c3a094e` 现打：`commands.ts` 之外还有 **16 处**裸 `invoke`（`tab-session-actions.ts` 11 ·
// `accounts.ts` 5），它们调的命令里有 10 条根本不在包装层 ⇒ 名字 / 实参形状 / 返回类型三样都没人钉。
// 这一拍把它们收进来，本节钉住「不许再长出来」。与 `generated-boundary-guard.vitest.ts` 那条
// 「直接 import invoke 的生产文件恰好 1 个」**异源**：那条量 `import` 语句，本条量**调用点**
// （命名空间导入 `import * as core` 之后 `core.invoke(` 那一形 import 那条看不见，本条的第三格补上）。
// ══════════════════════════════════════════════════════════════════════════

/** 一段（已剥注释的）TS 里直呼 `invoke(` 的处数。前面是标识符字符或 `.` 的不算（`myinvoke(` / `this.invoke(`）。 */
function bareInvokeSites(code: string): number {
  return [...code.matchAll(/(?<![A-Za-z0-9_$.])invoke\s*(?:<[\s\S]{0,200}?>)?\s*\(/g)].length;
}

describe("〔C4a〕裸 invoke 只在包装层", () => {
  it("★★ 全仓生产 TS 里直呼 invoke 的文件 == { 包装层 }，且包装层里的处数 == 条目数（正控）", () => {
    const perFile = new Map<string, number>();
    for (const f of walk(resolve(REPO_ROOT, "src"), ".ts")) {
      if (f.includes(".test.") || f.includes(".vitest.")) continue;
      const n = bareInvokeSites(stripComments(readFileSync(f, "utf8"), "ts"));
      if (n > 0) perFile.set(f.slice(REPO_ROOT.length + 1), n);
    }
    expect(
      [...perFile.keys()].sort(),
      "包装层之外又有人直呼 `invoke` 了 —— 那等于两条路并存：名字 / 实参 / 返回类型三样没人钉。\n" +
        "出路：在 `src/ipc/commands.ts` 加一个条目（键名 == 命令名 == 字面量），调用点改 `commands.x(…)`。",
    ).toEqual([WRAPPER_FILE]);
    // 正控：同一个识别器在包装层上数得出每个条目那一处（塌成 0 ⇒ 上面那条是拿空集比出的绿）。
    expect(perFile.get(WRAPPER_FILE), "识别器在包装层上数出的处数 != 条目数").toBe(
      Object.keys(commands).length,
    );
  });

  it("★ 没有绕开 `invoke` 这个名字的第二条路（命名空间导入 / 全局注入对象）", () => {
    const offenders: string[] = [];
    for (const f of walk(resolve(REPO_ROOT, "src"), ".ts")) {
      if (f.includes(".test.") || f.includes(".vitest.")) continue;
      const code = stripComments(readFileSync(f, "utf8"), "ts");
      if (/import\s*\*\s*as\s+\w+\s+from\s*["']@tauri-apps\/api\/core["']/.test(code)) offenders.push(`${f} · import *`);
      if (/__TAURI(?:_INTERNALS)?__/.test(code)) offenders.push(`${f} · __TAURI__`);
    }
    expect(offenders).toEqual([]);
  });

  it("★ 识别器阳性三形 ＋ 阴性三形", () => {
    expect(bareInvokeSites('await invoke("x");')).toBe(1);
    expect(bareInvokeSites('invoke<Record<string, number>>("x", a);')).toBe(1);
    expect(bareInvokeSites("invoke(cmd, args);"), "动态命令名也是直呼").toBe(1);
    expect(bareInvokeSites("invokeLaunchOrCopyFallback(origin, cmd);")).toBe(0);
    expect(bareInvokeSites("this.invoke(x); myinvoke(y);")).toBe(0);
    expect(bareInvokeSites("commands.invoke_thing();")).toBe(0);
  });
});
