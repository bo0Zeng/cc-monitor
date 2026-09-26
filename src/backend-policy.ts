// P2s（定框 C8）：**每台机一份后端策略** —— 今天只有一条「monitor 退出时是否结束它」。
//
// # 〔B2 · 条 66 · `设计/01 §3.3b`〕那个值**不住这里**，也不住 monitor 进程里
//
// 它住**后端所在那台机器**上（后端自己的状态文件，一台机器一个值），**只有后端写**。
// 本文件与 `settings/backend-section.ts` 读它、改它都经两条后端命令
// （`backend_exit_policy` / `set_backend_exit_policy` → 后端 `exit-policy-read` / `exit-policy-set`）。
// ⇒ 搬家前那一套（存进 monitor 的 config.json、启动时与改动时**推**给 Rust、一条写盘串行链）
//   **整条删掉**：留着就是第二个真相源（`§3.3b ②`）。原来那条「两个写者」的担心换了解法 ——
//   不是「让 Rust 别写」，是「让两边写不同的文件」，后端那份前端从不碰（单写者）。
//
// # ⚠ 「不结束」到底等不等于「继续跑」——**今天要看它有没有真脱离**（K-P1 08-26 翻面）
//
// 翻面之前：backend 是纯 stdio 子进程，monitor 一退读端就断，它在 **153 毫秒**内自己
// broken-pipe 退出（实测 08-11，P2s §0a）。那时这条策略的真实语义是
// 「立刻结束」与「让它自己退」之差，不是「后台常驻」，而 P2s-Y5 据此立了一条**无条件**禁令。
//
// K-P1 给了后端一个监听口，并让它在 Linux 上**真脱离** ⇒ 那一支上「勾掉」**真的**是继续跑。
// ⇒ 禁令换成**按状态分档**：见下面的 EXIT_* 四条与 describeExitBehavior。
//
// ★★ 「无人监护」这半是用户裁定的一半，不许省（DECISIONS K14 逐字：
// 「第一档必须在 UI 上如实说『继续跑，无人监护』，这是本裁定的一半，不许只做常驻不做这句话」）。
//
// 〔CP2b · 第四波〕下面那几句话的**家搬进了文案表**（`src/shared/copy/table.json` 的 `backendPolicy.*`）。
//   这里的导出名照旧（界面与测试按名字取），值从表里取；Rust 那侧从前那份「同名同值、逐字对拍」的副本
//   随之删掉 —— 两侧读的是同一条表项，第二份副本与对拍一起没了（`backend_policy.rs::describe_health` 直接取同一批 key）。

import { copyText } from "./copy-table";


/**
 * 本机在 origin 这套命名里的名字。
 *
 * ⚠ **跨语言常量**：Rust 侧是 `inbound_client::LOCAL_ORIGIN`。两边漂了**不会报错** ——
 * 本机开关会去操作一个谁都没登记过的 origin，设了没反应且不报错。
 * 由 Rust 侧一条判据逐字对拍（`the_local_origin_is_the_same_string_on_both_sides`）。
 */
export const LOCAL_ORIGIN = "<local>";

// 〔B2〕缺省值（不结束）**不在这里** —— 它住后端（`control/exit_policy.rs` 的那个常量），
//   后端回的 `killOnExit` 已经是套过缺省的生效值。这里再写一份就是两处缺省各自漂。

/**
 * K-P1 KPY4：退出行为的那几句话 —— **用户可见文案的唯一一个家**。
 * 〔B2 · 条 66〕三句变四句：多了「读不出来」那一句（`EXIT_UNREADABLE`）。
 * ⚠ `设计/01 §3.3b ⑤` 写的是「四句变五句」—— 那是假前提（`W20` 现打：今天是三句），本次现打仍是 3 → 4。
 *
 * ⚠ 别把它们抄进 `settings/backend-section.ts`：那正是这一格要防的事。
 * 现行判据（K-P1 之前那条）`readFileSync` 的**只有一个文件**、只剥整行注释
 * ⇒ 文案搬家 / 拼串 / 进一张 i18n 表就**零命中地绿**。
 * 翻转时同轮扩了人群（backend-section.ts + backend-policy.ts），并配了一个今天真会红的反向锚点。
 *
 * 〔CP2b〕Rust 侧从前有同名同值的四条 const 与一条逐字对拍；家搬进文案表之后两侧读同一条表项，副本与对拍一起删了。
 */
