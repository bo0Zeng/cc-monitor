/**
 * 设置面板「backend 开关」区（P2s，定框 C8）。
 *
 * 每台机各一行：**本机 + 每台远端**。一行给三件事 —— 状态 · 起/停 · 「monitor 退出时结束它」。
 *
 * # 为什么独立一区，不塞进远端机器卡片
 *
 * 机器卡片那块是 **SSH 配置面**（地址/端口/密钥/指纹）。backend 开关是**运行期**的事，
 * 而且**本机也有一份** —— 本机没有 SSH 配置卡片可挂。
 * 挂在一起会逼出「本机那行长得和别人不一样」的特例，正是 `C1` 要避免的形状。
 *
 * # ⚠ 文案纪律（P2s-Y5 → K-P1 KPY4 翻面）：**按状态说实话**，不是「一律不许说」
 *
 * 翻面之前：backend 是纯 stdio 子进程，monitor 一退读端就断，它 **153 毫秒**内自己退出
 * ⇒ 那时「关掉开关 = 继续在后台跑」是一句做不到的承诺，判据是一张**禁词表**。
 *
 * K-P1 之后它在 Linux 上**真脱离**了 ⇒ 那一支上「继续跑」是真的，
 * 而**没脱离**的那一支上它仍然是假的。⇒ 判据从「禁这几个词」翻成
 * 「**必须出现「无人监护」这一档，且它只在真脱离那一支出现**」。
 *
 * ★★ **本文件不许有自己的那几句文案，也不判哪一档** —— 〔MIG-2 · `99 §2.1 ㊴`〕那一句由那台后端出成品
 * （`exit-policy-read` 的 `said`，它知道自己是回环常驻还是被监护），本文件原样摆（原先的 `describeExitBehavior`〔散文墓碑〕删了）。
 *
 * # 〔B2 · 条 66〕那个勾的值**问后端要、交后端写**
 *
 * 值住后端所在那台机器上（`设计/01 §3.3b`）。本文件画勾之前问一次（后端 `exit-policy-read`），
 * 改勾就发一条后端命令（`exit-policy-set`），**画的是后端写完读回来的那一份**。
 * 〔C4c · 第四波 4B〕两问**经通道直接问那台机器的后端**（[`askExitPolicy`] / [`putExitPolicy`]）：此前是两条 Tauri 命令
 * （`backend_exit_policy` / `set_backend_exit_policy`〔散文墓碑〕），monitor 那一跳只在转 ＋ 原样交回 —— 解释本来就在这里（`readExitAnswer`）。
 * 问不到（没连上 / 旧后端不认那条命令）⇒ 勾**禁用**、那一行不说话 —— 我们不知道那台机器上是什么，
 * 就不替它说一句；更不许拿一个本地缺省值画一个看起来能用的勾。
 * 理由是实测过的失效形态：现行那条判据 `readFileSync` 的**只有一个文件**、只剥整行注释
 * ⇒ 文案搬家 / 拼串 / 进一张 i18n 表就**零命中地绿**（与 `bind_guard` 头注自陈的
 * 「单独存在时是安慰剂」同族）。
 */

import { commands, type StopAnswer } from "../ipc/commands";
import { chan } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "../ipc/chan-caller";
import { isLocalOrigin, type Origin } from "../ipc/origin";
import { hostOs } from "./host-os";
import { noteLocalCcm } from "./machine-aliases"; // 〔FIX4 ⑥〕本机 ccm 那一格的唯一写点

/** 起/停之后轮询状态的次数与间隔 —— 命令是「发出去就返回」的，不轮询看到的是操作前的状态。 */
const SETTLE_TRIES = 30;
const SETTLE_INTERVAL_MS = 100;
import { showActionFailureToast } from "../error-toast";
import { resync, resyncSaid } from "../resync";
import { emit } from "@tauri-apps/api/event";
import { RESYNC_DONE_EVENT } from "./events";
import { makeInfoIcon } from "./info-icon";
import { LOCAL_ORIGIN } from "../backend-policy";
import { copyText } from "../copy-table";
import { formatBytes } from "../format";
import { askConfirm, type ConfirmFn } from "../ask-dialog";
import { fetchSessionAccountsOrNull } from "../account-reads";
import type { SessionAccount } from "../accounts";

