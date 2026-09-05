// P2s（定框 C8）：**每台机一份 daemon 策略** —— 今天只有一条「monitor 退出时是否结束它」。
//
// # 持久化归这里，生效值归 Rust
//
// `config.rs` 头注逐字「Rust 端**不解释配置内容**（schema 在前端定义）」。
// 若 Rust 也往 config.json 里读写，同一个文件就有**两个写者**，
// 前端「读—改—写整份」的那一刻会拿一份陈旧副本把 Rust 刚写的键覆盖掉。
// ⇒ 这里存盘，改动时与启动时把生效值**推**给 Rust（`set_daemon_kill_on_exit`）。
//
// # ⚠ 「不结束」到底等不等于「继续跑」——**今天要看它有没有真脱离**（K-P1 08-26 翻面）
//
// 翻面之前：daemon 是纯 stdio 子进程，monitor 一退读端就断，它在 **153 毫秒**内自己
// broken-pipe 退出（实测 08-11，P2s §0a）。那时这条策略的真实语义是
// 「立刻结束」与「让它自己退」之差，不是「后台常驻」，而 P2s-Y5 据此立了一条**无条件**禁令。
//
// K-P1 给了 daemon 一个监听口，并让它在 Linux 上**真脱离** ⇒ 那一支上「勾掉」**真的**是继续跑。
// ⇒ 禁令换成**按状态分档**：见下面的 EXIT_* 四条与 describeExitBehavior。
//
// ★★ 「无人监护」这半是用户裁定的一半，不许省（DECISIONS K14 逐字：
// 「第一档必须在 UI 上如实说『继续跑，无人监护』，这是本裁定的一半，不许只做常驻不做这句话」）。

import { commands } from "./ipc/commands";
import { loadConfig, saveConfig, type Config } from "./config";

/**
 * 本机在 origin 这套命名里的名字。
 *
 * ⚠ **跨语言常量**：Rust 侧是 `inbound_client::LOCAL_ORIGIN`。两边漂了**不会报错** ——
 * 本机开关会去操作一个谁都没登记过的 origin，设了没反应且不报错。
 * 由 Rust 侧一条判据逐字对拍（`the_local_origin_is_the_same_string_on_both_sides`）。
 */
export const LOCAL_ORIGIN = "<local>";

/** 缺省：**不结束**（C8③ 的前半句 —— 那半是站得住的）。 */
export const DEFAULT_KILL_ON_EXIT = false;

/**
 * K-P1 KPY4：退出行为的四句话 —— **用户可见文案的唯一一个家**。
 *
 * ⚠ 别把它们抄进 `settings/daemon-section.ts`：那正是这一格要防的事。
 * 现行判据（K-P1 之前那条）`readFileSync` 的**只有一个文件**、只剥整行注释
 * ⇒ 文案搬家 / 拼串 / 进一张 i18n 表就**零命中地绿**。
 * 翻转时同轮扩了人群（daemon-section.ts + daemon-policy.ts），并配了一个今天真会红的反向锚点。
 *
 * ⚠ Rust 侧 `src-tauri/src/daemon_policy.rs` 有同名同值的四条 const，
 * 由 `the_exit_copy_is_the_same_string_on_both_sides` 逐字对拍
 * （形状抄 `the_local_origin_is_the_same_string_on_both_sides`）。**两边漂了不会报错。**
 */
/**
 * ① 勾上「退出时结束它」。
 *
 * ⚠ 它与那个复选框的**标签**刻意不是同一个串（标签是「monitor 退出时结束它」）：
 * 标签说的是**这个开关是什么**，这一句说的是**接下来会发生什么**。
 * 写成同一个串的话，「那四句只许有一个家」那条判据会把复选框的标签算成第二个家 ——
 * 而那**不是误报**：两处一模一样的串，下一次改文案时一定只会改到一处。
 */
export const EXIT_KILLS = "monitor 退出时会结束它";
/** ② 勾掉 + **真脱离了**。★ 「无人监护」是 K14 背书的那半，不许删。 */
export const EXIT_UNATTENDED =
  "monitor 退出后它继续跑，无人监护：崩了不会自动重起；下次开 monitor 会接上它，接不上才起一个新的";
/** ③ 勾掉 + **没脱离**（平台不支持 / 被关掉了 / 脱离失败）⇒ 保持今天那句，一字不改。 */
export const EXIT_SELF_DIES = "monitor 不主动结束它；它仍会在 monitor 退出后很快自行退出";