/**
 * ① 勾上「退出时结束它」。
 *
 * ⚠ 它与那个复选框的**标签**刻意不是同一个串（标签是「monitor 退出时结束它」）：
 * 标签说的是**这个开关是什么**，这一句说的是**接下来会发生什么**。
 * 写成同一个串的话，「那四句只许有一个家」那条判据会把复选框的标签算成第二个家 ——
 * 而那**不是误报**：两处一模一样的串，下一次改文案时一定只会改到一处。
 */
export const EXIT_KILLS = copyText("backendPolicy.exit.kills");
/** ② 勾掉 + **真脱离了**。★ 「无人监护」是 K14 背书的那半，不许删。 */
export const EXIT_UNATTENDED = copyText("backendPolicy.exit.unattended");
/** ③ 勾掉 + **没脱离**（平台不支持 / 被关掉了 / 脱离失败）⇒ 保持今天那句，一字不改。 */
export const EXIT_SELF_DIES = copyText("backendPolicy.exit.selfDies");
/**
 * ④〔B2 · 条 66 · `设计/01 §3.3b ⑤`〕那台机器上的值**读不出来** ⇒ 按缺省（不结束）办 ——
 * **并且说出来这不是谁选的**。
 *
 * ⚠ 它与「没人选过」（文件不在）**不是一回事**：没人选过就是缺省，说的是上面②③那两句；
 * 读不出来是「有一份东西在那儿、我们读不懂」，照缺省办，但不许装作那是一个选择。
 * 同一条道理仓里立过一次：`HEALTH_UNKNOWN` 逐字「『答不出来』不等于『没崩过』」。
 */
export const EXIT_UNREADABLE = copyText("backendPolicy.exit.unreadable");

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
//   `settings/backend-section.vitest.ts` 用**等号**逐格钉着（它自己逐字写着
//   「不是「包含」而是「等于」——「包含」会放过「在正确那句后面又加了一句错的」」），
//   在它后面接一句就是当场红那四格。⇒ 读数是**另一句话**，由界面另起一行说。
//
// 〔CP2b〕占位符（{crashed} / {last} …）住表项里，由 copyText 填；Rust 那侧 `describe_health` 取的是同一批 key，
//   两侧用的 key 集合由 `backend_policy_tests.rs::both_sides_describe_health_with_the_same_keys` 比。
// ══════════════════════════════════════════════════════════════════════════

/**
 * ① 账上一条都没有 ⇒ **无记录**。
 *
 * ⚠ 这一格刻意不说「没崩过」。今天那本账**不跨 monitor 进程**
 * （唯一的持久账 `~/.cc-monitor/bin/wrap.log` 现打 2 行、停在 07-08、两行都 rc=0，
 * 而且仓里没有任何一处写它 —— 一本没有写者的孤账）。
 *
 * 🔴 〔第四波 ST2 · `设计/70 §2.3` · 第二刀 步 6〕原来这一格是 40 字的一整句
 * （「上次崩没崩：答不出来 —— 今天没有任何东西在跨 monitor 进程地记它崩没崩，而……」）。
 * `§2.2` 逐字：**区分本身是对的，不能一起扫掉** ⇒ 区分保留、换成界面状态：
 * 格子里只写「— 无记录」，那句「为什么这不等于没崩过」进 ⓘ（[`HEALTH_UNKNOWN_WHY`]）。
 */
export const HEALTH_UNKNOWN = copyText("backendPolicy.health.unknown");
/**
 * ①′ 「无记录」那一格的 ⓘ —— **那条区分的全部内容住这里**（`§2.2`：不许一起扫掉）。
 * 只在 TS 这一侧（Rust 那侧的 `describe_health` 不产它），所以不进跨语言表。
 */
export const HEALTH_UNKNOWN_WHY = copyText("backendPolicy.health.unknownWhy");
/** ② 记到过事，但**一次崩溃都没有**。读坏了几次、被拒几次进 `[详情]`（[`HEALTH_DETAIL`]）。 */
export const HEALTH_CLEAN = copyText("backendPolicy.health.clean");
/** ③ 崩过。带次数与最后那一次的**短摘要**（判定 ＋ 退出状态，后端 `last_brief`，不是账行）。 */
export function healthCrashed(crashed: number, last: string): string {
  return copyText("backendPolicy.health.crashed", { crashed: String(crashed), last });
}
/** ④ 崩过但那一次没留住（表被清过 / 锁毒化）—— 也要说出口，不许拿空串糊过去。 */
export const HEALTH_LAST_MISSING = copyText("backendPolicy.health.lastMissing");
/**
 * ⑤ `[详情]` 里那一段：四个计数**分开**列（「读坏了」不许被算成一次崩溃），外加完整记录去哪看。
 * 只在 TS 这一侧（界面专用），不进跨语言表。
 */