/**
 * 〔HX1 · 主会话 D-f〕停后端之前**要不要先问一句**：走那台中转的活会话在后端停了之后每一次请求都会失败（中转住在后端进程里；〔TAIL〕远端同形）。
 * 回要问的那句话；`null` = 不用问（问到了、而且一条走中转的活会话都没有）。
 * - 问不到（`rows === null`）⇒ 照样问，说「不知道有几条」（出声，不把「问不到」当成「没有」）；
 * - 活着但说不清走不走中转的（`viaRelay` 缺 / `null`）⇒ 连同确定的几条一起说出来。
 */
/**
 * 〔STOP〕「停」的结局说一句（三个词各一句，穷举 —— 多一个词 tsc 就红）。机器页那一行照它说，不只进 console。
 */
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

export function stopWarning(rows: SessionAccount[] | null): string | null {
  if (rows === null) return copyText("backend.stop.relayUnknown");
  const live = rows.filter((r) => r.alive);
  const n = live.filter((r) => r.viaRelay === true).length;
  const k = live.filter((r) => r.viaRelay === null || r.viaRelay === undefined).length;
  if (k > 0) return copyText("backend.stop.relayMaybe", { n, k });
  if (n > 0) return copyText("backend.stop.relayConfirm", { n });
  return null;
}

/** 那个值现读出来的三态（与后端 `exit_policy::Read::state` 逐字对齐）。 */
type ExitPolicyState = "chosen" | "absent" | "unreadable";

/** 后端 `exit-policy-read` / `exit-policy-set` 回的那一份里，本区要用的三格（〔㊴〕`said` 是成品那一句）。 */
interface ExitAnswer {
  policy: ExitPolicyState;
  killOnExit: boolean;
  said: string;
}

/**
 * 〔B2〕从后端那份不透明 JSON 里取「退出行为」两格。**缺一格 / 形状不对 ⇒ `null`**（= 问不到）。
 *
 * ⚠ **答不出来就说答不出来**，不替后端补一个缺省值 ——
 * 补了就是在一台我们不知道的机器上画一个看起来能用的勾。
 */
function readExitAnswer(raw: unknown): ExitAnswer | null {
  if (typeof raw !== "object" || raw === null) return null;
  const v = raw as Record<string, unknown>;
  const policy =
    v.state === "chosen" || v.state === "absent" || v.state === "unreadable" ? v.state : null;
  if (policy === null || typeof v.killOnExit !== "boolean") return null;
  // 〔㊴〕成品那一句缺了 / 空了 ⇒ 当问不到（不替后端补一句）。
  if (typeof v.said !== "string" || v.said === "") return null;
  return { policy, killOnExit: v.killOnExit, said: v.said };
}

/** 「退出行为」那两问的期限：10 秒 —— 与它们上一个住址（monitor `backend_policy::EXIT_POLICY_BUDGET`）同值。 */
const EXIT_POLICY_BUDGET_MS = 10_000;

/** 那台后端比「退出行为」搬过去还老（不认这两条命令）时的那句话。 */
// 〔CP2b〕做成函数、用到时才取文（模块顶层不留取文口调用 —— 顶层调用会让 Rollup 把设置面板挪进主窗共享 chunk）。
const EXIT_POLICY_OLD_BACKEND = (): string =>
  copyText("backend.exitPolicy.oldBackend");

/**
 * 〔C4c · 第四波 4B〕问那台机器（本机也一样）「退出行为」那个值：后端 `exit-policy-read`，经通道。
 * 回后端那份原样（交给 [`readExitAnswer`] 收）。**失败就抛**（一句人话，通道那一层的说法住 `chan-caller.ts::saidOf`）。
 */
