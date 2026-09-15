/**
 * S5 / E56（settings-ia）：「**还差什么、点哪里补齐**」。
 *
 * 与「改动足迹」是一体两面 —— 那页答「我在你机器上装过 / 写过什么」，这里答「还差什么」。
 * 用户 2026-07-31 的要求：新用户能直接上手、依赖一站式备齐。
 *
 * # 数据从哪来：只读 S3 的账本，**不探测**
 *
 * 全部结论来自 `machine-status` 账本（用户动作留下的记录 + 时刻）。
 * **不发任何请求**（主计划 §1-2：状态灯绝不引入轮询）。一个「新用户装好就打开设置」
 * 的场景里，账本是空的 —— 那本来就该显示成「还没测过」，而不是替他跑一遍 N 台机器的 ssh。
 *
 * # 「缺」与「不知道」是两回事，**不能混**
 *
 * 这是本模块最容易做错的地方，也是 S3 那套账本设计的延续：
 * - `missing` —— **测过、确认没有**（账本里是 `fail`）。可以理直气壮说「缺」。
 * - `unknown` —— **从没测过**（账本里压根没这一格）。说「缺」就是替用户下一个他没做过的
 *   结论；一个刚装好、什么都没点过的新用户会看到一屏红叉，而事实只是「还没测」。
 *
 * 所以这里产出的是**两类**条目，UI 必须分开呈现。
 */

import {
  MACHINE_FACETS,
  FACET_LABELS,
  LOCAL_MACHINE_KEY,
  type MachineFacet,
  type MachineStatus,
} from "./machine-status";
import type { HostOs } from "./host-os";

export type GapKind = "missing" | "unknown";

/**
 * 🔴 **「缺」与「不知道」这两个字的唯一住址。**
 *
 * 〔`K-R65` 09-11 抽出来〕在此之前它是 [`describeGap`] 里的一句三目表达式。
 * 抽出来的理由是**它有了第二个读者**：配置面审计那一页（`config-surface-section.ts`）
 * 也要把 `absent`（查了、确认没有）与 `undetermined`（查不动）**显示成两回事**，
 * 而件计划 `KR65D1` 逐字写着「`readiness.ts` 那条 `missing` vs `unknown` 的分法
 * **是现成的，别再造一套**」。
 *
 * ⇒ 两页用同一对词。改这里，两页一起改；在别处再写一句 `? "缺" : "未测过"`
 * 就是第二个住址。
 */
export const GAP_HEAD: Record<GapKind, string> = {
  /** **测过、确认没有** —— 可以理直气壮说「缺」。 */
  missing: "缺",
  /** **从没测过 / 查不动** —— 说「缺」就是替用户下一个他没做过的结论。 */
  unknown: "未测过",
};

export interface Gap {
  /** 哪台机器（`LOCAL_MACHINE_KEY` = 本机）。 */
  origin: string;
  facet: MachineFacet;
  kind: GapKind;
  /** 缺了它有什么后果 —— 让用户自己判断值不值得补，而不是只丢一个 ✗。 */
  consequence: string;
  /**
   * `blocking` = 不补就用不了（连不上 / 没有数据源）；
   * `optional` = 补了更好用（终端起会话、多账号…）。
   */
  severity: "blocking" | "optional";
  /**
   * `K-R59`：**这条缺口的名字**。绝大多数缺口没有名字（它们由 `origin`+`facet` 唯一确定），
   * 只有需要用户**认得出、说得出**的那种迁移告知才配一个 —— 形状抄 `K-P2`
   * 那天落成的唯一失败面 `CCM_RC_NO_BACKEND=4`。今天只有一个：[`NO_BACKEND_GAP_CODE`]。
   */
  code?: string;
}

/**
 * `K-R59` / `KR59D3`：**旧配置里那个 `true` 的唯一失败面**，有名字。
 *
 * 它进 DOM（`remote-section.ts` 把它写成 `data-code`），所以用户与判据看见的是同一个串。
 * ⚠ **不许把这条降成一句 `console.warn`** —— 日志不是失败面，用户看不见的告知等于没有。
 */
export const NO_BACKEND_GAP_CODE = "REMOTE_NO_BACKEND";

/** [`NO_BACKEND_GAP_CODE`] 的人话：**为什么** + **下一步**，两半缺一不可。 */
export const NO_BACKEND_CONSEQUENCE =
  "这台机器此前是按「不装后端」配的（旧的 daemonless 开关），而今天没有那一档了 —— " +
  "打开它的机器卡片装上后端；装完保存一次，这条就消失";

