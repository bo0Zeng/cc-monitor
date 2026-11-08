/**
 * **一个实时 tab 长什么样** ＋ 它的标题怎么算。
 *
 * 纯形状与纯函数：零 DOM 操作、零 IPC。`Tab` 上挂着的那几样运行期对象（流 / 时间线 / 折叠层 /
 * 尾部窗口 / 骨架 / 大纲）只以**类型**出现在这里，谁建它们住 `tab-stream-view.ts`。
 * `tabs.ts` 原样 re-export，从 `tabs.ts` 引的照旧引得到。
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
import type { RecordAt, ToolResultBlock, ToolUseSeen } from "./cards/index";
import type { Machine } from "./ipc/origin";
import type { SessionFace } from "./generated/SessionFace";
import type { LineRecord } from "./generated/LineRecord";
import type { AutoChoice } from "./generated/AutoChoice";
import type { AskEnd } from "./generated/AskEnd";
import type { BackgroundWork, Needs, PendingCall, UsageFact } from "./session-reads";
import type { RetryRun } from "./generated/RetryRun";
import type { SessionHidden, SessionLineage } from "./session-hidden";

export interface Tab {
  sessionId: string;
  /** 后台会话（后端判好的 `background`；监控板那一格照它换样子）。 */
  background: boolean;
  /**
   * Tab 标题：那台核心写好的整行（帧上 `title`：项目 · 分叉 · 后台那几个记号 · 名字），界面照抄、不拼。
   * 那台机器不在这一行里：要说哪台的地方另画那台的标签。
   */
  title: string;
  /**
   * 会话的项目目录（会话起在哪个目录）：只来自后端（会话宣告那一帧的 `project_dir` · 会话事实的 `projectDir`，或固定 tab 存下来的那一份）。
   * 标题 · 打开工作目录 · 分组 · resume / 分叉的起始目录都用它；行里各自的 cwd 不读（shell 进了子目录之后写的是子目录）。
   * `null` = 后端没给（老后端）。
   */
  projectDir: string | null;
  /**
   * 项目目录的显示形：那台按自己的家目录写好的 `~/…`（会话事实的 `cwdShown`）。会话头目录行与终端页页头「在哪」照它显，不显原样 `projectDir`；
   * 事实还没到 / 那台还没读到目录 ⇒ `null`（不显目录）。
   */
  cwdShown: string | null;
  /**
   * 这个会话是哪一家（线上的 kind）：只来自后端（会话事实的 `agent`，那台按记录认）。恢复 · 接回 · 选号 · 账号面板 · 卡头都按它；
   * `null` ＝ 事实还没到 / 认不出 ⇒ 要分家的那几项灰着，不落哪一家。
   */
  agent: string | null;
  /**
   * 叫这条会话什么（那台核心写的 `label`，经会话帧来，改名 · 来了标题 / 第一句随 `session_status` 更新；与历史清单 · 通知同一个函数）。
   * `null` ＝ 核心还没报过（没宣告过的会话，如从历史里打开的已结束会话）。
   */
  label: string | null;
  /**
   * 分叉来的那一句（后端会话事实的 `forkedLine`：「分叉自「父会话的名字」」，界面照摆）；不是分叉来的 ⇒ `null`。
   * 悬停卡那一行照它（标题里的 `↳` 是核心写进整行的）。
   */
  forkedLine: string | null;
  /** 这条会话在哪：「机器 · 目录」那一句（后端会话事实的 `whereLine`，那台照看的这一端对它的称呼写好，界面照摆）；没有 ⇒ `null`。 */
  whereLine: string | null;
  /**
   * 数据来源：本机 = `LOCAL`；远端 = 那台的键（标签页 · 会话头另画那台的标签）。
   * 首条行的 origin 决定，之后不变。判本机 / 远端一律经 `ipc/origin.ts`。
   */
  origin: Machine;
  /**
   * 此刻的整份脸（那台后端判好的点 · 字 · 序 · 能做什么，会话三帧带来；同一 sid 后到的覆盖先到的）。
   * `null` ＝ 一张都还没收到（恢复的标签页、重放还没到）⇒ 不画点；那台看不见时那一张也是后端造的（随「说不清」那一格来）。
   */
  face: SessionFace | null;
  /**
   * **固定** —— 「关了 app 再打开它还在」。
   *
   * 与会话状态正交的一维（不是第三态）：主用例就是固定住一个已经跑完的会话。固定是会话注解（住会话那一台，跟着组一起跨端）。三个维度各管一件事：
   * `face`（进程活没活、能做什么，用户改不了）· `pinned`（要不要它一直在，只由用户改）· 集合归属（怎么分类）。
   * 只影响「重启后还在不在」，不影响位置（固定的 tab 留在原位，多一个 📌 角标）。
   * 活着的 tab 也能固定（「跑完之后别丢」），效果在它变灰之后才显现 —— 活着的重启后由后端自动宣告回来。
   */
  pinned: boolean;
  /**
   * **这个 tab 画在哪个组**（组 id；`null` = 散 tab）。
   *
   * 组是后端的数据（组员关系是会话注解，`group-book.ts`）：这一格照组的账读回来的那一份填，一条会话在几组时画在排在最前的那一组；
   * 分组动作（`tab-bar-prefs.ts`）先改这一格、再交给那台。关掉 tab 不出组。与 `face` / `pinned` 正交（不变量 3）。
   */
  group: string | null;
  /**
   * **隐藏**那一格（后端判好的 `{by, said}`；不隐藏 ⇒ `null`）：会话宣告那一帧带来，推帧主题「会话隐藏」改它。
   * 隐藏的会话不占标签页，活着的收在栏底「已隐藏 N」那一行（`tab-bar-view.ts`）；需手动 · 通知照常。界面不判它，只照它画。
   */
  hidden: SessionHidden | null;
  /** 血缘（会话事实的 `startedBy` · `children`）：会话头「由 X 启动」「子会话 N」照它画。 */
  lineage: SessionLineage;
  /** 本会话写类工具（Edit / Write / MultiEdit / NotebookEdit）碰过的文件路径（原样、去重、近因序）。
   * 后端出成品（`history-facts` 的 `touchedFiles`），供监控板 peek 列最近改过的文件。纯内存、不落盘（守 §28）。 */
  touchedFiles: Set<string>;
  /** 本会话最新一轮的用量与上下文（后端 `history-facts` 的 `usage`：数 ＋ 写好的字，HUD 与监控板照抄）。null = 还没有（或事实还没到）。 */
  usage: UsageFact | null;
  /** 模型写给人看的字（后端 `history-facts` 的 `modelText`，会话头 · HUD · 监控板照抄）；没有 ⇒ `null`。 */
  modelText: string | null;
  /** 此刻是换模型清单里哪一项（后端 `history-facts` 的 `modelPick`，抽屉里勾它）；认不出 ⇒ `null`。 */
  modelPick: string | null;
  /** 此刻的权限档：原值（`permissionMode`，抽屉里勾哪一档）与写给人看的字（`permissionModeText`）；没有 ⇒ `null`。 */
  permissionMode: string | null;
  permissionModeText: string | null;
  /** 需手动（后端 `history-facts` 的 `needs`：种类 · 那一句 · 何时起等）；不在等 ⇒ `null`。脸上那颗点不说在等时不认（`session-face.ts::needsOf`）。 */
  needs: Needs | null;
  /** 没人答的提问等几分钟替人答（后端 `history-facts` 的 `autoAnswer`，会话注解里那一格）；关 ⇒ `null`。 */
  autoAnswer: number | null;
  /** 那一格可选的几档（后端 `history-facts` 的 `autoAnswerChoices`：几分钟 ＋ 写好的字，`0` 是关）；菜单照列。 */
  autoAnswerChoices: AutoChoice[];
  /** 上一条请求怎么收的场（`history-facts` 的 `askEnd`：后端写好的那一句 · 留不留着）；还没收过场 ⇒ `null`。 */
  askEnd: AskEnd | null;
  /** 后台任务运行中那一句（后端 `history-facts` 的 `background`：写好的字 · 会走的那一句 · 命令那一格 · 几条）；不是这一态 ⇒ `null`。 */
  backgroundWork: BackgroundWork | null;
  /** 还没有结果的工具调用（后端 `pending`，文件序）：悬停卡「在做什么」。 */
  pending: PendingCall[];
  /** 最后一段正文的头一行（后端 `lastSay`）：悬停卡「它最后一句」。 */
  lastSay: { text: string; at: string | null; atMs: number | null } | null;
  /** 一串一串的重试（后端 `retries`：首条重试记录的 uuid → 结局与写好的整句）：消息流里的重试细条照它画。 */
  retries: ReadonlyMap<string, RetryRun>;
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
  /** 按 seq 排序的 timeline：renderStreamRecord 调 `insert / peekPrev` 决定挂载位置与相邻合并。 */
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
  /** 出口省掉的正文展开那一下取回那一行全文（`RenderContext.fullRecord`；按这个 tab 的骨架取）。 */
  fullRecord: (from: RecordAt) => Promise<LineRecord | null>;
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
   * 大纲的数据源 —— 问后端要（`history-user-inputs`），不在前端攒（到达序 ≠ 对话序）。
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
 * Tab 摘要，发给宿主用于状态栏 / empty-state / 会话头 / 终端页 / 需手动那几处。按进程在不在分：活 / 死（死里含 Claude 已退出）；
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