async function askExitPolicy(origin: Origin): Promise<unknown> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(EXIT_POLICY_BUDGET_MS);
    return readJson(await chan.call(origin, "exit-policy-read", body, budget));
  } catch (e) {
    throw new Error(saidOf(e, EXIT_POLICY_OLD_BACKEND()));
  }
}

/** 〔C4c〕交那台机器写那个值：后端 `exit-policy-set`，经通道；回**写完读回来**的那一份。失败就抛。 */
async function putExitPolicy(origin: Origin, kill: boolean): Promise<unknown> {
  try {
    const body = jsonBody({ killOnExit: kill });
    const budget = budgetWithin(EXIT_POLICY_BUDGET_MS);
    return readJson(await chan.call(origin, "exit-policy-set", body, budget));
  } catch (e) {
    throw new Error(saidOf(e, EXIT_POLICY_OLD_BACKEND()));
  }
}

/** 〔GAP1 · `设计/15 §4.7 S1`〕那台后端的 stderr 诊断文件尾部（后端 `backend-log` 的应答）。 */
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
    throw new Error(saidOf(e, copyText("backend.log.oldBackend")));
  }
  const log = readBackendLog(raw);
  if (!log) throw new Error(copyText("backend.log.badShape"));
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
 * 〔PB1 · `设计/90 §4` 阶段 B〕「健康」那一格的**成品** —— 后端 `backend_policy.rs::health_face` 出，经 `backend_status` 的 `health`。
 * 本文件只排版：`summary` 进格子 · `state` 原样挂 `data-health`（`70 §2.2`「区分用界面状态表达」；取值集不在这里抄）·
 * `why` 在就摆 ⓘ · `detail` 在就摆 `[详情]`。哪一档、每一档给什么，**一处都不在前端判**。
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
 * 按形状**严格收**（`设计/05 §14.3`「多一格 / 缺一格 / 类型不对 ⇒ 抛……不猜」）：键集恰好那四个 ·
 * `state` / `summary` 非空串 · `why` / `detail` 非空串或 `null`。收不下 ⇒ `null`，那一格说「格式不对」。
 *
 * ⚠ 收不下**不回落成「— 无记录」**：原来那份读数解析缺格就画无记录，那是一条前端的回落判定；
 *   `backend_status` 是 monitor 自己的命令、与界面同一个构建，缺格只能是程序错 ⇒ 说出来（D7 / D11），不替后端编一档。
 */
export function decodeHealthFace(raw: unknown): HealthFace | null {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return null;
  const v = raw as Record<string, unknown>;
  const keys = Object.keys(v).sort();
  if (keys.length !== HEALTH_FACE_KEYS.length || keys.some((k, i) => k !== HEALTH_FACE_KEYS[i])) return null;
  const text = (x: unknown): x is string => typeof x === "string" && x !== "";
  const textOrNull = (x: unknown): x is string | null => x === null || text(x);
  if (!text(v.state) || !text(v.summary) || !textOrNull(v.why) || !textOrNull(v.detail)) return null;
  return { state: v.state, summary: v.summary, why: v.why, detail: v.detail };
}

/**
 * 🔴 第二刀 步 6（`设计/70 §2.3`）：一台机那一行的**四栏**。栏名只在这里写一次，
 * 表头与每一格的 `data-col` 都从这张表来。
 */
// 〔CP2b〕做成函数、用到时才取文（顶层不留取文口调用）。
export const BACKEND_COLUMNS = () =>
  [
    ["state", copyText("backend.column.status")],
    ["ops", copyText("backend.column.actions")],
    ["exit", copyText("backend.column.exit")],
    ["health", copyText("backend.column.health")],
  ] as const;
export type BackendColumn = ReturnType<typeof BACKEND_COLUMNS>[number][0];

