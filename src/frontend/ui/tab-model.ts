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
import type { ToolUseSeen } from "./cards/index";
import type { Origin } from "./ipc/origin";
import type { SessionState } from "./tab-session-state";
import type { Needs, PendingCall, UsageFact } from "./session-reads";

// 原先这里是 `TabStatus = "live" | "archived"`（与下面的 `tmuxIdle` 一起挤着两个轴）。
//   会话状态改住 `tab-session-state.ts` 的 `SessionState`（活性 × 可恢复性），字段是 `Tab.state`。

export interface Tab {
  sessionId: string;
  /** Batch7-F24：会话类型（"interactive"/"bg"/null=未知视为交互）。bg → ⚙ 标题（不再挂树，见 `isBgKind`）。 */
  kind: string | null;
  /** Batch7-F24：bg 任务名（pidfile name 字段）；bg 标题优先用它。 */
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
   * issue #63①：本会话是从哪个会话 fork 来的（首条带 `forkedFrom` 的 user / assistant 记录的 `forkedFrom.sessionId`，
   * 出现一次就锁定，同 aiTitle；判定住后端 `history_query::fork_origin`，与历史树同一份）。null = 非 fork。用于给 tab 标题加 `↳` 血缘徽标 + tooltip——否则 fork
   * 出来的会话与原会话是**同名独立 tab**、肉眼分不清（活 tab 层原本只按 sessionId keyed、完全不看
   * `forkedFrom`，它此前只在历史树用）。
   */
  forkedFromSessionId: string | null;
  /** 此刻持着这条会话的活进程 pid（后端会话事实的 `writers`，到了才有）。不止一个 ⇒ 几个进程在同时写这条会话。 */
  writers: number[];
  /**
   * issue #15：数据来源主机标签。本机 = `LOCAL_ORIGIN`（标题无前缀）；远端（如 "raspberrypi.local"）
   * = 远端 SSH 主机名，标题加 `[origin]` 前缀以区分本地/远端。首条 line 帧的 origin
   * 决定，之后不变（同一 sid 只来自一个来源）。
   * 本机不再是 `null`：判本机 / 远端一律经 `ipc/origin.ts`。
   */
  origin: Origin;
  /**
   * **活性 × 可恢复性**两个轴（形状、转移与谓词都住 `tab-session-state.ts`）。
   * 只经那一份的 `nextState` 改、经它的谓词读 —— 别处不再各判一遍。
   */
  state: SessionState;
  /**
   * **固定** —— 「关了 app 再打开它还在」。
   *
   * 🔴 **必须是正交的一维，不能做成会话状态的第三态**（`§B.3` 逐字）：
   * `archived + pinned` 才是用户的主用例（固定住一个**已经跑完**的会话），
   * 做成第三态就表达不了它。三个维度各管一件事：
   * `state`（进程活没活、死了怎么回去，用户改不了）· `pinned`（你要不要它一直在，只由用户改）·
   * 集合归属（你怎么分类）。
   *
   * ⚠ **它只影响「重启后还在不在」，不影响位置**（`§B.3b` 用户 2026-09-16 收窄）：
   *   没有「固定区」，固定的 tab **留在原位**，只多一个 📌 角标；位置由 `§C` 的顺序落盘管。
   * ⚠ live tab 也可以打 pin（「这个会话我还在跑，但**它跑完之后别丢**」是真实意图），
   *   但**效果只在它变灰之后才显现** —— live 的重启后由后端 `event_replay` 自动宣告回来。
   */
  pinned: boolean;
  /**
   * **这个 tab 在哪个组**（组 id；`null` = 散 tab）。
   *
   * 用户原话「分组不应该单独存会话记录. x就是没了, 不存在还要移出分组」⇒ 组员关系是 **tab 自己的属性**：
   * 组表（`tab-collections.ts`）只存 `{id, name}`，「组里有谁」= `group` 等于那个 id 的 tab，现算。
   * 🔴 内存里组员关系**唯一的住址**；落盘是 `tabBar.groupOf.<sid>`（`tab-bar-state.ts`）。
   * 只经 `tab-bar-prefs.ts` 的那几个分组动作改（它们同时写盘）；tab 被 × 掉，这一格随 tab 一起没。
   * 单值 ⇒ 「一个 tab 只属一个集合」结构上成立。与 `state` / `pinned` 正交（不变量 3）。
   */
  group: string | null;
  /**
   * issue #23：红绿灯（与 `state` 正交）。null=未知（旧版 CC
   * 无 status 字段 / 远端 v1 暂无透传）→ 维持现状绿点。
   */
  activity: { status: string; waitingFor: string | null } | null;
  // 原先这里是 `tmuxIdle: boolean`（「claude 已退但 tmux 会话还在」，与 `status` 正交、却让 `status` 留在 live）。
  //   它说的是可恢复性那一轴 ⇒ 并进 `state`：`RECONNECTABLE`（死 ＋ 容器还在）。
  /** F70：本会话写类工具（Edit/Write/MultiEdit/NotebookEdit）碰过的文件路径（原样、去重、近因序）。
   * 后端出成品（`history-facts` 的 `touchedFiles`），供监控板 peek 列最近改过的文件。纯内存、不落盘（守 §28）。 */
  touchedFiles: Set<string>;
  /** F88b：本会话**最新一条带 usage 的 assistant 记录**的 prompt token（input+cache 合计）与
   *  model——供 HUD 算 context 占用%。后端按**文件序**取最后一条（成品的 `usage`），不再看到达序。
   *  null=尚无带 usage 的 assistant 记录（或事实还没到）。 */
  latestPromptTokens: number | null;
  latestModel: string | null;
  /** 这份会话的上下文上限（后端定的，见 `session-reads.ts::UsageFact`）；判不出 ⇒ `null`（只写用了多少）。与上两格同一份成品。 */
  latestContextLimit: number | null;
  /** 上限从哪来（与上一格同一份成品；`assumed` ＝ 判不出）。 */
  latestLimitFrom: UsageFact["limitFrom"];
  /** 需要你（后端 `history-facts` 的 `needs`：种类 · 那一句 · 何时起等）；不在等 ⇒ `null`。与 `activity` 对不上时以后者为准（见 `tab-needs.ts`）。 */
  needs: Needs | null;
  /** 还没有结果的工具调用（后端 `pending`，文件序）：悬停卡「在做什么」。 */
  pending: PendingCall[];
  /** 最后一段正文的头一行（后端 `lastSay`）：悬停卡「它最后一句」。 */
  lastSay: { text: string; at: string | null } | null;
  /**
   * 这份会话的事实从哪来：问后端要（`views/facts-source.ts`）。
   * 上面四样（分叉血缘 · agent 列表 · 改动文件集 · 最新 usage）只经它落下来 —— `onLine` 上不再有旁路记账员。
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
  /**
   * P5.2 B 重构：按 seq 排序的 timeline。renderStreamRecord 调
   * `timeline.insert / peekPrev` 决定 DOM 挂载位置 + tool-group 合并。
   * **取代**：原 pendingToolGroup（tool-group 合并改后处理，看 timeline 左邻居）
   * 和 pendingPrependFragment（无 source/inPrependMode 概念，永远按 seq 插入）。
   */
  timeline: RecordTimeline;
  /**
   * tool_use_id → 那次 tool_use 的工具名与卡型（`cards/index.ts::ToolUseSeen`）。tool_use 在 assistant 消息出现时记下，
   * 下一条 user 消息的 tool_result 反查显示工具名、挑默认怎么画。
   */
  toolUseNames: Map<string, ToolUseSeen>;
  /**
   * tool_use_id → tool_use 折叠条 DOM。tool_result 直接注入对应 tool_use
   * 内部，不再产生独立折叠条。详见 cards/index.ts 的 injectOrBuildToolResult。
   */
  toolUseElements: Map<string, HTMLElement>;
  /** 派出子运行的那几张卡：父侧工具调用 id → 卡（运行表到了按它给那张卡标上是哪个子运行、什么状态）。 */
  runCards: Map<string, HTMLElement>;
  /**
   * issue #8: ESC 回退分支折叠管理器。
   * 跟踪本 Tab 内所有有 uuid 的卡片，监听 parentUuid 分叉，把"被回退"的连续段
   * 包到可折叠容器里。工具组（tool-group）不参与折叠（无单一 uuid）。
   */
  branchFolder: BranchFolder;
  /**
   * v2.3.1 (issue #1)：tool_result 可能在 tool_use 之前到达（jsonl 行序错位 /
   * 不同 session 文件混杂）。先走 fallback 渲染独立卡，batch 结束后
   * reconcilePendingToolResults 重新匹配 + 注入。
   */
  pendingToolResults: Map<
    string,
    {
      block: { type: "tool_result"; tool_use_id: string; content: unknown; is_error?: boolean };
      element: HTMLElement;
    }
  >;
  /**
   * 按 seq 去重集合。一个 Tab == 一个 jsonl path == 一个 seq 空间（那台机器后端的 per-process SeqCounter；
   * CF1 起本机会话也走本机后端，从前「本地 watcher 的 per-path seqs」那一份没了）。重连后新后端会从
   * seq 0 重发整个会话 → 命中即丢，避免 Tab 内容翻倍（本机后端重连也一样，从前「本地 seq 全程唯一 → 永不命中」不再成立）。
   *
   * **这是入口唯一一道去重**：seq ＝ 当前文件里的行号；文件从头重读时后端先出声、
   * 行号从 0 重数，这个 tab 整份重来（`TabStreamView.restartContent`，新的一代配一个新的集合）⇒ 不再有
   * 「换新 seq 重投同一条记录」（INVARIANTS § 25）。拓扑那一层的 uuid 幂等（computeMainBranch 入口去重 ＋
   * BranchFolder.seenUuids，#25）照留。closeTab 时 clear。
   */
  seenSeqs: SeqSet;
  /**
   * Batch13-F40a:尾部优先窗口账本(单洞后缀不变量,详 live-window.ts)。
   * 启动重放的旧记录不建卡、收纳于此(meta/branch 数据已喂);floor=null(virgin)
   * 的后台 tab 在 onBatchEnd 空闲物化尾段 / switchTo 命中时同步物化。
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
   * F40b R-1:批期「窗口内中部插入」缓冲——大增量批(>600 行切块,末块先发)落在
   * 已渲染 tab 上时,老块 seq≥floor 但 <timeline.maxSeq,逐条挂 DOM 会跨帧上方
   * 插入(§21 病根)。缓冲到 onBatchEnd 一次 batchInsert 挂载。
   */
  midBatchBuffer: JsonlLinePayload[];
  /** F40b:上翻补批 scroll listener 引线(closeTab 摘) */
  fillHandler: (() => void) | null;
  /**
   * 大纲的数据源 —— **问后端要**（`--list-user-inputs`），不在前端攒。
   * 本 tab 只持有「上次要到哪个字节」与已列出的 uuid，不存正文；摘要在面板的行上。
   * 原先这里是 `userInputs` 旁路账本（`onLine` 一条一条攒，到达序 ≠ 对话序），已删。
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

/** Tab 数量摘要，发给宿主用于状态栏 / empty-state 等外部 UI。按活性轴分：活 / 死（死里含可重连）。 */
export interface TabsSummary {
  total: number;
  live: number;
  dead: number;
}

export function projectNameFromCwd(dir: string): string | null {
  const normalized = dir.replace(/\\/g, "/").replace(/\/+$/, "");
  const last = normalized.split("/").filter(Boolean).pop();
  return last ?? null;
}

/**
 * 〔「删掉树」〕「这是不是 bg 会话」在 tab 代码里的**唯一**判法（原先同一条式子散写四处：
 * 树状落位 · 拖拽块 · `.tab-bg` 类 · 标题）。kind 缺失（旧 CC）恒视为交互。
 *
 * 树删了之后按它分叉的只剩两处，都不是 tab 栏：标题的 `⚙`（`computeTitleFor`）与同 sid 两份身份的
 * 升格（`tabs.ts::ensureTab`）。tab 栏通用代码（落位 · 拖拽 · 集合 · 渲染）零处 —— `tests/frontend/ui/bg-flat.vitest.ts`。
 */
export function isBgKind(kind: string | null): boolean {
  return kind !== null && kind !== "interactive";
}

/**
 * 标题格式（决策见 project_monitor_decisions.md）：
 *   aiTitle 有 + 项目目录有 → `[项目] aiTitle`
 *   aiTitle 有 + 项目目录无 → `aiTitle`
 *   aiTitle 无 + 项目目录有 → `项目`
 *   都没有 → `<sid 前 8 位>`
 * 项目 = 项目目录（`Tab.projectDir`，后端给的那一格）的最后一段。
 *
 * issue #15：远端 Tab 在以上结果前再加 `[那台远端] ` 前缀，
 * 让用户一眼区分本地 / 远端 Tab（如 `[raspberrypi.local] [proj] aiTitle`）。本机行为与历史完全一致，
 * 不加任何前缀。
 * 第四个参数是**前缀里写的那台远端名**（本机 ⇒ `null`，没有前缀），不是 origin：
 * 「是不是本机」由调用方经 `ipc/origin.ts` 判完再传进来 —— 本文件只有类型，零运行期依赖（`tabs-split-graph` 钉着）。
 *
 * Subagent 不再独立 Tab（嵌入到父 session 的 Task 折叠卡），所以没有 `↳` 前缀分支。
 */
export function computeTitleFor(
  sessionId: string,
  projectDir: string | null,
  aiTitle: string | null,
  remoteLabel: string | null = null,
  kind: string | null = null,
  bgName: string | null = null,
  forkedFromSessionId: string | null = null,
): string {
  // issue #63①:fork 会话在最终标题前加 `↳ ` 血缘徽标——与原会话(同名)区分开。
  const mark = (s: string): string => (forkedFromSessionId ? `↳ ${s}` : s);
  const project = projectDir ? projectNameFromCwd(projectDir) : null;
  // Batch7-F24：bg 任务 → ⚙ + 任务名（原先还有缩进 / ⌞ 的 `.tab-bg` 样式，随树一起删了）
  if (isBgKind(kind)) {
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
