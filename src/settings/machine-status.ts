/**
 * S3（settings-ia）：机器列表行上那几个状态格子的**账本**。
 *
 * # 它为什么不去查状态
 *
 * 主计划 §1-2 是条红线：**状态灯绝不引入轮询**。依据是 `INVARIANTS §41`
 * （daemon 生产段零定时器，`no_timer_guard` 钉住）以及 `cc-bus-section.ts` /
 * `config-surface-section.ts` 文件头各自写死的「不新增轮询」。
 *
 * 「打开设置页时顺便把 N 台机器都探一遍」听起来不像轮询，但它是同一件事的另一种说法：
 * 一次 UI 动作扇出 N 次 ssh 往返，而用户根本没要求。所以本模块**只记录用户动作的结果**
 * ——测了连接就记连接、装了 daemon 就记 daemon——并把**记录时刻**一起存下来。
 *
 * ⇒ 行上显示的永远是「上次那次动作的结论 + 它有多旧」，绝不伪装成实时。
 * 没动作过就明说「未测过」，不猜、不填一个好看的 ✓。
 *
 * # 为什么持久化
 *
 * 存 localStorage（UI 缓存，**不是权威数据**）。跨重启保留是对的：时间戳一起显示，
 * 旧就是旧，诚实即可。不留的话每次重开 monitor 列表都是一片「未测过」，
 * 那这一栏就没有存在意义了。
 */

import { LS_KEYS, safeGet, safeSet } from "../local-storage";

/**
 * S3：本机在账本里的 key。
 *
 * 本机没有 origin（它不走 ssh），但它在列表里是一行，那几个格子也要有地方存。
 * 用一个**不可能与真实 origin 撞车**的名字：origin 来自 `label || host`，
 * 用户填不出带空格和中文括号的 host，label 也不会长这样。
 */
export const LOCAL_MACHINE_KEY = "（本机）";

/** 行上的一个格子。 */
export type MachineFacet =
  | "connection"
  | "daemon"
  | "ccm"
  | "acctIso"
  | "accounts";

/** 面板上从左到右的显示顺序（也是 §2.3 那张示意图里的顺序）。 */
export const MACHINE_FACETS: readonly MachineFacet[] = [
  "connection",
  "daemon",
  "ccm",
  "acctIso",
  "accounts",
];

export const FACET_LABELS: Record<MachineFacet, string> = {
  connection: "连接",
  daemon: "daemon",
  ccm: "ccm",
  acctIso: "acct-iso",
  accounts: "账号",
};

export interface FacetState {
  /**
   * - `ok` / `fail`：上次动作的结论。
   * - `na`：**不适用** —— 例如本机不需要 daemon（主计划 §2.4 逐字写着「不需要」，
   *   `watcher.rs` 直读 jsonl）。**这和「没测过」是两回事**：混成一个值的话，
   *   用户会以为本机缺了个组件。「没测过」的表示是**这个 facet 压根不在表里**。
   */
  kind: "ok" | "fail" | "na";
  /** 可选的一句话细节：版本号、账号数、失败原因摘要。 */
  detail?: string;
  /** 记录时刻（epoch ms）。`na` 也带，但渲染时不显示年龄（不适用没有新鲜度可言）。 */
  at: number;
}

export type MachineStatus = Partial<Record<MachineFacet, FacetState>>;

type Ledger = Record<string, MachineStatus>;

function loadLedger(): Ledger {
  const raw = safeGet(LS_KEYS.machineStatus);
  if (!raw) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    // 存档是用户机器上的文件，可能被手改/被旧版本写坏。**读坏了就当空**，
    // 绝不让一个坏掉的缓存把设置页炸掉（它只是缓存，丢了无所谓）。
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as Ledger;
    }
  } catch {
    /* 见上：坏存档一律当空 */
  }
  return {};
}

function saveLedger(l: Ledger): void {
  safeSet(LS_KEYS.machineStatus, JSON.stringify(l));
}

/** 记一次动作结果。`at` 缺省取现在（测试可注入）。 */
export function recordFacet(
  origin: string,
  facet: MachineFacet,
  state: Omit<FacetState, "at"> & { at?: number },
): void {
  if (!origin) return;
  const l = loadLedger();
  const cur = l[origin] ?? {};
  cur[facet] = { ...state, at: state.at ?? Date.now() };
  l[origin] = cur;
  saveLedger(l);
}

/**
 * 读某台机器的状态。**同步、纯读、绝不发起任何 IO** —— 这是本模块存在的全部理由，
 * 由 `machine-status.vitest.ts` 里那条「渲染列表时对 ipc/commands 零调用」钉住。
 */
export function readStatus(origin: string): MachineStatus {
  return loadLedger()[origin] ?? {};
}

/** 机器被删 / 改名时清掉旧记录，免得下一台同名机器继承上一台的结论。 */
export function forgetMachine(origin: string): void {
  const l = loadLedger();
  if (!(origin in l)) return;
  delete l[origin];
  saveLedger(l);
}

/** 机器改名：把记录挪过去（否则改个名字状态就凭空清零）。 */
export function renameMachine(from: string, to: string): void {
  if (from === to || !from || !to) return;
  const l = loadLedger();
  const cur = l[from];
  if (!cur) return;
  delete l[from];
  l[to] = cur;
  saveLedger(l);
}

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/**
 * 「多久以前」。**刻意粗粒度**：这里的数字是给人判断「这条结论还能不能信」用的，
 * 不是计时器。精确到秒只会让人误以为它在实时刷新。
 */
export function formatAge(at: number, now: number = Date.now()): string {
  const d = now - at;
  // 负数（时钟回拨 / 存档来自另一台机器）也落进这一档 —— 刻意**不**单独写一个
  // `if (d < 0)`：那条分支被这一条完全覆盖，是等价死码（变异验证当场证实：
  // 把它的条件改成永假，行为一字不变）。
  if (d < MIN) return "刚刚";
  if (d < HOUR) return `${Math.floor(d / MIN)} 分钟前`;
  if (d < DAY) return `${Math.floor(d / HOUR)} 小时前`;
  return `${Math.floor(d / DAY)} 天前`;
}

/** 一个格子渲染成什么。`undefined` = 从没记录过。 */
export function describeFacet(
  state: FacetState | undefined,
  now: number = Date.now(),
): { icon: string; text: string; tone: "ok" | "fail" | "na" | "unknown" } {
  if (!state) {
    // **不猜**。没测过就写没测过——填个好看的 ✓ 是在替用户下一个他没做过的结论。
    return { icon: "·", text: "未测过", tone: "unknown" };
  }
  if (state.kind === "na") {
    // 不适用没有新鲜度可言，不带时间。
    return { icon: "—", text: state.detail ?? "不需要", tone: "na" };
  }
  const icon = state.kind === "ok" ? "✓" : "✗";
  const head = state.detail ? `${state.detail} · ` : "";
  return {
    icon,
    text: `${head}${formatAge(state.at, now)}`,
    tone: state.kind,
  };
}