/** 〔步 14〕列表里有、后端清单里没有的那一台 —— 「未登记」那一格的 ⓘ。 */
export const BACKEND_UNREGISTERED_WHY = (): string =>
  copyText("backend.unregistered.why");

/** 一台机在这一区里的身份。`origin` 是唯一键，`title` 只给人看。 */
interface Machine {
  origin: string;
  title: string;
}

export class BackendSection {
  readonly element: HTMLElement;
  private rows = new Map<string, HTMLElement>();
  /**
   * origin → 那一台的**四格**容器。重画（状态 / 退出行为 / 健康）与起停都只认这里，
   * 不认它挂在谁底下 ——〔步 14〕这四格会挂进机器列表那一行上。
   */
  private cellHosts = new Map<string, HTMLElement>();

  /**
   * 〔步 14〕寄居模式：四格挂在**机器列表那一行**上（`cellsFor`），本块自己的 `element`
   * 只装「后端清单里有、机器列表里没有」的那几台（见 `refresh` 的头注）。
   */
  private readonly hosted: boolean;
  /** 后端清单（`backend_machines`）。`null` = 还没问到 —— 那时寄居的四格先不画。 */
  private registered: Set<string> | null = null;

  /** 〔HX1 · D-f〕停之前那一问 · 数会话那一问（注入缝：判据换成同步答 / 假数据）。 */
  private readonly confirm: ConfirmFn;
  private readonly sessions: (origin: Origin) => Promise<SessionAccount[] | null>;

  constructor(
    opts: {
      headless?: boolean;
      hosted?: boolean;
      confirm?: ConfirmFn;
      sessions?: (origin: Origin) => Promise<SessionAccount[] | null>;
    } = {},
  ) {
    this.hosted = opts.hosted ?? false;
    this.confirm = opts.confirm ?? askConfirm;
    this.sessions = opts.sessions ?? fetchSessionAccountsOrNull;
    this.element = document.createElement("div");
    this.element.className = "settings-section backend-section";
    if (!opts.headless) {
      const h = document.createElement("h3");
      // 〔ST2〕原来是「backend 开关」—— `backend` 是术语表禁词（对外叫「后端」，`terms.json`）。
      h.textContent = copyText("backend.ctor.title");
      this.element.appendChild(h);
    }
    if (!this.hosted) {
      const hint = document.createElement("p");
      hint.className = "settings-hint";
      // ★ 这一句**刻意不再承诺任何一种退出行为** —— 那句话今天是**按机器分档**的
      //   （同一台机上勾没勾、脱没脱离，四种组合各说各的），所以它住在每一行里，
      //   由那台后端出成品（`exit-policy-read` 的 `said`）。
      //   ⚠ 原来这里那句「它仍会在 monitor 退出后很快自行退出」是**实测结论**，
      //   而 K-P1 之后它只对**没脱离**的那一支成立 —— 留在这里就成了一句半假的全称。
      hint.textContent = copyText("backend.ctor.intro");
      this.element.appendChild(hint);
      // 🔴 第二刀 步 6（`设计/70 §2.3`）：**表格式四栏** —— 状态 / 操作 / 退出行为 / 健康。
      //   原来是一行跑句（「本机未连上 [起][停] ☐ monitor 退出时结束它」），控件嵌在散文里。
      this.element.appendChild(BackendSection.columnHead());
    }
    this.list = document.createElement("div");
    this.list.className = "backend-list";
    this.element.appendChild(this.list);
    void this.refresh();
  }

  private list: HTMLElement;