// ══════════════════════════════════════════════════════════════════════════
// K-P3 KP3C（09-04）：**那句「无人监护」后面接的那个读数。**
//
// K14 逐字要的是「如实说『继续跑，无人监护』」，而 K-P1 KPY4 已经把那句话钉住了。
// 本件加的那一半是：那句话后面要能接上一个**真读数**（上次崩没崩、崩过几次），
// 而不是永远只是一句静态承诺。
//
// ★★ 这三句话最要紧的一句是 HEALTH_UNKNOWN，而它买的正是 K-P3 §0-1 那一格：
//    今天不是「它没崩过」，是「**没有任何东西在记它崩没崩**」——
//    §0-1 逐字：「这两句话差得很远，件计划里不许混用」。
//    ⇒ 读数的默认档是**答不出来**，不是「没崩过」。把它写成后者就是把一句
//    查不出来的事说成了一个绿灯。
//
// ⚠ 这三句**不进 describeExitBehavior 的返回值**。那个函数被
//   `settings/daemon-section.vitest.ts` 用**等号**逐格钉着（它自己逐字写着
//   「不是「包含」而是「等于」——「包含」会放过「在正确那句后面又加了一句错的」」），
//   在它后面接一句就是当场红那四格。⇒ 读数是**另一句话**，由界面另起一行说。
//
// ⚠ 占位符（{misread} / {crashed} / {last}）是**两侧共用的字面**：Rust 那侧
//   `daemon_policy.rs` 有同名同值的四条 const，由
//   `every_cross_language_table_is_compared_on_both_sides` 逐字对拍
//   （形状抄 `the_exit_copy_is_the_same_string_on_both_sides`，只是人群从
//   `CROSS_LANGUAGE_COPY` 那张清单派生 ⇒ 加一张新表不配对拍会当场红）。
//   **两边漂了不会报错**，所以改文案要同一拍改两处。
// ══════════════════════════════════════════════════════════════════════════

/**
 * ① 账上一条都没有 ⇒ **答不出来**。
 *
 * ⚠ 这一句刻意不说「没崩过」。今天那本账**不跨 monitor 进程**
 * （唯一的持久账 `~/.cc-monitor/bin/wrap.log` 现打 2 行、停在 07-08、两行都 rc=0，
 * 而且仓里没有任何一处写它 —— 一本没有写者的孤账）。
 */
export const HEALTH_UNKNOWN =
  "上次崩没崩：答不出来 —— 今天没有任何东西在跨 monitor 进程地记它崩没崩，而「答不出来」不等于「没崩过」";
/** ② 记到过事，但**一次崩溃都没有**。读坏了那一格单独说，它不算崩。 */
export const HEALTH_CLEAN =
  "这次 monitor 开着以来：它一次都没崩过（读坏了 {misread} 次不算它崩 —— 那是我们这一侧的读端）";
/** ③ 崩过。带次数与最后那一行。 */
export const HEALTH_CRASHED =
  "这次 monitor 开着以来：它崩过 {crashed} 次，最后一次是「{last}」";
/** ④ 崩过但那一行没留住（表被清过 / 锁毒化）—— 也要说出口，不许拿空串糊过去。 */
export const HEALTH_LAST_MISSING = "那一行没留下来";

/**
 * 一台机的死亡账**读数**。四个计数分开装 —— 「读坏了」不许被算成一次崩溃。
 *
 * ⚠ 那一条是 B1 那次事故的全部内容（`backend/control/local_backend.rs:696` 与 `:1348`）：
 * 一个坏字节让 `InvalidData` 与 EOF 走同一条路 ⇒ 消费者返回 = 判死 ⇒ 记一次「崩溃」，
 * 三次之后整个进程周期不再起来，日志写「崩了 3 次」——源码逐字：「**一个错误的诊断**」。
 */
export interface DaemonHealth {
  crashed: number;
  refused: number;
  neverStarted: number;
  misread: number;
  last: string | null;
}

/**
 * 那句读数 —— 三档。**纯函数**，三档在单测里逐格钉得死。
 *
 * ⚠ 第一档的判准是「**这本账上一条记录都没有**」，不是「crashed === 0」。
 * 写成后者的话，一台从来没被记过的机器会被说成「一次都没崩过」——
 * 那正是 §0-1 点名不许混用的那两句话。
 */
export function describeDaemonHealth(h: DaemonHealth): string {
  const seen = h.crashed + h.refused + h.neverStarted + h.misread;
  if (seen === 0) return HEALTH_UNKNOWN;
  if (h.crashed === 0) return HEALTH_CLEAN.replace("{misread}", String(h.misread));
  return HEALTH_CRASHED.replace("{crashed}", String(h.crashed)).replace(
    "{last}",
    h.last ?? HEALTH_LAST_MISSING,
  );
}

/** 一台机此刻的退出行为。`detached` 来自 `daemon_status`（远端恒 null ⇒ 按未脱离算）。 */
export interface ExitState {
  killOnExit: boolean;
  detached: boolean;
}

