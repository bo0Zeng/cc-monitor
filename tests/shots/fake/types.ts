/**
 * 截图工具的合成世界：几台机器、几个会话、各个面板要的成品。全是结构占位，不含任何真会话正文。
 * 形状挂在产品生成物类型上 ⇒ 后端改了线上形状，这里当场编不过。
 */
import type { JsonlRecord } from "../../../src/frontend/ui/generated/JsonlRecord";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";

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
  name: string | null;
  /** `null` / `"interactive"` = 交互；`"bg"` = 后台。 */
  kind: string | null;
  /** Claude 的 status 原值：busy / idle / waiting / shell；`null` = 不说。 */
  status: string | null;
  waitingFor: string | null;
  container: "tmux" | "none";
  records: JsonlRecord[];
  runs: RunInfo[];
  /** 子运行的记录（agent 面板点开那一条时按行取）：run → 记录。 */
  runRecords: Record<string, JsonlRecord[]>;
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
  ) {}
}

export interface World {
  /** 第一台是本机（`<local>`）。 */
  machines: string[];
  /** 订阅时看不见的那几台（流里第一格 `unseen`）。 */
  unseenMachines: string[];
  /** 会话流被那台后端关掉的那几台（流里第一格 `closed`）。 */
  closedMachines: string[];
  config: Record<string, unknown>;
  sessions: SessionSpec[];
  /** 帧命令的答法，按 op 名；场景可以整条覆盖。 */
  ops: Record<string, OpHandler>;
  /** 场景故意不答的那几条（演「后端比界面老」）：答不上照样按「不认」拒，只是不报。 */
  quiet?: string[];
  /** Tauri 命令的答法（`load_config` 之类），按命令名；场景可以整条覆盖。 */
  commands: Record<string, (args: Record<string, unknown>, world: World) => unknown>;
}

export interface ShotsHandle {
  state: "booting" | "done" | "failed";
  error: string | null;
  unhandled: string[];
}

export interface SceneCtx {
  backend: import("./backend").FakeBackend;
}

declare global {
  interface Window {
    __shots?: ShotsHandle;
  }
}