  /**
   * 〔步 14 · `设计/70 §5.3`〕**机器列表那一行上的四格**：DAEMON 开关并进列表行，
   * 不再单独占一块（「列表页 = 列表 ＋ 添加 ＋ 全局开关 ＋ 诊断」四样）。
   *
   * 每次调都建一份新的（列表每重建一次就来要一次），旧的那份随旧行一起被摘掉。
   * 后端清单里这台若正以「列表里没有」的身份挂在本块自己的列表里，那一行当场收掉 —— 一台机只许一份四格。
   */
  cellsFor(origin: string): HTMLElement {
    this.rows.get(origin)?.remove();
    this.rows.delete(origin);
    const cells = this.buildCells(origin);
    this.paintOne(origin);
    return cells;
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
    for (const col of ["ops", "exit", "health"] as const) {
      cells.querySelector<HTMLElement>(`[data-col="${col}"]`)?.replaceChildren();
    }
  }

  /**
   * 重新拉一遍机器清单 + 每台的状态。
   *
   * 〔步 14〕寄居模式下：清单里的每一台，机器列表那一行已经来要过四格（`cellsFor`）⇒ 只重画；
   * **没来要过**的（后端清单与机器列表对不上的那几台：重名被后缀化、远端模式关着时列表照样列全部）
   * ⇒ 在本块自己的列表里另起一行，不丢。列表那一行晚到时由 `cellsFor` 把这一行收掉。
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
   * 机器清单**问后端要**，不自己算〔D 阶段补审 08-11，A5〕。
   *
   * 原版自己拼（那个 helper 叫 hostKey，**这里刻意不带括号写** —— 下面那条判据扫的就是
   * 「带括号的调用形态」，写全会把这句解释算成一次调用；本轮已被自己的散文绊到四次）：
   * 它是 `h.label.trim() || h.host`，与 Rust 的 origin 分叉四处：
   * ① 前端 `trim()` 而 `origin_label()` 不 trim；② Rust 对**重复 label 做后缀化**
   * （`"pi" → "pi (#2)"`）并按后缀化后的名字注册 ⇒ 第二台起/停恒回「没有这台机的把手」；
   * ③ 前端忽略 `cfg.enabled`，远端总开关关着时一个都没注册而 UI 照样列全部；
   * ④ `register_remote` 只在启动时跑一次，之后新增的机器永远不在注册表里。
   *
   * ⇒ 注册表就是真相源。本机永远在第一行 —— 它不是「另一种机器」，
   * 只是不走 ssh 的那一台（§40 / C1）。
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
   * 一台机那一行：名字 ＋ **四格**（`BACKEND_COLUMNS`）。
   *
   * ⚠ 每一格是一个带 `data-col` 的容器，里面的元素**沿用原来的类名**
   *   （`.backend-row-state` / `.backend-row-kill` / `.backend-row-exit` / `.backend-row-health`）——
   *   判据与既有测试按类名找它们；分栏这一维走 `data-*`，不新造 CSS 类（`css-ledger` ③ 是棘轮）。
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

  /**
   * 四格本体（不含名字）。〔步 14〕机器列表那一行也挂这一份 —— 同一套格子、同一套重画，
   * 只是宿主不同（见 `cellsFor`）。
   */
  /** 〔RESYNC〕「重新对齐」：问那台后端 `resync`（整机），结果说一句。 */
  private async realign(origin: string, btn: HTMLButtonElement): Promise<void> {
    btn.disabled = true;
    try {
      const r = await resync(origin);
      // 〔FIX4 · 主会话裁⑥ · V149 手动兜底〕本机那一行对齐 ⇒ 顺手作废「PATH 上的 ccm」那份 5 分钟缓存、重记那一格（Windows 不适用）。
      if (isLocalOrigin(origin) && hostOs() !== "windows") {
        noteLocalCcm(true).catch((e: unknown) => console.warn("[resync] 本机 ccm 那一格没重问：", e));
      }
      showActionFailureToast(copyText("backend.resync.doneTitle"), resyncSaid(r), { level: "info", durationMs: 6000 });
      void emit(RESYNC_DONE_EVENT, { origin }); // ㉟①：主窗口标出这台上记录没了的固定条
    } catch (e) {
      showActionFailureToast(copyText("backend.resync.failed"), e instanceof Error ? e.message : String(e));
    } finally {
      btn.disabled = false;
    }
  }

