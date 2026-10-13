/**
 * 截图工具的合成世界：几台机器、几个会话、各个面板要的成品。全是结构占位，不含任何真会话正文。
 * 形状挂在产品生成物类型上 ⇒ 后端改了线上形状，这里当场编不过。
 */
import type { SessionActivity } from "../../../src/frontend/ui/generated/SessionActivity";
import type { Prefs } from "../../../src/frontend/ui/generated/Prefs";
import type { Convo } from "./records";

export interface TaskSpec {
  id: string;
  subject: string;
  status: "pending" | "in_progress" | "completed";
  blocks: string[];
  blockedBy: string[];
  description?: string;
  activeForm?: string;
}

export interface SessionSpec {
  sid: string;
  origin: string;
  cwd: string;
  /** 会话名（pidfile 的 `name`）。 */
  name: string | null;
  /** 后台会话（pidfile 的 `kind: "bg"`）。 */
  background: boolean;
  /** 此刻在干什么（写进 pidfile 的 `status`，后端照它翻）；`null` = 没写（说不清）。 */
  activity: SessionActivity | null;
  /** 在等什么框（pidfile 的 `waitingFor`）。 */
  waitingFor: string | null;
  /** 在等你时从何时起等（epoch ms；pidfile 的 `statusUpdatedAt`）。缺 ⇒ 不知道。 */
  waitingSinceMs?: number;
  /** 不住 tmux（直接在终端里起的）：替身是个光进程。缺 ⇒ 住在那台私有 tmux 的一个窗格里。 */
  outsideTmux?: boolean;
  /** 会话记录（盘上的原始 jsonl ＋ 派出的子 agent 那几份）；`null` ＝ 还没写过一行。 */
  convo: Convo | null;
  tasks: TaskSpec[];
  /** 会话已结束：开页时还活着，页里收到那台的清单之后替身收场（灰掉、归档）。 */
  ended: boolean;
  /** claude 退出、tmux 还在（灰灯）：只有挂着标签的那个窗格，没有 pidfile。 */
  idle: boolean;
}

/** 一条帧命令怎么答：给值 ⇒ 当 JSON 答；抛 `Refuse` ⇒ 对端说不行；不在表里 ⇒ 对端说不认。 */
export type OpHandler = (origin: string, req: Record<string, unknown>, world: World) => unknown;

export class Refuse {
  constructor(
    readonly code: string,
    readonly message: string,
    /** 按码定形的那几个码带的 `data`（例：`account_unavailable`）。 */
    readonly data?: unknown,
    /** 下层原话（进复制详情的「原话」那一项；`message` 是那台写好的那一句）。 */
    readonly raw?: string,
  ) {}
}

export interface World {
  /** 每台机器家目录里的原始文件（真后端读它们答 `REAL_OPS` 那几条；`../disk`）。缺 ⇒ 空家目录。 */
  disk: Record<string, import("../disk").MachineDisk>;
  /** 第一台是本机（`<local>`）。 */
  machines: string[];
  /** 订阅时看不见的那几台（流里第一格 `unseen`）。 */
  unseenMachines: string[];
  /** 到那台那一跳断开的几条帧命令：机器 → op（例：历史清单问不到那台，`history-list`）。 */
  hopDown?: Record<string, string[]>;
  /** 那台后端比界面老、不认的几条帧命令：机器 → op。 */
  older?: Record<string, string[]>;
  /** 会话流被那台后端关掉的那几台（流里第一格 `closed`）。 */
  closedMachines: string[];
  /** 那台的 cc-monitor 比这一版旧（状态成品 `needs_update`）。 */
  staleMachines: string[];
  /** 主机指纹与记下的不一样的那几台（状态成品 `host_key_changed`，带那台出示的那一枚）。 */
  hostKeyChanged?: string[];
  /** 正在把 cc-monitor 装上去的那几台（状态成品 `installing`）。 */
  installingMachines?: string[];
  /** 订阅时看得见、交完会话之后断了的那几台（交完清单一会儿之后那台后端被杀：壳那一侧照真的那一套作废它的成品、通道说断了）。 */
  droppedMachines?: string[];
  config: Record<string, unknown>;
  /** 那台后端 `prefs.json` 里的偏好（启动命令 · 恢复到 tmux 里 · 每号模型 · 上下文上限 · 数据目录）：机器 → 那一份。没有那一台 ⇒ 那台没设过（`prefs-read` 答「没有这份」）。 */
  prefs?: Record<string, Prefs>;
  sessions: SessionSpec[];
  /** 帧命令的答法，按 op 名；场景可以整条覆盖。 */
  ops: Record<string, OpHandler>;
  /** 终端实时画面那一路怎么演（不给 ⇒ `live`：订上、推一屏）：推一屏之后画面中断 · 推一屏之后那台断开。只能快照的那一种由场景把 `terminal-follow` 换成拒。 */
  terminalLive?: "live" | "lost" | "offline";
  /** 场景故意不答的那几条（演「后端比界面老」）：答不上照样按「不认」拒，只是不报。 */
  quiet?: string[];
  /** 帧命令晚多少毫秒才答（按 op 名；不给 ⇒ 当场答）。性能台架用它演「后端读大文件 / 远端那一跳要时间」：快速连切时，切走之后才回来的那几问还干不干活。 */
  opDelayMs?: Record<string, number>;
  /** 不带声明的那一问（展开那一下取回全文）的延时（op ⇒ 毫秒）；缺 ⇒ 照 `opDelayMs`。 */
  fullFetchDelayMs?: Record<string, number>;
  /** Tauri 命令的答法（`load_config` 之类），按命令名；场景可以整条覆盖。 */
  commands: Record<string, (args: Record<string, unknown>, world: World) => unknown>;
}

export interface ShotsHandle {
  state: "booting" | "done" | "failed";
  /** `state` 离开 `booting` 的那一刻兑现（截图工具按它等，不按钟点问）。场景里每一处等都有兜底上限、到了就把在等什么写进 `error`。 */
  finished: Promise<void>;
  error: string | null;
  unhandled: string[];
  /** 截之前量 DOM 外接框发现的排版问题（`scenes/layout-check.ts`）。 */
  layout: string[];
  /** 场景要真鼠标停的那一点（页坐标）；截图工具停好之后调 `remeasure` 重量一遍排版。 */
  pointer?: { x: number; y: number };
  remeasure?: () => string[];
  /** 场景要真鼠标拖的那一下（页坐标：从 `(x0, y0)` 按下、拖到 `(x1, y1)` 松开（竖着的列表 x1 ＝ x0））；松开之后截图工具调 `afterDrag`，回一句话 ⇒ 进问题表。 */
  drag?: { x0: number; y0: number; x1: number; y1: number };
  afterDrag?: () => Promise<string | null>;
  /** 会话到了几个：世界宣告的 · 假后端已经交出 `live` 那一格的。等 tab 等不到时拿它分「后端没交」与「交了界面没收」。 */
  sessions?: () => { declared: number; sent: number };
}

export interface SceneCtx {
  backend: import("./backend").FakeBackend;
}

declare global {
  interface Window {
    __shots?: ShotsHandle;
  }
}
