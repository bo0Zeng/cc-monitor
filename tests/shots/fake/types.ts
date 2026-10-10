/**
 * 截图工具的合成世界：几台机器、几个会话、各个面板要的成品。全是结构占位，不含任何真会话正文。
 * 形状挂在产品生成物类型上 ⇒ 后端改了线上形状，这里当场编不过。
 */
import type { LineRecord } from "../../../src/frontend/ui/generated/LineRecord";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";
import type { SessionContainer } from "../../../src/frontend/ui/generated/SessionContainer";
import type { SessionActivity } from "../../../src/frontend/ui/generated/SessionActivity";

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
  /** 哪一家（会话事实的 `agent`）；缺 ⇒ claude。 */
  agent?: string;
  cwd: string;
  name: string | null;
  /** 后台会话。 */
  background: boolean;
  /** 此刻在干什么（后端翻好的那一格）；`null` = 说不清。 */
  activity: SessionActivity | null;
  waitingFor: string | null;
  /** 在等你时从何时起等（epoch ms；`history-facts` 的 `needs.sinceMs`）。缺 ⇒ 不知道。 */
  waitingSinceMs?: number;
  /** 后台任务运行中时还在跑的后台命令（最早起的在前；`history-facts` 的 `background` 照后端 `facts_query::background_of` 拼）。缺 ⇒ 记录里一条都对不上。 */
  bgCommands?: { cmd: string; sinceMs: number }[];
  container: SessionContainer;
  records: LineRecord[];
  runs: RunInfo[];
  /** 子运行的记录（agent 面板点开那一条时按行取）：run → 记录。 */
  runRecords: Record<string, LineRecord[]>;
  tasks: TaskSpec[];
  /** 会话已结束（灰掉、归档）。 */
  ended: boolean;
  /** claude 退出、tmux 还在（灰灯）。 */
  idle: boolean;
  /** 那台的中转看见过这个会话的请求：带过扩展上下文那一项（`wide`）/ 没带过（`std`）；缺 ＝ 中转没看见过。 */
  relay?: "wide" | "std";
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
  /** 第一台是本机（`<local>`）。 */
  machines: string[];
  /** 订阅时看不见的那几台（流里第一格 `unseen`）。 */
  unseenMachines: string[];
  /** 配置文件那一页换哪种样子（`fake/profiles.ts`；不给 ⇒ 照设计稿那台的清单）。 */
  profiles?: "normal" | "broken" | "syntax" | "empty" | "migrated" | "edited" | "stale";
  /** 历史清单问不到的那几台（`history-list` 带它 ⇒ 答 `unreachable`）。 */
  historyDown?: string[];
  /** 会话流被那台后端关掉的那几台（流里第一格 `closed`）。 */
  closedMachines: string[];
  /** 那台的 cc-monitor 比这一版旧（状态成品 `needs_update`）。 */
  staleMachines: string[];
  /** 主机指纹与记下的不一样的那几台（状态成品 `host_key_changed`，带那台出示的那一枚）。 */
  hostKeyChanged?: string[];
  /** 正在把 cc-monitor 装上去的那几台（状态成品 `installing`）。 */
  installingMachines?: string[];
  /** 订阅时看得见、交完会话之后断了的那几台（随后一格 `unseen`，断在读那一跳）。 */
  droppedMachines?: string[];
  config: Record<string, unknown>;
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
  /** 重放时每个会话只交尾部这么多条（照真壳 `event_replay.rs::REPLAY_TAIL_KEEP`；更早的由骨架索引按偏移取）。不给 ⇒ 整份交。 */
  replayTail?: number;
  /** Tauri 命令的答法（`load_config` 之类），按命令名；场景可以整条覆盖。 */
  commands: Record<string, (args: Record<string, unknown>, world: World) => unknown>;
}

export interface ShotsHandle {
  state: "booting" | "done" | "failed";
  error: string | null;
  unhandled: string[];
  /** 截之前量 DOM 外接框发现的排版问题（`scenes/layout-check.ts`）。 */
  layout: string[];
  /** 场景要真鼠标停的那一点（页坐标）；截图工具停好之后调 `remeasure` 重量一遍排版。 */
  pointer?: { x: number; y: number };
  remeasure?: () => string[];
}

export interface SceneCtx {
  backend: import("./backend").FakeBackend;
}

declare global {
  interface Window {
    __shots?: ShotsHandle;
  }
}
