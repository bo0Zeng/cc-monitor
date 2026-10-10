/**
 * 〔拆 `tabs.ts` ①〕**一个实时 tab 长什么样** ＋ 它的标题怎么算。
 *
 * 纯形状与纯函数：零 DOM 操作、零 IPC。`Tab` 上挂着的那几样运行期对象（流 / 时间线 / 折叠层 /
 * 尾部窗口 / 骨架 / 大纲）只以**类型**出现在这里，谁建它们住 `tab-stream-view.ts`。
 * 原住 `tabs.ts`（`TabManager` 同文件），逐字搬出；`tabs.ts` 原样 re-export，既有 import 面零改动。
 */
import type { MessageStream } from "./stream";
import type { BranchFolder } from "./branch-fold";
import type { JsonlLinePayload } from "./events";
import type { RecordTimeline } from "./record-timeline";
import type { SeqSet, TailWindow } from "./live-window";
import type { SkeletonView } from "./skeleton-view";
import type { UserInputPanel } from "./views/user-input-panel";
import type { OutlineSource } from "./views/outline-source";
import type { TurnFold } from "./turn-fold";
import type { TurnRail } from "./turn-rail";
import type { FactsSource } from "./views/facts-source";
import type { ToolResultBlock, ToolUseSeen } from "./cards/index";
import type { Origin } from "./ipc/origin";
import type { SessionActivity } from "./generated/SessionActivity";
import type { LineRecord } from "./generated/LineRecord";
import type { SessionState } from "./tab-session-state";
import type { BackgroundWork, Needs, PendingCall, RetryOutcome, UsageFact } from "./session-reads";

// 会话状态住 `tab-session-state.ts` 的 `SessionState`（活性 × 可恢复性），字段是 `Tab.state`。