// 〔CP2b〕整句住文案表 `backendPolicy.health.detail`（四个计数是它的占位符）。

/**
 * 一台机的死亡账**读数**。四个计数分开装 —— 「读坏了」不许被算成一次崩溃。
 *
 * ⚠ 那一条是 B1 那次事故的全部内容（`backend/control/local_backend.rs:696` 与 `:1348`）：
 * 一个坏字节让 `InvalidData` 与 EOF 走同一条路 ⇒ 消费者返回 = 判死 ⇒ 记一次「崩溃」，
 * 三次之后整个进程周期不再起来，日志写「崩了 3 次」——源码逐字：「**一个错误的诊断**」。
 */
export interface BackendHealth {
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
export function describeBackendHealth(h: BackendHealth): string {
  const seen = h.crashed + h.refused + h.neverStarted + h.misread;
  if (seen === 0) return HEALTH_UNKNOWN;
  if (h.crashed === 0) return HEALTH_CLEAN;
  return healthCrashed(h.crashed, h.last ?? HEALTH_LAST_MISSING);
}

/**
 * `[详情]` 里那一段。**账上一条都没有 ⇒ `null`**（那时格子里是「— 无记录」＋ ⓘ，没有详情可展开）。
 */
export function describeHealthDetail(h: BackendHealth): string | null {
  const seen = h.crashed + h.refused + h.neverStarted + h.misread;
  if (seen === 0) return null;
  return copyText("backendPolicy.health.detail", {
    crashed: String(h.crashed),
    refused: String(h.refused),
    neverStarted: String(h.neverStarted),
    misread: String(h.misread),
  });
}

/** 那个值现读出来的三态（与后端 `exit_policy::Read::state` 逐字对齐）。 */
export type ExitPolicyState = "chosen" | "absent" | "unreadable";

/**
 * 一台机此刻的退出行为。
 *
 * - `policy` / `killOnExit`：**后端答的**（`exit-policy-read`）；
 * - `detached`：来自 `backend_status`（远端恒 null ⇒ 按未脱离算）。
 *
 * 〔S5 · 第四波 · V105 清账〕原来还有一格 `shell`（后端答「这一趟是哪个壳」），
 * 为的是「折进前端进程」那一档：那一档生命周期没得选，整格说「不适用」（E4）。
 * 那一档已放弃（`99 §1` V105），壳只剩独立进程 ⇒ 这一格、那一支、判据 E4 同拍删掉。
 */
export interface ExitState {
  policy: ExitPolicyState;
  killOnExit: boolean;
  detached: boolean;
}

/**
 * 这台机现在**退出时会发生什么** —— 一句话，四档。
 *
 * ★ 它是纯函数：每一档在单测里逐格钉得死，不需要真起一个后端。
 * ⚠ **「无人监护」只许出现在真脱离那一档**（②）——
 * 出现在别处就是承诺一件做不到的事，而那正是 P2s-Y5 当初立禁令要防的东西。
 *
 * ⚠ 判断的**顺序**是承重的：
 * ① 读不出来 ⇒ 说读不出来（**不看** `killOnExit` —— 那是套过缺省的值，不是谁选的）；
 * ② 勾上 ⇒ 会结束它（**不看** `detached`：常驻那条由后端在最后一个客户走时自己退，
 *    被监护那条由 monitor 退出臂收）；
 * ③ 没勾 ⇒ 看它有没有真脱离。
 * ⚠ 曾经有过一档「已经脱离了 ⇒ 这个勾管不到它」—— 那一档是**一个缺口的产物**，
 * 缺口补上之后它就成了一句假话，随缺口一起删掉了。
 */
export function describeExitBehavior(s: ExitState): string {
  if (s.policy === "unreadable") return EXIT_UNREADABLE;
  if (s.killOnExit) return EXIT_KILLS;
  return s.detached ? EXIT_UNATTENDED : EXIT_SELF_DIES;
}