/** 每个 facet 缺席时的后果与轻重。**写在一处**，别散到 UI 里各说各话。 */
const FACET_MEANING: Record<
  MachineFacet,
  { consequence: string; severity: "blocking" | "optional" }
> = {
  connection: {
    consequence: "连不上这台机器，它上面的会话都看不到",
    severity: "blocking",
  },
  daemon: {
    consequence: "没有数据源，这台机器的会话不会出现在 tab 里",
    severity: "blocking",
  },
  ccm: {
    consequence: "终端里没有 cc 命令；从终端起的会话 app 也认不出",
    severity: "optional",
  },
  acctIso: {
    consequence: "不能在这台机器上按账号隔离地起会话",
    severity: "optional",
  },
  accounts: {
    consequence: "还没读过这台机器上有哪些账号",
    severity: "optional",
  },
};

/**
 * 某台机器上**不适用**的项 —— 不适用不是缺。
 *
 * 1. 🔴 **这里此前排掉的第一项是「本机的 `daemon`」，`K-R59`（09-11）把它撤了。**
 *    当年的理由逐字是「本机不需要 daemon（`watcher.rs` 直读 jsonl，主计划 §2.4
 *    那张表逐字写着「不需要」）」—— 那句话在 `C7`〔用 08-03〕之后就**不成立**了：
 *    `C7` 逐字「没有 daemonless，使用软件就要有后端 ⇒ **本机**也要有后端进程」，
 *    `backend/control/local_backend.rs` 就是它的产物。
 *    ⇒ 本机后端**今天真的存在**，而这张「还差什么」的清单当时永远不会告诉用户它没起来。
 *    ⚠ **这一条一个 `daemonless` 字样都不含，却与它同一档** —— 换个说法留着同一条退路，
 *    数名字的判据一格都不会红（`KR59D1` 的失效方向逐字写着这件事）。
 * 2. **本机不需要「连接」**（`INVARIANTS §40`：本地 = **不走 ssh** 的远端）。
 *    **Phase G 逮到的真 bug**：漏了这一条，于是每台新装的机器落地页顶上永久挂着一条
 *    **blocking** 的「本机 · 连接：未测过 —— **连不上这台机器，它上面的会话都看不到**」。
 *    那句话对本机是**假的**，而且它是清单里最重的一级，还没有任何按钮能把它消掉。
 * 3. **S9**：`ccm` 是 POSIX 的 bash 启动器。monitor 跑在 Windows 上时，本机的对应物是
 *    「终端集成」那块 PowerShell $PROFILE 注入（§2.4 表里「本机 · 启动器」一格），
 *    不是 `ccm`。不排掉的话，Windows 用户会在这张专为新用户做的清单上，
 *    读到一条「本机缺 cc 命令」—— 而那条在他机器上压根无从补起。
 *
 *    🔴 **〔`K-R69` 09-12〕这一条的前提翻了一半，而结论今天没跟着翻 —— 两件事都写在这里。**
 *
 *    - **翻了的那半**：「`ccm` 是 POSIX 的 bash 启动器」**今天不成立**。`K-R48` 第二拍把
 *      `shared/ccm` 那份 1592 行 bash 删了（`K33` 逐字「不要有什么 bash 脚本，
 *      不要有什么单独的 ccm」），今天的 `ccm` **就是后端二进制本身**；`K-R69` 起
 *      monitor 会把它放到 `~/.cc-monitor/bin/ccm`，**Windows 上那一份叫 `ccm.exe`，
 *      一样在**（名字的真相源是 `backend/control/local_backend.rs::local_ccm_entry_name`，
 *      后缀由 `build.rs` 按 `TARGET` 算）。⇒ 「在他机器上压根无从补起」这句已经是假话。
 *    - **没跟着翻的那半（本轮刻意不动，理由可证伪）**：撤掉这一条会让 Windows 用户
 *      **永久**多一条「本机 · ccm：未测过」—— 因为**全仓对本机 `ccm` 这一格
 *      一个 `recordFacet` 写点都没有**（下面那条「诚实边界」判据自己就写着这件事）。
 *      把「不适用」换成一条永远消不掉的「未测过」，是拿一种假话换另一种。
 *      ⇒ 撤它要与「谁来记这一格」同拍做，而**那是产品决定**（`ok` 的判据是
 *      「我们那一份装下来了」还是「终端里敲 `ccm` 走到的就是它」？两者今天可以不同 ——
 *      见 `ccm_probe::PathCcmVerdict` 的四态）。`K-R69` 的三条 DoD 都不含它 ⇒
 *      **不自批、交回 PM**（`brief` 17：题目比该做的窄一格时，改题不是实现方能自批的事）。
 *
 * 三条都是同一句话：**把不适用算成缺，会让用户以为自己装漏了东西**。
 */