export interface Tab {
  sessionId: string;
  /** 后台会话（后端判好的 `background`）⇒ ⚙ 标题。 */
  background: boolean;
  /** bg 任务名（pidfile 的 name 字段）；bg 标题优先用它。 */
  bgName: string | null;
  /**
   * Tab 标题。优先级：[项目] aiTitle > 项目名 > session_id 前 8 位。
   * aiTitle 一旦出现就锁住。
   */
  title: string;
  /**
   * 会话的项目目录（会话起在哪个目录）：只来自后端（会话宣告那一帧的 `project_dir` · 会话事实的 `projectDir`，或固定 tab 存下来的那一份）。
   * 标题 · 打开工作目录 · 分组 · resume / 分叉的起始目录都用它；行里各自的 cwd 不读（shell 进了子目录之后写的是子目录）。
   * `null` = 后端没给（老后端）。
   */
  projectDir: string | null;
  /**
   * 这个会话是哪一家（线上的 kind）：只来自后端（会话事实的 `agent`，那台按记录认）。恢复 · 接回 · 选号 · 账号面板 · 卡头都按它；
   * `null` ＝ 事实还没到 / 认不出 ⇒ 要分家的那几项灰着，不落哪一家。
   */
  agent: string | null;
  /** Claude 给出的语义标题（JSONL 里 `ai-title` 记录的 aiTitle 字段），出现一次就锁定 */
  aiTitle: string | null;
  /**
   * 本会话是从哪个会话 fork 来的（判定住后端 `history_query::fork_origin`，与历史树同一份；出现一次就锁定）。null = 非 fork。
   * 给 tab 标题加 `↳` 血缘徽标 ＋ 悬停 —— 不然 fork 出来的会话与原会话是同名的两个 tab、分不清。
   */
  forkedFromSessionId: string | null;
  /** 此刻持着这条会话的活进程 pid（后端会话事实的 `writers`，到了才有）。不止一个 ⇒ 几个进程在同时写这条会话。 */
  writers: number[];
  /**
   * 数据来源：本机 = `LOCAL_ORIGIN`（标题无前缀）；远端 = 那台的名字（标题加 `[origin]` 前缀）。
   * 首条行的 origin 决定，之后不变。判本机 / 远端一律经 `ipc/origin.ts`。
   */
  origin: Origin;
  /**
   * **活性 × 可恢复性**两个轴（形状、转移与谓词都住 `tab-session-state.ts`）。
   * 只经那一份的 `nextState` 改、经它的谓词读 —— 别处不各判一遍。
   */
  state: SessionState;
  /**
   * **固定** —— 「关了 app 再打开它还在」。
   *
   * 与会话状态正交的一维（不是第三态）：主用例就是固定住一个已经跑完的会话。三个维度各管一件事：
   * `state`（进程活没活、死了怎么回去，用户改不了）· `pinned`（要不要它一直在，只由用户改）· 集合归属（怎么分类）。
   * 只影响「重启后还在不在」，不影响位置（固定的 tab 留在原位，多一个 📌 角标）。
   * 活着的 tab 也能固定（「跑完之后别丢」），效果在它变灰之后才显现 —— 活着的重启后由后端自动宣告回来。
   */
  pinned: boolean;
  /**
   * **这个 tab 在哪个组**（组 id；`null` = 散 tab）。
   *
   * 组员关系是 tab 自己的属性（tab × 掉就没了，不用再「移出分组」）：组表（`tab-collections.ts`）只存 `{id, name}`，
   * 「组里有谁」= `group` 等于那个 id 的 tab，现算。内存里组员关系只住这里；落盘是 `tabBar.groupOf.<sid>`（`tab-bar-state.ts`）。
   * 只经 `tab-bar-prefs.ts` 的那几个分组动作改（它们同时写盘）；tab 被 × 掉，这一格随 tab 一起没。
   * 单值 ⇒ 「一个 tab 只属一个集合」结构上成立。与 `state` / `pinned` 正交（不变量 3）。
   */
  group: string | null;
  /**
   * 红绿灯（与 `state` 正交）：此刻在干什么（后端翻好的；`doing` 为 null ＝ 那一家没说）＋ 在等什么 ＋ 核心写好的字与语气（说不清也有）。null ＝ 还没收到。
   */
  activity: { doing: SessionActivity | null; waitingFor: string | null; text: string; tone: string; order: number } | null;
  // 「claude 已退但 tmux 会话还在」是 `state` 的 `RECONNECTABLE`（死 ＋ 容器还在）。
  /** 本会话写类工具（Edit / Write / MultiEdit / NotebookEdit）碰过的文件路径（原样、去重、近因序）。
   * 后端出成品（`history-facts` 的 `touchedFiles`），供监控板 peek 列最近改过的文件。纯内存、不落盘（守 §28）。 */
  touchedFiles: Set<string>;
  /** 本会话最新一轮的用量与上下文（后端 `history-facts` 的 `usage`：数 ＋ 写好的字，HUD 与监控板照抄）。null = 还没有（或事实还没到）。 */
  usage: UsageFact | null;
  /** 需手动（后端 `history-facts` 的 `needs`：种类 · 那一句 · 何时起等）；不在等 ⇒ `null`。与 `activity` 对不上时以后者为准（见 `tab-needs.ts`）。 */
  needs: Needs | null;
  /** 后台任务运行中那一句（后端 `history-facts` 的 `background`：写好的字 · 会走的那一句 · 命令那一格 · 几条）；不是这一态 ⇒ `null`。 */
  backgroundWork: BackgroundWork | null;
  /** 还没有结果的工具调用（后端 `pending`，文件序）：悬停卡「在做什么」。 */
  pending: PendingCall[];
  /** 最后一段正文的头一行（后端 `lastSay`）：悬停卡「它最后一句」。 */
  lastSay: { text: string; at: string | null; atMs: number | null } | null;
  /** 一串一串重试的结局（后端 `retries`：首条重试记录的 uuid → 结局）：消息流里的重试细条按它画。 */
  retries: ReadonlyMap<string, RetryOutcome>;
  /**
   * 这份会话的事实从哪来：问后端要（`views/facts-source.ts`）。
   * 上面四样（分叉血缘 · agent 列表 · 改动文件集 · 最新 usage）只经它落下来 —— `onLine` 上没有旁路记账。
   */
  facts: FactsSource;
  /** 按轮折叠（后端 `history-turns` 的成品 ⇒ 完成的轮过程折成一行，`turn-fold.ts`）。 */
  turnFold: TurnFold;
  /** 轮次刻度（同一份轮，`turn-rail.ts`）。 */
  turnRail: TurnRail;
  streamEl: HTMLElement;
  stream: MessageStream;
  /** 父 JSONL 路径（subagent 加载需要） */
  parentPath: string;
  unread: number;
  /** 按 seq 排序的 timeline：renderStreamRecord 调 `insert / peekPrev` 决定挂载位置与工具组合并。 */
  timeline: RecordTimeline;
  /**
   * 工具调用 id → 那次调用的工具名与卡型（`cards/index.ts::ToolUseSeen`）。调用在回复记录出现时记下，
   * 下一条 said 记录的工具结果反查显示工具名、挑默认怎么画。
   */
  toolUseNames: Map<string, ToolUseSeen>;
  /**
   * 工具调用 id → tool_use 折叠条 DOM。tool_result 直接注入对应 tool_use 内部（cards/index.ts 的 injectOrBuildToolResult）。
   */
  toolUseElements: Map<string, HTMLElement>;
  /** 派出子运行的那几张卡：父侧工具调用 id → 卡（运行表到了按它给那张卡标上是哪个子运行、什么状态）。 */
  runCards: Map<string, HTMLElement>;
  /**
   * ESC 回退分支折叠管理器：按那台后端给的主线外清单（会话流的 `branch` 格）把「被回退」的连续段
   * 包到可折叠容器里。
   */
  branchFolder: BranchFolder;
  /** tool_result 可能先于它的 tool_use 到：先画独立卡，批结束后 reconcilePendingToolResults 重新配、注入。 */
  pendingToolResults: Map<
    string,
    { block: ToolResultBlock; element: HTMLElement }
  >;
  /** 出口省掉的正文展开那一下按记录 id 取回那一行全文（`RenderContext.fullRecord`；按这个 tab 的骨架取）。 */
  fullRecord: (id: string) => Promise<LineRecord | null>;
  /**
   * 按 seq 去重集合。一个 Tab == 一个 jsonl == 一个 seq 空间（seq ＝ 当前文件里的行号）：重连后后端从 0 重发 → 命中即丢，Tab 内容不翻倍。
   * 这是入口唯一一道去重：文件从头重读时后端先出声、行号从 0 重数，这个 tab 整份重来（`TabStreamView.restartContent`，新的一代配新的集合；INVARIANTS § 25）。
   * closeTab 时 clear。
   */
  seenSeqs: SeqSet;
  /**
   * 尾部优先窗口账本（单洞后缀不变量，见 live-window.ts）：启动重放的旧记录不建卡、收在这里（标题已喂）；
   * floor = null（virgin）的后台 tab 在 onBatchEnd 后空闲物化尾段，switchTo 命中时同步物化。
   */
  window: TailWindow;
  /**
   * 〔骨架〕骨架层：`[0, floor)` 那段没物化的历史由占位顶住（总高与滚动条
   * 一开始就是全会话的），滚到哪里只物化哪里。`null` = 没接上 —— 没拿到索引（老后端 /
   * 本机后端不在 / seq 与索引对不上）⇒ 退回 `window` 的尾部窗口 + `fillAbove`，行为与之前逐字相同。
   */
  skeleton: SkeletonView | null;
  /** 索引拉取：`idle` 没拉过 · `pending` 在途 · `again` 瞬时失败过一次、下一次触发点再问一次 · `done` 定了（不再拉）。 */
  skeletonFetch: "idle" | "pending" | "again" | "done";
  /**
   * 批期「窗口内中部插入」缓冲：大增量批（切块、末块先发）落在已渲染的 tab 上时，老块 seq ≥ floor 但 < timeline.maxSeq，
   * 逐条挂会跨帧往上方插（§21）⇒ 缓冲到 onBatchEnd 一次挂载。
   */
  midBatchBuffer: JsonlLinePayload[];
  /** 上翻补批的 scroll listener（closeTab 摘） */
  fillHandler: (() => void) | null;
  /**
   * 大纲的数据源 —— 问后端要（`--list-user-inputs`），不在前端攒（到达序 ≠ 对话序）。
   * 本 tab 只持有「上次要到哪个字节」与已列出的 uuid，不存正文；摘要在面板的行上。
   */
  outline: OutlineSource;
  /** 清单界面（与历史查看器**同一个类**）。开关 + 面板都挂在 `inputsEl` 里。 */
  inputsPanel: UserInputPanel;
  /**
   * 清单那块悬浮层。实时 tab 没有查看器那样的顶栏可以塞开关 —— 每个 tab 只有一个
   * `.stream`（`position:absolute; inset:0`），所以做成与它**平级**的一层，
   * `.active` 跟着 tab 一起翻（同一套 visibility 机制，切 tab 0 reflow）。
   */
  inputsEl: HTMLElement;
}

