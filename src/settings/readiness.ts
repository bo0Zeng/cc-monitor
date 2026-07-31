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
}

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
 * 1. 本机不需要 daemon（`watcher.rs` 直读 jsonl，主计划 §2.4 那张表逐字写着「不需要」）。
 * 2. **本机不需要「连接」**（`INVARIANTS §40`：本地 = **不走 ssh** 的远端）。
 *    **Phase G 逮到的真 bug**：漏了这一条，于是每台新装的机器落地页顶上永久挂着一条
 *    **blocking** 的「本机 · 连接：未测过 —— **连不上这台机器，它上面的会话都看不到**」。
 *    那句话对本机是**假的**，而且它是清单里最重的一级，还没有任何按钮能把它消掉。
 * 3. **S9**：`ccm` 是 POSIX 的 bash 启动器。monitor 跑在 Windows 上时，本机的对应物是
 *    「终端集成」那块 PowerShell $PROFILE 注入（§2.4 表里「本机 · 启动器」一格），
 *    不是 `ccm`。不排掉的话，Windows 用户会在这张专为新用户做的清单上，
 *    读到一条「本机缺 cc 命令」—— 而那条在他机器上压根无从补起。
 *
 * 三条都是同一句话：**把不适用算成缺，会让用户以为自己装漏了东西**。
 */
function notApplicable(
  origin: string,
  facet: MachineFacet,
  hostOs: HostOs,
): boolean {
  if (origin !== LOCAL_MACHINE_KEY) return false;
  if (facet === "daemon" || facet === "connection") return true;
  return facet === "ccm" && hostOs === "windows";
}

export interface ReadinessInput {
  /** 要检查的机器（本机用 `LOCAL_MACHINE_KEY`）。顺序即呈现顺序。 */
  origins: string[];
  /** 读账本。注入进来而不是直接 import，纯函数才好测。 */
  statusOf: (origin: string) => MachineStatus;
  /** daemonless 的机器不需要 daemon —— 那是用户显式选的降级，不是缺件。 */
  isDaemonless?: (origin: string) => boolean;
  /**
   * S9：monitor 跑在哪个 OS 上。**注入而不是直接调 `hostOs()`** ——
   * 这个模块的卖点就是纯函数，`isDaemonless` 当初也是为同一个理由注入的。
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
      if (facet === "daemon" && input.isDaemonless?.(origin)) continue;
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
  const head = g.kind === "missing" ? "缺" : "未测过";
  return `${who} · ${what}：${head} —— ${g.consequence}`;
}
