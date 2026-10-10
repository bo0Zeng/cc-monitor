/**
 * 「这台上的 cc-monitor」（后端开关）：本机 ＋ 每台远端各一行，状态 · 起 / 停 · 「随 cc-monitor 退出停止」· 健康。
 * 四格并进机器列表那一行上（`cellsFor`）；本机也有一份，所以不挂在 SSH 配置卡片上。
 *
 * 退出时会发生什么按机器分档（脱没脱离 × 勾没勾），那一句由那台后端出成品（`exit-policy-read` 的 `said`），本文件原样摆、不判哪一档。
 * 那个勾的值住后端所在那台：画之前问（`exit-policy-read`）、改勾交后端写（`exit-policy-set`），画的是写完读回来的那一份，
 * 都经通道直接问那台后端（[`askExitPolicy`] / [`putExitPolicy`]）。问不到 ⇒ 勾禁用、那一行不说话 ——
 * 不知道那台机器上是什么就不替它说，更不拿本地缺省值画一个看起来能用的勾。
 */

import { commands, type StopAnswer } from "../ipc/commands";
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, peerVersionSaid, readJson, saidFrom, unreadableFrom } from "../ipc/chan-caller";
import { isLocalOrigin, type Origin } from "../ipc/origin";
import { hostFacts } from "./host-os";
import { askLocalCcm } from "./machine-aliases";

/** 起/停之后轮询状态的次数与间隔 —— 命令是「发出去就返回」的，不轮询看到的是操作前的状态。 */
const SETTLE_TRIES = 30;
const SETTLE_INTERVAL_MS = 100;
import { toast, failToast } from "../kit/toast";
import { resync, resyncSaid } from "../resync";
import { emit } from "@tauri-apps/api/event";
import { RESYNC_DONE_EVENT } from "./events";
import { makeInfoIcon } from "./info-icon";
import { LOCAL_ORIGIN } from "../backend-policy";
import { copyText } from "../copy-table";
import { formatBytes } from "../format";
import { confirmDialog, type ConfirmFn } from "../kit/dialog";
import { machineName } from "../control-said";
import { askInterrupts, interruptRows, type Interrupts } from "./interrupts";
import { button } from "../kit/button";
import { toggleSwitch } from "../kit/switch";
import { readRecordDrift } from "../record-reads";
import { ccRow } from "./cc-row";
import { decodeMachineState, type MachineState } from "./machine-state";
import { exactKeys, isObj } from "../ipc/decode";
import { sayFailure, sayWithDetail } from "../kit/detail";

/** 「停」的结局说一句（三个词各一句，穷举 —— 多一个词 tsc 就红）；机器页那一行照它说。 */
export function stopSaid(a: StopAnswer): string {
  const pid = a.pid === null ? "" : String(a.pid);
  switch (a.stopped) {
    case "graceful":
      return copyText("backend.stopSaid.graceful", { pid });
    case "killed":
      return copyText("backend.stopSaid.killed", { pid });
    case "not_running":
      return copyText("backend.stopSaid.notRunning");
  }
}

/** 那个值现读出来的三态（与后端 `exit_policy::Read::state` 逐字对齐）。 */
type ExitPolicyState = "chosen" | "absent" | "unreadable";

/** 后端 `exit-policy-read` / `exit-policy-set` 回的那一份里，本区要用的三格（`said` 是成品那一句）。 */
interface ExitAnswer {
  policy: ExitPolicyState;
  killOnExit: boolean;
  said: string;
  /** 只在 `unreadable` 时有：复制详情（原话在这里，不在那一句）。 */
  detail: string;
}

/** 从后端那份不透明 JSON 里取「退出行为」两格；缺一格 / 形状不对 ⇒ `null`（＝ 问不到，不替后端补缺省值）。 */
function readExitAnswer(raw: unknown): ExitAnswer | null {
  if (typeof raw !== "object" || raw === null) return null;
  const v = raw as Record<string, unknown>;
  const policy =
    v.state === "chosen" || v.state === "absent" || v.state === "unreadable" ? v.state : null;
  if (policy === null || typeof v.killOnExit !== "boolean") return null;
  // 成品那一句缺了 / 空了 ⇒ 当问不到（不替后端补一句）。
  if (typeof v.said !== "string" || v.said === "") return null;
  return { policy, killOnExit: v.killOnExit, said: v.said, detail: typeof v.detail === "string" ? v.detail : "" };
}