/**
 * Tab 摘要，发给宿主用于状态栏 / empty-state / 会话头 / 终端页 / 需手动那几处。按活性轴分：活 / 死（死里含可重连）；
 * `faces` 是每个 tab 此刻的点 · 在等什么 · 在跑哪一步（只拿来比「变没变」：运行中 → 空闲数量不变，会话头也得重画）。
 */
export interface TabsSummary {
  total: number;
  live: number;
  dead: number;
  faces: string;
}

export function projectNameFromCwd(dir: string): string | null {
  const normalized = dir.replace(/\\/g, "/").replace(/\/+$/, "");
  const last = normalized.split("/").filter(Boolean).pop();
  return last ?? null;
}

/**
 * 标题格式：
 *   aiTitle 有 + 项目目录有 → `[项目] aiTitle`
 *   aiTitle 有 + 项目目录无 → `aiTitle`
 *   aiTitle 无 + 项目目录有 → `项目`
 *   都没有 → `<sid 前 8 位>`
 * 项目 = 项目目录（`Tab.projectDir`，后端给的那一格）的最后一段。
 *
 * 远端 Tab 再加 `[那台远端] ` 前缀（如 `[raspberrypi.local] [proj] aiTitle`）；本机不加。
 * 第四个参数是**前缀里写的那台远端名**（本机 ⇒ `null`，没有前缀），不是 origin：
 * 「是不是本机」由调用方经 `ipc/origin.ts` 判完再传进来 —— 本文件只有类型，零运行期依赖（`tabs-split-graph` 钉着）。
  */
export function computeTitleFor(
  sessionId: string,
  projectDir: string | null,
  aiTitle: string | null,
  remoteLabel: string | null = null,
  background = false,
  bgName: string | null = null,
  forkedFromSessionId: string | null = null,
): string {
  // fork 会话在最终标题前加 `↳ ` 血缘徽标 —— 与同名的原会话区分开。
  const mark = (s: string): string => (forkedFromSessionId ? `↳ ${s}` : s);
  const project = projectDir ? projectNameFromCwd(projectDir) : null;
  // bg 任务 → ⚙ ＋ 任务名
  if (background) {
    const base = `⚙ ${bgName ?? aiTitle ?? project ?? sessionId.slice(0, 8)}`;
    return mark(remoteLabel !== null ? `[${remoteLabel}] ${base}` : base);
  }
  let base: string;
  if (aiTitle) {
    base = project ? `[${project}] ${aiTitle}` : aiTitle;
  } else if (project) {
    base = project;
  } else {
    base = sessionId.slice(0, 8);
  }
  return mark(remoteLabel !== null ? `[${remoteLabel}] ${base}` : base);
}