/**
 * 这台机现在**退出时会发生什么** —— 一句话，三档（与 K-P1 §0b-4 那张表逐格对应）。
 *
 * ★ 它是纯函数：三档在单测里逐格钉得死，不需要真起一个 daemon。
 * ⚠ **「无人监护」只许出现在真脱离那一档**（②）——
 * 出现在①③ 就是承诺一件做不到的事，而那正是 P2s-Y5 当初立禁令要防的东西。
 *
 * ⚠ ① 为什么**不看** `detached`：退出钩子对**两条起法都收**
 * （`lib.rs` 的 `RunEvent::Exit` 那一段：被监护的走 `stop()`，脱离的走 `stop_local_backend()`）。
 * 曾经有过第四档「已经脱离了 ⇒ 这个勾管不到它」—— 那一档是**那个缺口的产物**，
 * 缺口补上之后它就成了一句假话，随缺口一起删掉了。
 * ⚠ 残留的诚实边界（**不进文案，进日志**）：接管来的实例是按记录下来的 pid 去停的，
 * 那份记录缺了或 pid 被复用时停不掉 —— 那时 `lib.rs` 会 `warn!` 出来，而不是静默。
 */
export function describeExitBehavior(s: ExitState): string {
  if (s.killOnExit) return EXIT_KILLS;
  return s.detached ? EXIT_UNATTENDED : EXIT_SELF_DIES;
}

/** config.json 里的键。 */
const KEY = "daemonPolicy";

export type DaemonPolicy = Record<string, boolean>;

/** 从整份 config 里取策略表；缺席/形状不对都退回空表（**不抛**——开关坏了不该拖垮设置页）。 */
export function readPolicy(cfg: Config): DaemonPolicy {
  const raw = cfg[KEY];
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return {};
  const out: DaemonPolicy = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof v === "boolean") out[k] = v;
  }
  return out;
}

/** 这台机退出时结不结束它。未登记 ⇒ 缺省。 */
export function killOnExit(policy: DaemonPolicy, origin: string): boolean {
  return policy[origin] ?? DEFAULT_KILL_ON_EXIT;
}

/** 把整表推给 Rust（启动时一次）。**逐台推**——Rust 那边是 per-origin 的表，没有「整表覆盖」这个口。 */
export async function pushPolicyToBackend(policy: DaemonPolicy): Promise<void> {
  for (const [origin, kill] of Object.entries(policy)) {
    await commands.set_daemon_kill_on_exit({ origin, kill });
  }
}

/** 启动时：读盘 → 推给 Rust。返回读到的表，供 UI 初始化用。 */
export async function initDaemonPolicy(): Promise<DaemonPolicy> {
  const policy = readPolicy(await loadConfig());
  await pushPolicyToBackend(policy);
  return policy;
}

/**
 * 改一台机的策略：**先推后存**。
 *
 * 顺序是刻意的 —— 推失败就不落盘，否则盘上写着 A 而运行中是 B，
 * 下次启动才「自动修好」，中间那段时间用户看到的开关是骗人的。
 */
/**
 * 写盘串行链〔D 阶段补审 08-11，A3〕。
 *
 * # 原来错在哪
 *
 * `setKillOnExit` 是 `push → loadConfig → 改 → saveConfig(整份)`，**四个 await 之间没有锁**。
 * 同一个复选框快点两下（或先后点两台机）：
 *
 * 1. T1 `push(A=true)` 到达 Rust；
 * 2. T2 `push(A=false)` 到达 Rust ⇒ **Rust 表 = false**；
 * 3. T2 的读—改—写先完成，盘上 = false；
 * 4. T1 的 `saveConfig` 后完成，**用它那份陈旧 cfg 覆盖回 true** ⇒ 盘上 = true。
 *
 * 结果：UI 勾 = false、运行时 = false、**盘上 = true**
 * ⇒ 下次启动 `initDaemonPolicy` 把 true 推回去，**开关自己翻过来**。
 * 而且 `saveConfig(cfg)` 是整份写，会连带把这期间别的设置区写入的键一起回滚。
 *
 * ★ 讽刺的是本文件头注花了 8 行论证「不能有两个写者」—— 第二个写者出现在了
 * **前端自己的并发点击**里。
 *
 * ⇒ 用一条 promise 链把写盘串起来：**同一时刻只有一个读—改—写在跑**。
 * ⚠ 它**不跨进程**（另一个 monitor 实例照样能覆盖），那一格如实登记在件里。
 */
let writeChain: Promise<unknown> = Promise.resolve();

export async function setKillOnExit(origin: string, kill: boolean): Promise<void> {
  if (!origin.trim()) throw new Error("origin 不许为空 —— 策略是每台机各一份的");
  // 前一笔失败不该卡住后一笔 ⇒ 链上先吞掉错误再排队；错误仍原样抛给**本次**调用方。
  const run = writeChain.catch(() => {}).then(async () => {
    await commands.set_daemon_kill_on_exit({ origin, kill });
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const policy = readPolicy(cfg);
    policy[origin] = kill;
    cfg[KEY] = policy;
    await saveConfig(cfg);
  });
  writeChain = run.catch(() => {});
  return run;
}