/** 「退出行为」那两问的期限：10 秒。 */
const EXIT_POLICY_BUDGET_MS = 10_000;

/** 问那台机器（本机也一样）「退出行为」的值（`exit-policy-read`，经通道），原样交 [`readExitAnswer`] 收。失败就抛（说法住 `chan-caller.ts::saidFrom`）。 */
async function askExitPolicy(origin: Origin): Promise<unknown> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(EXIT_POLICY_BUDGET_MS);
    return readJson(await chan.call(origin, "exit-policy-read", body, budget));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

/** 交那台机器写那个值：后端 `exit-policy-set`，经通道；回**写完读回来**的那一份。失败就抛。 */
async function putExitPolicy(origin: Origin, kill: boolean): Promise<unknown> {
  try {
    const body = jsonBody({ killOnExit: kill });
    const budget = budgetWithin(EXIT_POLICY_BUDGET_MS);
    return readJson(await chan.call(origin, "exit-policy-set", body, budget));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

/** 那台后端的 stderr 诊断文件尾部（后端 `backend-log` 的应答）。 */
export interface BackendLog {
  path: string | null;
  size: number;
  text: string;
  truncated: boolean;
}

/** `backend-log` 那一份 ⇒ [`BackendLog`]；缺格 / 形状不对 ⇒ `null`。 */
export function readBackendLog(v: unknown): BackendLog | null {
  const o = v as Partial<BackendLog> | null;
  if (!o || typeof o !== "object") return null;
  if (!(o.path === null || typeof o.path === "string")) return null;
  if (typeof o.size !== "number" || typeof o.text !== "string" || typeof o.truncated !== "boolean") return null;
  return { path: o.path, size: o.size, text: o.text, truncated: o.truncated };
}

/** 问那台机器的后端要它的诊断文件尾部（经通道、只读面）。失败 / 形状不对就抛（一句人话）。 */
async function askBackendLog(origin: Origin): Promise<BackendLog> {
  let raw: unknown;
  try {
    const body = jsonBody({});
    const budget = budgetWithin(EXIT_POLICY_BUDGET_MS);
    raw = readJson(await chan.call(origin, "backend-log", body, budget));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
  const log = readBackendLog(raw);
  if (!log) throw unreadableFrom(origin, "backend-log reply shape");
  return log;
}

/** 那一份怎么摆：头一行（路径 · 大小 · 截没截）＋ 正文；没落文件 ⇒ 只一行说清。 */
export function backendLogLines(log: BackendLog): { head: string; body: string } {
  if (log.path === null) return { head: copyText("backend.log.none"), body: "" };
  const head = copyText("backend.log.head", { path: log.path, size: formatBytes(log.size) });
  return {
    head: log.truncated ? `${head} ${copyText("backend.log.truncated")}` : head,
    body: log.text === "" ? copyText("backend.log.empty") : log.text,
  };
}

/**
 * 「健康」那一格的成品（后端 `backend_policy.rs::health_face` 出，经 `backend_status` 的 `health`）。
 * 本文件只排版：`summary` 进格子 · `state` 原样挂 `data-health` · `why` 在就摆 ⓘ · `detail` 在就摆［详情］；哪一档一处都不在前端判。
 * 形状由金样 `tests/__fixtures__/backend-health.golden.json` 两侧同读钉住。
 */
export interface HealthFace {
  state: string;
  summary: string;
  why: string | null;
  detail: string | null;
}

/** 成品的键集（排好序）。多一格 / 少一格都收不下。 */
const HEALTH_FACE_KEYS = ["detail", "state", "summary", "why"] as const;

/**
 * 按形状严格收：键集恰好那四个 · `state` / `summary` 非空串 · `why` / `detail` 非空串或 `null`。收不下 ⇒ `null`，那一格说「格式不对」。
 * 不回落成「无记录」：`backend_status` 与界面同一个构建，缺格只能是程序错 ⇒ 说出来，不替后端编一档。
 */
export function decodeHealthFace(raw: unknown): HealthFace | null {
  if (!isObj(raw) || !exactKeys(raw, HEALTH_FACE_KEYS)) return null;
  const v = raw;
  const text = (x: unknown): x is string => typeof x === "string" && x !== "";
  const textOrNull = (x: unknown): x is string | null => x === null || text(x);
  if (!text(v.state) || !text(v.summary) || !textOrNull(v.why) || !textOrNull(v.detail)) return null;
  return { state: v.state, summary: v.summary, why: v.why, detail: v.detail };
}

/** 一台机那一行的四栏：栏名只在这里写一次，表头与每一格的 `data-col` 都从这张表来。 */
// 做成函数、用到时才取文（顶层不留取文口调用）。
export const BACKEND_COLUMNS = () =>
  [
    ["state", copyText("backend.column.status")],
    ["exit", copyText("backend.column.exit")],
    ["drift", copyText("backend.cc.drift")],
    ["ops", copyText("backend.column.actions")],
  ] as const;
export type BackendColumn = ReturnType<typeof BACKEND_COLUMNS>[number][0];

/** 列表里有、后端清单里没有的那一台 —— 「未登记」那一格的 ⓘ。 */
export const BACKEND_UNREGISTERED_WHY = (): string =>
  copyText("backend.unregistered.why");

/** 宿主往「这台上的 cc-monitor」里挂的东西：插在退出那一行后面的几行 · 底行右侧那一颗。 */
export interface CellsExtra {
  rows?: HTMLElement[];
  trailing?: HTMLElement;
}

/** 一台机在这一区里的身份。`origin` 是唯一键，`title` 只给人看。 */
interface Machine {
  origin: string;
  title: string;
}

export class BackendSection {
  readonly element: HTMLElement;
  private rows = new Map<string, HTMLElement>();
  /** origin → 那一台的四格容器（挂在机器列表那一行上）。重画与起停只认这里。 */
  private cellHosts = new Map<string, HTMLElement>();
  /** origin → 那一台「随 cc-monitor 退出停止」那个开关。 */
  private killSwitches = new Map<string, ReturnType<typeof toggleSwitch>>();


  /** 寄居模式：四格挂在机器列表那一行上（`cellsFor`），本块自己的 `element` 只装后端清单里有、机器列表里没有的那几台。 */
  private readonly hosted: boolean;
  private readonly onChannel: ((origin: string, connected: boolean | null) => void) | undefined;
  private readonly onMachine: ((origin: string, machine: MachineState | null) => void) | undefined;
  /** 后端清单（`backend_machines`）。`null` = 还没问到 —— 那时寄居的四格先不画。 */
  private registered: Set<string> | null = null;

  /** 停之前那一问 · 数会话那一问（注入缝：判据换成同步答 / 假数据）。 */
  private readonly confirm: ConfirmFn;
  private readonly interrupts: (origin: Origin) => Promise<Interrupts | null>;

  constructor(
    opts: {
      headless?: boolean;
      hosted?: boolean;
      confirm?: ConfirmFn;
      interrupts?: (origin: Origin) => Promise<Interrupts | null>;
      /** 每次问完那台的连接（`null` ＝ 没问到）。 */
      onChannel?: (origin: string, connected: boolean | null) => void;
      /** 每次问完那台的状态成品（收不下 / 没问到 ⇒ `null`）。 */
      onMachine?: (origin: string, machine: MachineState | null) => void;
    } = {},
  ) {
    this.onChannel = opts.onChannel;
    this.onMachine = opts.onMachine;
    this.hosted = opts.hosted ?? false;
    this.confirm = opts.confirm ?? confirmDialog;
    this.interrupts = opts.interrupts ?? askInterrupts;
    this.element = document.createElement("div");
    this.element.className = "settings-section backend-section";
    if (!opts.headless) {
      const h = document.createElement("h3");
      h.textContent = copyText("backend.ctor.title");
      this.element.appendChild(h);
    }
    if (!this.hosted) {
      const hint = document.createElement("p");
      hint.className = "settings-hint";
      // 这一句不承诺任何一种退出行为：它按机器分档，住在每一行里、由那台后端出成品。
      hint.textContent = copyText("backend.ctor.intro");
      this.element.appendChild(hint);
      // 表格式四栏：状态 / 操作 / 退出行为 / 健康。
      this.element.appendChild(BackendSection.columnHead());
    }
    this.list = document.createElement("div");
    this.list.className = "backend-list";
    this.element.appendChild(this.list);
    void this.refresh();
  }

  private list: HTMLElement;

  /**
   * 机器列表那一行上的四格：每次调都建一份新的（列表每重建一次来要一次），旧的随旧行摘掉。
   * 这台若正以「列表里没有」的身份挂在本块自己的列表里，那一行当场收掉 —— 一台机只许一份四格。
   */
  cellsFor(origin: string, extra: CellsExtra = {}): HTMLElement {
    this.rows.get(origin)?.remove();
    this.rows.delete(origin);
    const cells = this.buildCells(origin, extra);
    this.paintOne(origin);
    return cells;
  }

  /** 问题行［重试］/［更新］之后：叫那条流当场重拨（断着就不等退避），再问一次状态。失败就抛。 */
  async reconnect(origin: string): Promise<void> {
    await commands.backend_start({ origin });
    await this.paintStatus(origin);
  }

  /** 重问一台（⋯ →「刷新」）。 */
  async refreshOrigin(origin: string): Promise<void> {
    await this.paintStatus(origin);
  }

  /** 画一台：后端清单还没到 ⇒ 等 `refresh`；清单里没有 ⇒ 说没登记；有 ⇒ 问状态。 */
  private paintOne(origin: string): void {
    if (this.registered === null) return;
    if (!this.registered.has(origin)) {
      this.paintUnregistered(origin);
      return;
    }
    void this.paintStatus(origin);
  }

  /**
   * 列表里有、后端清单里没有（远端模式关着 / 改了配置还没重启 / 重名被后端改了名）。
   * 起 / 停 / 退出行为对它都**没有把手** ⇒ 那几格不摆控件，状态那一格说清为什么。
   */
  private paintUnregistered(origin: string): void {
    const cells = this.cellHosts.get(origin);
    if (!cells) return;
    const state = cells.querySelector<HTMLElement>(".backend-row-state");
    if (state) {
      state.textContent = copyText("backend.paintUnregistered.unregistered");
      state.dataset.on = "unregistered";
      state.after(makeInfoIcon(BACKEND_UNREGISTERED_WHY()));
    }
    for (const col of ["ops", "exit", "drift"] as const) {
      cells.querySelector<HTMLElement>(`[data-col="${col}"]`)?.replaceChildren();
    }
    cells.querySelector<HTMLElement>('[data-col="state"] .machine-cc-controls')?.replaceChildren();
  }

  /**
   * 重拉机器清单 ＋ 每台的状态。寄居模式下：机器列表那一行来要过四格的只重画；没来要过的（重名被后缀化 · 远端模式关着时列表照样列全部）
   * 在本块自己的列表里另起一行，不丢；列表那一行晚到时由 `cellsFor` 把它收掉。
   */
  async refresh(): Promise<void> {
    const machines = await this.machines();
    this.registered = new Set(machines.map((m) => m.origin));
    this.list.innerHTML = "";
    this.rows.clear();
    for (const m of machines) {
      const hostedCells = this.hosted ? this.cellHosts.get(m.origin) : undefined;
      if (hostedCells?.isConnected) {
        void this.paintStatus(m.origin);
        continue;
      }
      const row = this.buildRow(m);
      this.rows.set(m.origin, row);
      this.list.appendChild(row);
      void this.paintStatus(m.origin);
    }
    if (this.hosted) {
      for (const [origin, cells] of this.cellHosts) {
        if (cells.isConnected && !this.registered.has(origin)) this.paintUnregistered(origin);
      }
    }
  }

  /**
   * 机器清单问后端要（注册表就是源头），不自己按 label 拼：前端拼法与 Rust 的 origin 分叉过 ——
   * 不 trim / 重复 label 后缀化 / 远端总开关关着时一个都没注册 / 启动后新增的机器不在注册表里。
   * 本机永远在第一行：它只是不走 ssh 的那一台。
   */
  private async machines(): Promise<Machine[]> {
    try {
      const origins = await commands.backend_machines();
      return origins.map((origin) => ({
        origin,
        title: isLocalOrigin(origin) ? copyText("backend.machine.local") : origin,
      }));
    } catch (e) {
      console.warn(`[P2s] 问后端要机器清单失败，只显示本机：${String(e)}`);
      return [{ origin: LOCAL_ORIGIN, title: copyText("backend.machine.local") }];
    }
  }

  /** 四栏的表头。栏名与 `buildRow` 里每一格的 `data-col` 一一对应（`BACKEND_COLUMNS`）。 */
  static columnHead(): HTMLElement {
    const head = document.createElement("div");
    head.className = "settings-hint";
    head.dataset.backendColumns = "head";
    for (const [col, title] of BACKEND_COLUMNS()) {
      const cell = document.createElement("span");
      cell.dataset.col = col;
      cell.textContent = title;
      head.appendChild(cell);
    }
    return head;
  }

  /**
   * 一台机那一行：名字 ＋ 四格。每一格是带 `data-col` 的容器，里面沿用 `.backend-row-*` 类名（判据按类名找）；
   * 分栏这一维走 `data-*`，不新造 CSS 类。
   */
  private buildRow(m: Machine): HTMLElement {
    const row = document.createElement("div");
    row.className = "backend-row";
    row.dataset.origin = m.origin;

    const name = document.createElement("span");
    name.className = "backend-row-name";
    name.textContent = m.title;
    row.appendChild(name);
    row.appendChild(this.buildCells(m.origin));
    return row;
  }

  /** 「重新对齐」：问那台后端 `resync`（整机），结果说一句。 */
  private async realign(origin: string, btn: HTMLButtonElement): Promise<void> {
    btn.disabled = true;
    try {
      const r = await resync(origin);
      // 手动兜底：本机那一行对齐 ⇒ 顺手作废「PATH 上的 ccm」那份 5 分钟缓存（这台有没有那份缓存由壳说）。
      if (isLocalOrigin(origin) && hostFacts().ccmPathCache) {
        askLocalCcm(true).catch((e: unknown) => console.warn("[resync] 本机 ccm 那一格没重问：", e));
      }
      toast(copyText("backend.resync.doneTitle"), resyncSaid(r), { level: "info" });
      void emit(RESYNC_DONE_EVENT, { origin }); // ㉟①：主窗口标出这台上记录没了的固定条
    } catch (e) {
      failToast(copyText("backend.resync.failed"), e);
    } finally {
      btn.disabled = false;
    }
  }

  /**
   * 「这台上的 cc-monitor」一行一项：状态（版本 · 运行中 · 本次无异常退出 ＋［停止…］［重启］/［启动］）·
   * 随 cc-monitor 退出停止（开关）·（宿主挂进来的几行，如恢复命令）· 未识别内容 · 底行［最近输出］［刷新］…［卸载…］。
   * 每一行的那一格容器带 `data-col`，重画只认它。
   */
  private buildCells(origin: string, extra: CellsExtra = {}): HTMLElement {
    const cells = document.createElement("div");
    cells.dataset.backendCells = origin;
    const col = (name: BackendColumn, row: HTMLElement): HTMLElement => {
      row.dataset.col = name;
      cells.appendChild(row);
      return row;
    };

    // ── 状态 ──
    const help = document.createElement("div");
    const state = document.createElement("span");
    state.className = "backend-row-state";
    state.textContent = copyText("backend.buildCells.querying");
    const sep = document.createElement("span");
    sep.textContent = copyText("kit.text.sep");
    const healthCol = document.createElement("span");
    healthCol.dataset.col = "health";
    const health = document.createElement("span");
    health.className = "backend-row-health";
    healthCol.appendChild(health);
    help.append(state, sep, healthCol);
    const stop = button({ label: copyText("backend.buildCells.stop"), size: "compact", onClick: () => void this.act(origin, "stop") });
    stop.dataset.op = "stop";
    const restart = button({ label: copyText("backend.buildCells.restart"), size: "compact", onClick: () => void this.act(origin, "restart") });
    restart.dataset.op = "restart";
    const start = button({ label: copyText("backend.buildCells.start"), size: "compact", onClick: () => void this.act(origin, "start") });
    start.dataset.op = "start";
    start.style.display = "none";
    col("state", ccRow(copyText("backend.cc.state"), help, [stop, restart, start]));

    // ── 随 cc-monitor 退出停止 ──（值问那台的后端要；问到之前开关不可拨。下面那一行是后端的成品 `said`。）
    const kill = toggleSwitch({
      label: copyText("backend.buildCells.exitKills"),
      on: false,
      onChange: (on) => this.toggleKill(origin, on),
    });
    kill.root.classList.add("backend-row-kill");
    kill.input.setAttribute("aria-disabled", "true");
    const exit = document.createElement("div");
    exit.className = "backend-row-exit";
    kill.root.querySelector("span")?.appendChild(exit);
    const exitRow = document.createElement("div");
    exitRow.className = "machine-cc-row";
    exitRow.appendChild(kill.root);
    col("exit", exitRow);
    this.killSwitches.set(origin, kill);

    for (const r of extra.rows ?? []) cells.appendChild(r);

    // ── 未识别内容（那台记下的认不出的会话流种类）──
    const drift = document.createElement("div");
    drift.className = "backend-row-drift";
    col("drift", ccRow(copyText("backend.cc.drift"), drift, []));

    // ── 底行：最近输出 · 刷新 ……… 从 X 卸载… ──
    const ops = document.createElement("div");
    ops.className = "machine-cc-foot";
    const log = button({ label: copyText("backend.buildCells.log"), size: "compact" });
    log.addEventListener("click", () => void this.toggleLog(origin, cells, log));
    // 事件驱动漏了一拍时的手动兜底：这台后端重跑起步那套对齐（会话 · 判死 · tmux · 身份标签 · 账号清单 · 监视）。
    const realign = button({ label: copyText("backend.buildCells.resync"), size: "compact" });
    realign.addEventListener("click", () => void this.realign(origin, realign));
    const sp = document.createElement("span");
    sp.className = "machine-head-sp";
    ops.append(log, realign, sp);
    if (extra.trailing) ops.appendChild(extra.trailing);
    // 上一次「停」的结局（`stopSaid`）；没停过就空着。
    const said = document.createElement("div");
    said.className = "settings-hint";
    said.dataset.stopSaid = "";
    col("ops", ops);
    cells.appendChild(said);

    this.cellHosts.set(origin, cells);
    return cells;
  }

  /**
   * 重画一行的「退出时会发生什么」＋ 那个勾：那一句是后端的成品 `said`。`answer` ＝ 后端答的那一份；`null` ＝ 问不到 ⇒ 勾禁用、那一行不说话。
   */
  private paintExit(origin: string, answer: ExitAnswer | null): void {
    const cells = this.cellHosts.get(origin);
    const el = cells?.querySelector<HTMLElement>(".backend-row-exit");
    const kill = this.killSwitches.get(origin);
    if (!el || !kill) return;
    if (answer === null) {
      kill.input.setAttribute("aria-disabled", "true");
      el.textContent = "";
      el.dataset.exit = "unasked";
      return;
    }
    el.dataset.exit = answer.policy;
    kill.input.removeAttribute("aria-disabled");
    kill.set(answer.killOnExit);
    sayWithDetail(el, answer.said, answer.detail);
  }

  /**
   * 重画一行的「上次崩没崩」：排版后端给的成品（[`decodeHealthFace`]）。长的那一半不进格子：
   * 无记录 ⇒ ⓘ 里说为什么这不等于没崩过；记到过事 ⇒［详情］展开。重画会跑很多遍（`settleStatus`），先摘上一次的附件再挂。
   * 收不下 ⇒ 只在这一格说「格式不对」，状态那一格照旧。
   */
  private paintHealth(origin: string, raw: unknown): void {
    const col = this.cellHosts.get(origin)?.querySelector<HTMLElement>('[data-col="health"]');
    const el = col?.querySelector<HTMLElement>(".backend-row-health");
    if (!col || !el) return;
    for (const extra of col.querySelectorAll("[data-health-extra]")) extra.remove();
    const face = decodeHealthFace(raw);
    if (face === null) {
      el.textContent = peerVersionSaid("reply_unreadable", origin);
      delete el.dataset.health;
      console.warn(`[PB1] ${origin} 的健康读数形状不对：${JSON.stringify(raw)}`);
      return;
    }
    el.textContent = face.summary;
    el.dataset.health = face.state;
    if (face.why !== null) {
      const why = makeInfoIcon(face.why);
      why.dataset.healthExtra = "why";
      col.appendChild(why);
    }
    if (face.detail !== null) {
      const more = document.createElement("details");
      more.dataset.healthExtra = "detail";
      const sum = document.createElement("summary");
      sum.textContent = copyText("backend.health.detail");
      const body = document.createElement("div");
      body.className = "settings-hint";
      body.textContent = face.detail;
      more.append(sum, body);
      col.appendChild(more);
    }
  }

  /** 点「日志」：没开 ⇒ 取回来摆在四格后面（不进四格本身，栏数不变）；开着 ⇒ 收起。 */
  private async toggleLog(origin: string, cells: HTMLElement, btn: HTMLButtonElement): Promise<void> {
    const open = cells.nextElementSibling as HTMLElement | null;
    if (open?.dataset.backendLog !== undefined) {
      open.remove();
      return;
    }
    const box = document.createElement("div");
    box.dataset.backendLog = origin;
    const head = document.createElement("div");
    head.className = "settings-hint";
    const pre = document.createElement("pre");
    box.append(head, pre);
    btn.disabled = true;
    try {
      const { head: h, body } = backendLogLines(await askBackendLog(origin));
      head.textContent = h;
      pre.textContent = body;
    } catch (e) {
      sayFailure(head, copyText("backend.log.failed"), e);
    } finally {
      btn.disabled = false;
    }
    cells.after(box);
  }

  /**
   * 起 / 停 / 重启一台：两条命令都是发出去就返回 ⇒ 操作期间禁用本行按钮、轮询到状态落定（或超时）再放开；超时不是失败。
   * 停：那台上一次性的 `--resident-stop` 先 SIGTERM、宽限期内等它收尾、还在才强杀，等完才返回；结局在这一行说一句（`stopSaid`）。
   */
  private async act(origin: string, what: "start" | "stop" | "restart"): Promise<void> {
    const cells = this.cellHosts.get(origin);
    const btns = cells ? [...cells.querySelectorAll<HTMLButtonElement>("[data-op]")] : [];
    for (const b of btns) b.disabled = true;
    // 停 / 重启之前：问一次会打断什么（经它的会话请求 · 开着看的会话 · 端口转发）；有才弹框。
    if (what !== "start") {
      const restart = what === "restart";
      const rows = interruptRows(await this.interrupts(origin), machineName(origin), restart ? "restart" : "stop");
      if (
        rows.length > 0 &&
        !(await this.confirm({
          title: restart
            ? copyText("backend.restart.title", { machine: machineName(origin) })
            : copyText("backend.stop.title", { machine: machineName(origin) }),
          action: restart ? copyText("backend.restart.action") : copyText("backend.stop.action"),
          danger: !restart,
          rows,
        }))
      ) {
        for (const b of btns) b.disabled = false;
        return;
      }
    }
    const said = cells?.querySelector<HTMLElement>("[data-stop-said]") ?? null;
    if (said) said.textContent = "";
    try {
      if (what !== "start") {
        const end = await commands.backend_stop({ origin });
        if (said && what === "stop") said.textContent = stopSaid(end);
        console.info(`[P2s] ${origin} stop: ${end.stopped} ${end.pid ?? ""}`);
      }
      if (what !== "stop") console.info(`[P2s] ${origin} start: ${await commands.backend_start({ origin })}`);
    } catch (e) {
      failToast(what === "stop" ? copyText("backend.stop.failed") : copyText("backend.start.failed"), e);
    }
    await this.settleStatus(origin, what !== "stop");
    for (const b of btns) b.disabled = false;
  }

  /** 轮询到「通道在不在」与期望一致，或超时。每次都重绘，用户看得到中间态。 */
  private async settleStatus(origin: string, want: boolean): Promise<void> {
    for (let i = 0; i < SETTLE_TRIES; i++) {
      if ((await this.paintStatus(origin)) === want) return;
      // 调度：自链 —— 起 / 停之后等状态落定：上限 30 次 × 100ms，落定即停
      await new Promise((r) => setTimeout(r, SETTLE_INTERVAL_MS));
    }
  }

  /** 拨开关：交那台的后端写，画的是它写完读回来的那一份；存不下 ⇒ 开关退回、说一句。 */
  private async toggleKill(origin: string, want: boolean): Promise<boolean> {
    try {
      const back = readExitAnswer(await putExitPolicy(origin, want));
      if (back === null) throw new Error(peerVersionSaid("reply_unreadable", origin));
      // 开关变了 ⇒ 那句「退出时会发生什么」也变了，同一拍重画。
      void this.paintStatus(origin);
      return true;
    } catch (e) {
      failToast(copyText("backend.policy.saveFailed"), e);
      return false;
    }
  }

  /** 那台记下的认不出的会话流：几种（两本账并起来数）；读不到 ⇒ 说读不到。 */
  private async paintDrift(origin: string): Promise<void> {
    const el = this.cellHosts.get(origin)?.querySelector<HTMLElement>(".backend-row-drift");
    if (!el) return;
    try {
      const records = await readRecordDrift(origin);
      const mine = await commands.drift_ledger_report({ origin });
      const n = records.reduce((k, f) => k + f.entries.length, 0) + mine.unknown_tokens.length;
      el.textContent = n > 0 ? copyText("backend.cc.driftSome", { n }) : copyText("backend.cc.driftNone");
    } catch {
      el.textContent = copyText("backend.cc.driftUnread");
    }
  }

  /** 画一次状态，并把「通道在不在」返回给 `settleStatus` 判落定。查不到回 `null`。 */
  private async paintStatus(origin: string): Promise<boolean | null> {
    const cells = this.cellHosts.get(origin);
    if (!cells) return null;
    const state = cells.querySelector<HTMLElement>(".backend-row-state");
    if (!state) return null;
    try {
      const st = await commands.backend_status({ origin });
      const on = st.channel === true;
      const word = on ? copyText("backend.status.connected") : copyText("backend.status.notConnected");
      const machine = decodeMachineState(st.machine);
      const ver = machine?.version ?? null;
      state.textContent = ver ? `${ver}${copyText("kit.text.sep")}${word}` : word;
      state.dataset.on = String(on);
      for (const b of cells.querySelectorAll<HTMLElement>("[data-op]")) b.style.display = (b.dataset.op === "start") === on ? "none" : "";
      if (on) void this.paintDrift(origin);
      this.onChannel?.(origin, on);
      this.onMachine?.(origin, machine);
      // 那个值问那台机器的后端要（**每次现问**，不用上一次的）；问不到就是 `null`。
      let answer: ExitAnswer | null;
      try {
        answer = readExitAnswer(await askExitPolicy(origin));
      } catch (e) {
        console.warn(`[B2] ${origin} 的退出策略问不到：${String(e)}`);
        answer = null;
      }
      this.paintExit(origin, answer);
      // 健康：同一份状态 JSON，不另问一次。
      this.paintHealth(origin, st.health);
      return on;
    } catch (e) {
      state.textContent = copyText("backend.status.unknown");
      this.onChannel?.(origin, null);
      console.warn(`[P2s] ${origin} 状态查询失败：${String(e)}`);
      return null;
    }
  }
}