  private buildCells(origin: string): HTMLElement {
    const cells = document.createElement("span");
    cells.dataset.backendCells = origin;
    const col = (name: BackendColumn): HTMLElement => {
      const c = document.createElement("span");
      c.dataset.col = name;
      cells.appendChild(c);
      return c;
    };

    const stateCol = col("state");
    const state = document.createElement("span");
    state.className = "backend-row-state";
    state.textContent = copyText("backend.buildCells.querying");
    stateCol.appendChild(state);

    const ops = col("ops");
    const start = document.createElement("button");
    start.textContent = copyText("backend.buildCells.start");
    start.onclick = () => void this.act(origin, "start");
    ops.appendChild(start);
    const stop = document.createElement("button");
    stop.textContent = copyText("backend.buildCells.stop");
    stop.onclick = () => void this.act(origin, "stop");
    ops.appendChild(stop);
    // 〔GAP1 · `设计/15 §4.7 S1`〕这台后端的诊断文件（本机远端同一问，经那台后端的只读面）。
    const log = document.createElement("button");
    log.textContent = copyText("backend.buildCells.log");
    log.onclick = () => void this.toggleLog(origin, cells, log);
    ops.appendChild(log);
    // 〔RESYNC · V149 · `设计/15 §4.1b`〕事件驱动漏了一拍时的手动兜底：这台后端重跑起步那套对齐（会话 · 判死 · tmux · 身份标签 · 账号清单 · 监视）。
    const realign = document.createElement("button");
    realign.textContent = copyText("backend.buildCells.resync");
    realign.onclick = () => void this.realign(origin, realign);
    ops.appendChild(realign);
    // 〔STOP〕上一次「停」的结局（`stopSaid`）；没停过就空着。
    const said = document.createElement("span");
    said.className = "settings-hint";
    said.dataset.stopSaid = "";
    ops.appendChild(said);

    const exitCol = col("exit");
    const label = document.createElement("label");
    label.className = "backend-row-kill";
    const box = document.createElement("input");
    box.type = "checkbox";
    // 〔B2〕问到那台机器的值之前，勾**禁用**：它的值不在这里，在那台机器上。
    box.checked = false;
    box.disabled = true;
    box.onchange = () => void this.toggleKill(origin, box);
    label.appendChild(box);
    label.appendChild(document.createTextNode(copyText("backend.buildCells.exitKills")));
    exitCol.appendChild(label);
    // ★★ `K-P1 KPY4`：**这台机退出时到底会发生什么**，按状态分档如实说。
    // 文案本体不在本文件（见头注）；这里只放它的位置。
    const exit = document.createElement("div");
    exit.className = "backend-row-exit";
    exitCol.appendChild(exit);

    // ★★ `K-P3b KP3W4`：**读数**，另起一格（〔PB1〕成品由后端出，见 `paintHealth`）。
    // ⚠ **不许接在退出那一句后面**：退出那一句是后端的成品 `said`，`backend-section.vitest.ts` 用**等号**钉着 —— 那两句话说的是两件事。
    const healthCol = col("health");
    const health = document.createElement("span");
    health.className = "backend-row-health";
    healthCol.appendChild(health);

    this.cellHosts.set(origin, cells);
    return cells;
  }