function notApplicable(
  origin: string,
  facet: MachineFacet,
  hostOs: HostOs,
): boolean {
  if (origin !== LOCAL_MACHINE_KEY) return false;
  if (facet === "connection") return true;
  return facet === "ccm" && hostOs === "windows";
}

export interface ReadinessInput {
  /** 要检查的机器（本机用 `LOCAL_MACHINE_KEY`）。顺序即呈现顺序。 */
  origins: string[];
  /** 读账本。注入进来而不是直接 import，纯函数才好测。 */
  statusOf: (origin: string) => MachineStatus;
  /**
   * `K-R59` / `KR59D3`：盘上**仍写着旧「不装后端」开关**的主机
   * （`remote-config.ts` 的 `legacyNoBackend`，口径 `hostKey`）。
   *
   * ⚠ **它不是那个开关的替身** —— 它不改变任何数据源路径，只让这台机器的
   * `daemon` 那一格换成一条**指名的**告知（[`NO_BACKEND_GAP_CODE`]），
   * 而不是通用的「没测过」。省略 = 盘上没有旧配置。
   */
  legacyNoBackend?: (origin: string) => boolean;
  /**
   * S9：monitor 跑在哪个 OS 上。**注入而不是直接调 `hostOs()`** ——
   * 这个模块的卖点就是纯函数（`K-R59` 之前那个 `isDaemonless` 当初也是为同一个理由注入的）。
   * 省略 = 按非 Windows 处理（`ccm` 照常算数）。
   */
  hostOs?: HostOs;
}

/**
 * 算出「还差什么」。**纯函数**，不碰 IO。
 *
 * 顺序：先 `blocking` 后 `optional`；同级内按传入的机器顺序、再按 facet 的固定顺序 ——
 * 让列表稳定，不会因为账本里键的枚举顺序而跳动。
 */
export function computeGaps(input: ReadinessInput): Gap[] {
  const blocking: Gap[] = [];
  const optional: Gap[] = [];
  const os = input.hostOs ?? "unknown";
  for (const origin of input.origins) {
    const st = input.statusOf(origin);
    for (const facet of MACHINE_FACETS) {
      if (notApplicable(origin, facet, os)) continue;
      // `KR59D3`：旧配置里那个 `true` **不许被静默吞掉**。它压过账本 ——
      // 账本里那一格今天多半是 `na`（旧路径把「用户选了降级」记成不适用），
      // 而那正是要被撤掉的那句话。**这条告知有名字**，见 `NO_BACKEND_GAP_CODE`。
      if (facet === "daemon" && input.legacyNoBackend?.(origin)) {
        blocking.push({
          origin,
          facet,
          kind: "missing",
          code: NO_BACKEND_GAP_CODE,
          consequence: NO_BACKEND_CONSEQUENCE,
          severity: "blocking",
        });
        continue;
      }
      const cur = st[facet];
      // `na` = 不适用，不是缺（账本里也可能显式记成 na）。
      if (cur?.kind === "ok" || cur?.kind === "na") continue;
      const meaning = FACET_MEANING[facet];
      const gap: Gap = {
        origin,
        facet,
        kind: cur?.kind === "fail" ? "missing" : "unknown",
        consequence: meaning.consequence,
        severity: meaning.severity,
      };
      (meaning.severity === "blocking" ? blocking : optional).push(gap);
    }
  }
  return [...blocking, ...optional];
}

/** 一句人话摘要，给折叠标题用。空列表返回 `null`（调用方据此整块不渲染）。 */
export function summarizeGaps(gaps: Gap[]): string | null {
  if (gaps.length === 0) return null;
  const missing = gaps.filter((g) => g.kind === "missing").length;
  const unknown = gaps.length - missing;
  const parts: string[] = [];
  // **措辞刻意区分**：确认缺的说「缺」，没测过的说「没测过」。
  if (missing > 0) parts.push(`${missing} 项确认缺`);
  if (unknown > 0) parts.push(`${unknown} 项还没测过`);
  return parts.join("，");
}

/** 一条条目的显示文案。 */
export function describeGap(g: Gap): string {
  const who = g.origin === LOCAL_MACHINE_KEY ? "本机" : g.origin;
  const what = FACET_LABELS[g.facet];
  // 措辞取自 [`GAP_HEAD`]，**不在这里再写一遍**（`K-R65`：那一对词有两个读者了）。
  const head = GAP_HEAD[g.kind];
  return `${who} · ${what}：${head} —— ${g.consequence}`;
}
