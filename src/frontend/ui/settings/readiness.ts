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
import { copyText } from "../copy-table";

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
  missing: copyText("readiness.gapHead.missing"),
  /** **从没测过 / 查不动** —— 说「缺」就是替用户下一个他没做过的结论。 */
  unknown: copyText("readiness.gapHead.untested"),
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
   * 那天落成的唯一失败面 `CCM_RC_NO_BACKEND=4`。
   * 〔步 8 · 条 80〕唯一那个生产者（旧 `daemonless` 配置的指名告知）删掉了 ⇒ **今天零个**。
   */
  code?: string;
}

// 🔴 〔步 8 · 条 80 「不要管旧配置」〕`NO_BACKEND_GAP_CODE` / `NO_BACKEND_CONSEQUENCE`
//    与它们背后那条 `legacyNoBackend` 指名告知**整块删了**。它们只为「认出盘上那个
//    已退役的 `daemonless: true`」而存在，而用户裁了不再管旧配置。
//    ⇒ `Gap.code` 这一格今天**没有任何生产者**，但字段留着：它本来就是给「需要用户
//      认得出、说得出的迁移告知」预留的口，下一条这种告知照这个形状加。

/** 每个 facet 缺席时的后果与轻重。**写在一处**，别散到 UI 里各说各话。 */
const FACET_MEANING: Record<
  MachineFacet,
  { consequence: string; severity: "blocking" | "optional" }
> = {
  connection: {
    consequence: copyText("readiness.meaning.connection"),
    severity: "blocking",
  },
  backend: {
    consequence: copyText("readiness.meaning.source"),
    severity: "blocking",
  },
  ccm: {
    consequence: copyText("readiness.meaning.cc"),
    severity: "optional",
  },
  accounts: {
    consequence: copyText("readiness.meaning.accounts"),
    severity: "optional",
  },
};

/**
 * 某台机器上**不适用**的项 —— 不适用不是缺。
 *
 * 1. 🔴 **这里此前排掉的第一项是「本机的 `backend`」，`K-R59`（09-11）把它撤了。**
 *    当年的理由逐字是「本机不需要后端（`watcher.rs` 直读 jsonl，主计划 §2.4
 *    那张表逐字写着「不需要」）」—— 那句话在 `C7`〔用 08-03〕之后就**不成立**了：
 *    `C7` 逐字「没有 daemonless，使用软件就要有后端 ⇒ **本机**也要有后端进程」，
 *    `local_backend.rs` 就是它的产物。
 *    ⇒ 本机后端**今天真的存在**，而这张「还差什么」的清单当时永远不会告诉用户它没起来。
 *    ⚠ **这一条一个 `daemonless` 字样都不含，却与它同一档** —— 换个说法留着同一条退路，
 *    数名字的判据一格都不会红（`KR59D1` 的失效方向逐字写着这件事）。
 * 2. **本机不需要「连接」**（`INVARIANTS §40`：本地 = **不走 ssh** 的远端）。
 *    **Phase G 逮到的真 bug**：漏了这一条，于是每台新装的机器落地页顶上永久挂着一条
 *    **blocking** 的「本机 · 连接：未测过 —— **连不上这台机器，它上面的会话都看不到**」。
 *    那句话对本机是**假的**，而且它是清单里最重的一级，还没有任何按钮能把它消掉。
 * 3. 〔S9〕从前这里另排掉 Windows 本机的 `ccm`（那时 Windows 上没有写点，不排掉就是一条永远消不掉的「未测过」）。
 *    〔WF1 · `99 §2.2 ㉔`〕Windows 上也有写点了（`ccm_probe::probe_via_fresh_powershell`：新开的 PowerShell 里敲 `ccm` 走到哪）
 *    ⇒ 这一条连同它要的 `hostOs` 入参一起删了，两平台同一条规则。
 *
 * 都是同一句话：**把不适用算成缺，会让用户以为自己装漏了东西**。
 */
function notApplicable(origin: string, facet: MachineFacet): boolean {
  return origin === LOCAL_MACHINE_KEY && facet === "connection";
}

export interface ReadinessInput {
  /** 要检查的机器（本机用 `LOCAL_MACHINE_KEY`）。顺序即呈现顺序。 */
  origins: string[];
  /** 读账本。注入进来而不是直接 import，纯函数才好测。 */
  statusOf: (origin: string) => MachineStatus;
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
  for (const origin of input.origins) {
    const st = input.statusOf(origin);
    for (const facet of MACHINE_FACETS) {
      if (notApplicable(origin, facet)) continue;
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

/**
 * 一句人话摘要，给折叠标题用。空列表返回 `null`（调用方据此整块不渲染）。
 *
 * 〔W5-VIS · `设计/15 §4.5` 缺口三〕**摘要按轻重分开说**：「必需：…；可选：…」。
 * 原先只分「确认缺 / 没测过」，blocking 的后端与 optional 的 ccm 在摘要那一行读起来一模一样
 * （信息在 `Gap.severity` 里，只是摘要没说）。每一档里照旧区分「缺」与「没测过」；只有一档就只说那一档。
 */
export function summarizeGaps(gaps: Gap[]): string | null {
  if (gaps.length === 0) return null;
  const groups: string[] = [];
  for (const severity of ["blocking", "optional"] as const) {
    const inGroup = gaps.filter((g) => g.severity === severity);
    if (inGroup.length === 0) continue;
    const missing = inGroup.filter((g) => g.kind === "missing").length;
    const unknown = inGroup.length - missing;
    const parts: string[] = [];
    // **措辞刻意区分**：确认缺的说「缺」，没测过的说「没测过」。
    if (missing > 0) parts.push(copyText("readiness.gaps.missing", { missing }));
    if (unknown > 0) parts.push(copyText("readiness.gaps.unknown", { unknown }));
    const joined = parts.join(copyText("readiness.gaps.sep"));
    groups.push(
      severity === "blocking"
        ? copyText("readiness.gaps.blocking", { parts: joined })
        : copyText("readiness.gaps.optional", { parts: joined }),
    );
  }
  return groups.join(copyText("readiness.gaps.groupSep"));
}

/** 一条条目的显示文案。 */
export function describeGap(g: Gap): string {
  const who = g.origin === LOCAL_MACHINE_KEY ? copyText("readiness.gap.local") : g.origin;
  const what = FACET_LABELS[g.facet];
  // 措辞取自 [`GAP_HEAD`]，**不在这里再写一遍**（`K-R65`：那一对词有两个读者了）。
  const head = GAP_HEAD[g.kind];
  return `${who} · ${what}：${head} —— ${g.consequence}`;
}