  /**
   * 重画一行的「退出时会发生什么」＋ 那个勾。
   *
   * 〔MIG-2 · ㊴〕那一句是后端的成品 `said`（它知道自己是回环常驻还是被监护）；原先这里拿 monitor 的 `backend_status.detached`
   * 拼（远端恒 `null` ⇒ 远端常驻也被说成「很快自行退出」），那一格不再参与。
   *
   * 〔B2〕`answer` 是**后端答的**那一份；`null` = 问不到 ⇒ 勾禁用、那一行不说话。
   * 〔S5 · V105 清账〕原来还有「不适用」一臂（折进前端那一档：勾、那一行、[起][停] 整个拿掉，E4）——
   * 那一档已放弃，这一臂随之删了。
   */
  private paintExit(origin: string, answer: ExitAnswer | null): void {
    const cells = this.cellHosts.get(origin);
    const el = cells?.querySelector<HTMLElement>(".backend-row-exit");
    const label = cells?.querySelector<HTMLElement>(".backend-row-kill");
    const box = label?.querySelector<HTMLInputElement>("input");
    if (!el || !label || !box) return;
    if (answer === null) {
      box.disabled = true;
      el.textContent = "";
      el.dataset.exit = "unasked";
      return;
    }
    el.dataset.exit = answer.policy;
    box.disabled = false;
    box.checked = answer.killOnExit;
    el.textContent = answer.said;
  }

  /**
   * 重画一行的「上次崩没崩」：**排版后端给的成品**（[`decodeHealthFace`]）。
   *
   * 🔴 第二刀 步 6（`70 §2.3`）：长的那一半**不进格子** —— 无记录 ⇒ ⓘ 里放那条「为什么这不等于没崩过」
   *   （`§2.2`：区分保留，只换位置）；记到过事 ⇒ `[详情]` 展开四个计数与完整记录去哪看。
   *   〔PB1〕哪一档、带不带 ⓘ / `[详情]`、每一句说什么，都是后端定的；这里只看 `why` / `detail` 在不在。
   *   重画会跑很多遍（`settleStatus` 轮询），所以先把上一次的附件摘掉再挂。
   * 收不下（形状不对）⇒ 只在这一格说「格式不对」，状态那一格照旧（`70 §1.2` D：失败落在那一块上）。
   */
  private paintHealth(origin: string, raw: unknown): void {
    const col = this.cellHosts.get(origin)?.querySelector<HTMLElement>('[data-col="health"]');
    const el = col?.querySelector<HTMLElement>(".backend-row-health");
    if (!col || !el) return;
    for (const extra of col.querySelectorAll("[data-health-extra]")) extra.remove();
    const face = decodeHealthFace(raw);
    if (face === null) {
      el.textContent = copyText("backend.health.badShape");
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

  /**
   * 起 / 停一台机〔D 阶段补审 08-11 重写，A4〕。
   *
   * # 原来错在哪
   *
   * 命令返回后**立刻**重绘 —— 而两个命令都是「发出去就返回」：
   * `backend_start` 只是 spawn 了监护线程（子进程还没起、hello 更没到）⇒ 屏上写「未连上」；
   * `backend_stop` 只发 SIGKILL 就返回（消费者要等 EOF 才 `unregister`）
   * ⇒ `channel` 多半仍是 true 而 `pid` 因句柄已被 take 而是 null ⇒ 屏上写「**已连上**（无 pid）」。
   *
   * **即每次操作后看到的都是操作前的状态。** 而且按钮全程不 disable，
   * 直接喂给「双起」那个窗口（补审 A1）。
   *
   * ⇒ 现在：操作期间**禁用本行按钮**，然后**轮询到状态落定**（或超时）再放开。
   * ⚠ 超时不是失败：远端断流后对面进程什么时候退，我们在本机看不见（诚实边界 11c）。
   *
   * 〔HX1 · 4D · D-a〕上面「只发 SIGKILL 就返回」是历史。〔STOP〕今天本机远端同一条：那台机器上的一次性 `--resident-stop`
   * 先 SIGTERM、在宽限期（35 秒，比后端自己 30 秒的排空上限长）内等它自己收尾、还在才强杀，**等完才返回** ⇒ 按钮会禁用那么久；
   * 结局（`graceful` / `killed` / `not_running`）在这一行说一句（`stopSaid`）；没停掉 ⇒ 命令回 `Err`，走下面那条失败提示。
   */
  /** 〔GAP1〕点「日志」：没开 ⇒ 取回来摆在四格后面（不进四格本身，栏数不变）；开着 ⇒ 收起。 */
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
      head.textContent = copyText("backend.log.failed", { e: e instanceof Error ? e.message : String(e) });
    } finally {
      btn.disabled = false;
    }
    cells.after(box);
  }

  private async act(origin: string, what: "start" | "stop"): Promise<void> {
    const cells = this.cellHosts.get(origin);
    const btns = cells ? [...cells.querySelectorAll("button")] : [];
    for (const b of btns) b.disabled = true;
    // 〔HX1 · D-f〕停后端之前：有走那台中转的活会话 ⇒ 先问一句、说几条会断。
    // 〔TAIL · HOST 余项〕远端也问：V139 之后远端中转住在那台的常驻后端里，停它就停了中转。
    if (what === "stop") {
      const warn = stopWarning(await this.sessions(origin));
      if (warn !== null && !(await this.confirm(warn))) {
        for (const b of btns) b.disabled = false;
        return;
      }
    }
    const said = cells?.querySelector<HTMLElement>("[data-stop-said]") ?? null;
    if (said) said.textContent = "";
    try {
      if (what === "start") {
        console.info(`[P2s] ${origin} start: ${await commands.backend_start({ origin })}`);
      } else {
        const end = await commands.backend_stop({ origin });
        if (said) said.textContent = stopSaid(end);
        console.info(`[P2s] ${origin} stop: ${end.stopped} ${end.pid ?? ""}`);
      }
    } catch (e) {
      showActionFailureToast(what === "start" ? copyText("backend.start.failed") : copyText("backend.stop.failed"), String(e));
    }
    await this.settleStatus(origin, what === "start");
    for (const b of btns) b.disabled = false;
  }

  /** 轮询到「通道在不在」与期望一致，或超时。每次都重绘，用户看得到中间态。 */
  private async settleStatus(origin: string, want: boolean): Promise<void> {
    for (let i = 0; i < SETTLE_TRIES; i++) {
      if ((await this.paintStatus(origin)) === want) return;
      await new Promise((r) => setTimeout(r, SETTLE_INTERVAL_MS));
    }
  }

  private async toggleKill(origin: string, box: HTMLInputElement): Promise<void> {
    const want = box.checked;
    try {
      // 〔B2〕交后端写（那个值住那台机器上），**画的是它写完读回来的那一份**。
      const back = readExitAnswer(await putExitPolicy(origin, want));
      if (back === null) throw new Error(copyText("backend.policy.badShape"));
      // 勾变了 ⇒ 那句「退出时会发生什么」也变了。**同一拍重画**，
      // 否则屏上那句话描述的是上一次的状态（与 A4 那条「画的是操作前的快照」同族）。
      void this.paintStatus(origin);
    } catch (e) {
      // 存不下就**把勾回退**——否则屏上写着 A 而实际是 B，比报错更坏。
      box.checked = !want;
      showActionFailureToast(copyText("backend.policy.saveFailed"), e instanceof Error ? e.message : String(e));
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
      const pid = typeof st.pid === "number" ? `（pid ${st.pid}）` : "";
      state.textContent = on ? copyText("backend.status.connected", { pid }) : copyText("backend.status.notConnected");
      state.dataset.on = String(on);
      // 〔B2〕那个值问那台机器的后端要（**每次现问**，不用上一次的）；问不到就是 `null`。
      let answer: ExitAnswer | null;
      try {
        answer = readExitAnswer(await askExitPolicy(origin));
      } catch (e) {
        console.warn(`[B2] ${origin} 的退出策略问不到：${String(e)}`);
        answer = null;
      }
      this.paintExit(origin, answer);
      // K-P3b：**同一份 JSON**，另一个元素。不新开一次查询，也不接在上面那一行后面。
      this.paintHealth(origin, st.health);
      return on;
    } catch (e) {
      state.textContent = copyText("backend.status.unknown");
      console.warn(`[P2s] ${origin} 状态查询失败：${String(e)}`);
      return null;
    }
  }
}
