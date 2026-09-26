/**
 * 〔U2 · 拆 `tabs.ts` ①〕**一个实时 tab 长什么样** ＋ 它的标题怎么算。
 *
 * 纯形状与纯函数：零 DOM 操作、零 IPC。`Tab` 上挂着的那几样运行期对象（流 / 时间线 / 折叠层 /
 * 尾部窗口 / 骨架 / 大纲）只以**类型**出现在这里，谁建它们住 `tab-stream-view.ts`。
 * 原住 `tabs.ts`（`TabManager` 同文件），逐字搬出；`tabs.ts` 原样 re-export，既有 import 面零改动。
 */
import type { MessageStream } from "./stream";
import type { BranchFolder } from "./branch-fold";
import type { JsonlLinePayload } from "./events";
import type { RecordTimeline } from "./record-timeline";
import type { TailWindow } from "./live-window";
import type { SkeletonView } from "./skeleton-view";
import type { UserInputPanel } from "./views/user-input-panel";
import type { OutlineSource } from "./views/outline-source";
import type { AgentEntry } from "./agents-panel";
import type { Origin } from "./ipc/origin";
import type { SessionState } from "./tab-session-state";

// 〔U4〕原先这里是 `TabStatus = "live" | "archived"`（与下面的 `tmuxIdle` 一起挤着两个轴）。
//   会话状态改住 `tab-session-state.ts` 的 `SessionState`（活性 × 可恢复性），字段是 `Tab.state`。

export interface Tab {
  sessionId: string;
  /** Batch7-F24：会话类型（"interactive"/"bg"/null=未知视为交互）。bg → ⚙ 标题（〔BG1〕不再挂树，见 `isBgKind`）。 */
  kind: string | null;
  /** Batch7-F24：bg 任务名（pidfile name 字段）；bg 标题优先用它。 */
  bgName: string | null;
  /**
   * Tab 标题。优先级：[项目] aiTitle > 项目名 > session_id 前 8 位。
   * aiTitle 一旦出现就锁住，后续 cwd 不再回退。
   */
  title: string;
  cwd: string | null;
  /**
   * `cwd` 来源记录的 seq。取**最小 seq（最早记录）**的 cwd = 项目根 / 启动目录。
   * 会话的 cwd 可能中途漂移到子目录；用最早记录的 cwd 才稳定指向项目根（与历史
   * 浏览器 quick_extract_cwd 口径一致）。Infinity = 尚未拿到任何带 cwd 的记录。
   */
  cwdSeq: number;
  /** Claude 给出的语义标题（JSONL 里 `ai-title` 记录的 aiTitle 字段），出现一次就锁定 */
  aiTitle: string | null;
  /**
   * issue #63①：本会话是从哪个会话 fork 来的（首条带 `forkedFrom` 的记录的 `forkedFrom.sessionId`，
   * 出现一次就锁定，同 aiTitle）。null = 非 fork。用于给 tab 标题加 `↳` 血缘徽标 + tooltip——否则 fork
   * 出来的会话与原会话是**同名独立 tab**、肉眼分不清（活 tab 层原本只按 sessionId keyed、完全不看
   * `forkedFrom`，它此前只在历史树用）。
   */
  forkedFromSessionId: string | null;
  /**
   * issue #15：数据来源主机标签。本机 = `LOCAL_ORIGIN`（标题无前缀）；远端（如 "raspberrypi.local"）
   * = 远端 SSH 主机名，标题加 `[origin]` 前缀以区分本地/远端。首条 line 帧的 origin
   * 决定，之后不变（同一 sid 只来自一个来源）。
   * 〔C4a · `设计/05 §8` 步 2〕本机不再是 `null`：判本机 / 远端一律经 `ipc/origin.ts`。
   */
  origin: Origin;
  /**
   * 〔U4 · `设计/01 §6.2`〕**活性 × 可恢复性**两个轴（形状、转移与谓词都住 `tab-session-state.ts`）。
   * 只经那一份的 `nextState` 改、经它的谓词读 —— 别处不再各判一遍。
   */
  state: SessionState;
  /**
   * 〔步 17·B · `设计/30 §B`〕**固定** —— 「关了 app 再打开它还在」。
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
   * issue #23：红绿灯（与 `state` 正交）。null=未知（旧版 CC
   * 无 status 字段 / 远端 v1 暂无透传）→ 维持现状绿点。
   */
  activity: { status: string; waitingFor: string | null } | null;
  // 〔U4〕原先这里是 `tmuxIdle: boolean`（「claude 已退但 tmux 会话还在」，与 `status` 正交、却让 `status` 留在 live）。
  //   它说的是可恢复性那一轴 ⇒ 并进 `state`：`RECONNECTABLE`（死 ＋ 容器还在）。
  /**
   * issue #23（第二增量）：本会话的 subagent 列表（tool_use id → entry，插入序）。
   * jsonl 流里配对 Task/Agent 的 tool_use（running）与 tool_result（done）；
   * 变 idle/归档时把仍 running 的标 aborted。上限 30，超出删最老的非 running。
   */
  agents: Map<string, AgentEntry>;
  /** F70：本会话写类工具（Edit/Write/MultiEdit/NotebookEdit）碰过的文件路径（原样、去重）。
   * onLine 增量累进，供「点会话 → 全景图高亮它改过的节点」。纯内存、不落盘（守 §28）。 */
  touchedFiles: Set<string>;
  /** F88b：本会话**最新一条带 usage 的 assistant 记录**的 prompt token（input+cache 合计）与
   *  model——供 HUD 算 context 占用%。onLine 捕获、纯内存。null=尚无带 usage 的 assistant 记录。 */
  latestPromptTokens: number | null;
  latestModel: string | null;
  /** F88b（审计）：产出上面两值的记录 seq。重放/远端重投的 onLine **投递序不保证升序**
   *  （timeline 靠 seq 排序而非到达序），故 trackUsage 只在 seq ≥ 此值时覆盖，保证「最新」= 最大 seq
   *  而非最后到达。init -1（任何 seq≥0 首次即可写）。 */
  latestUsageSeq: number;
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
   * tool_use_id → tool_name 缓存。tool_use 在 assistant 消息出现时记下，
   * 下一条 user 消息的 tool_result 反查显示工具名。
   */
  toolUseNames: Map<string, string>;
  /**
   * tool_use_id → tool_use 折叠条 DOM。tool_result 直接注入对应 tool_use
   * 内部，不再产生独立折叠条。详见 cards/index.ts 的 injectOrBuildToolResult。
   */
  toolUseElements: Map<string, HTMLElement>;
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
   * 〔TL1 · 4C〕CF1 起本机会话也走本机后端，从前「本地 watcher 的 per-path seqs」那一份没了）。重连后新后端会从
   * seq 0 重发整个会话 → 命中即丢，避免 Tab 内容翻倍（本机后端重连也一样，从前「本地 seq 全程唯一 → 永不命中」不再成立）。
   *
   * 注意：本集合**只防同 seq 重投**。后端的 jsonl 读者截断重读是**换新 seq** 重投整个
   * 文件、此处放行（at-least-once 投递，INVARIANTS § 25）——uuid 级幂等由下面的
   * processedUuids（#26）+ computeMainBranch 入口去重 + BranchFolder.seenUuids
   * （#25）分层兜住。closeTab 时 clear。
   */
  seenSeqs: Set<number>;
  /**
   * Batch13-F40a:尾部优先窗口账本(单洞后缀不变量,详 live-window.ts)。
   * 启动重放的旧记录不建卡、收纳于此(meta/branch 数据已喂);floor=null(virgin)
   * 的后台 tab 在 onBatchEnd 空闲物化尾段 / switchTo 命中时同步物化。
   */
  window: TailWindow;
  /**
   * 〔`设计/10` 骨架 · 子步 4〕骨架层：`[0, floor)` 那段没物化的历史由占位顶住（总高与滚动条
   * 一开始就是全会话的），滚到哪里只物化哪里。`null` = 没接上 —— 没拿到索引（老后端 /
   * 本机后端不在 / seq 与索引对不上）⇒ 退回 `window` 的尾部窗口 + `fillAbove`，行为与之前逐字相同。
   */
  skeleton: SkeletonView | null;
  /** 索引拉取：`idle` 没拉过 · `pending` 在途 · `done` 拉过（成不成都不重拉，免得每次切 tab 起一次进程） */
  skeletonFetch: "idle" | "pending" | "done";
  /**
   * F40b R-1:批期「窗口内中部插入」缓冲——大增量批(>600 行切块,末块先发)落在
   * 已渲染 tab 上时,老块 seq≥floor 但 <timeline.maxSeq,逐条挂 DOM 会跨帧上方
   * 插入(§21 病根)。缓冲到 onBatchEnd 一次 batchInsert 挂载。
   */
  midBatchBuffer: JsonlLinePayload[];
  /** F40b:上翻补批 scroll listener 引线(closeTab 摘) */
  fillHandler: (() => void) | null;
  /**
   * issue #26：已处理记录的 uuid 集——onLine 入口的 at-least-once 幂等
   * （违反此约束见 src/doc/INVARIANTS.md § 25）。截断重读换新 seq 重投时 seenSeqs 放行，
   * 若不按 uuid 拒掉，每条记录会以更大的 seq 在 timeline 末尾再渲染一遍（整段内容
   * 翻倍），且 trackAgents/unread 等副作用也会被重投误触发——故在入口整体拒掉。
   * 无 uuid 的记录（ai-title/mode 等元信息）不占集合、照常处理（它们本身幂等；
   * 已知微小残留：无 uuid 的 system 细条理论上可翻倍，影响面可忽略）。closeTab 时 clear。
   */
  processedUuids: Set<string>;
  /**
   * 〔SE1 · `设计/10 §2.2b ⑥`〕大纲的数据源 —— **问后端要**（`--list-user-inputs`），不在前端攒。
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

/** Tab 数量摘要，发给宿主用于状态栏 / empty-state 等外部 UI。〔U4〕按活性轴分：活 / 死（死里含可重连）。 */
export interface TabsSummary {
  total: number;
  live: number;
  dead: number;
}

function projectNameFromCwd(cwd: string): string | null {
  const normalized = cwd.replace(/\\/g, "/").replace(/\/+$/, "");
  const last = normalized.split("/").filter(Boolean).pop();
  return last ?? null;
}

/**
 * 〔BG1 · V125「删掉树」〕「这是不是 bg 会话」在 tab 代码里的**唯一**判法（原先同一条式子散写四处：
 * 树状落位 · 拖拽块 · `.tab-bg` 类 · 标题）。kind 缺失（旧 CC）恒视为交互。
 *
 * 树删了之后按它分叉的只剩两处，都不是 tab 栏：标题的 `⚙`（`computeTitleFor`）与同 sid 两份身份的
 * 升格（`tabs.ts::ensureTab`）。tab 栏通用代码（落位 · 拖拽 · 集合 · 渲染）零处 —— `tests/bg-flat.vitest.ts`。
 */
export function isBgKind(kind: string | null): boolean {
  return kind !== null && kind !== "interactive";
}

/**
 * 标题格式（决策见 project_monitor_decisions.md）：
 *   aiTitle 有 + cwd 有 → `[项目] aiTitle`
 *   aiTitle 有 + cwd 无 → `aiTitle`
 *   aiTitle 无 + cwd 有 → `项目`
 *   都没有 → `<sid 前 8 位>`
 *
 * issue #15：远端 Tab 在以上结果前再加 `[那台远端] ` 前缀，
 * 让用户一眼区分本地 / 远端 Tab（如 `[raspberrypi.local] [proj] aiTitle`）。本机行为与历史完全一致，
 * 不加任何前缀。
 * 〔C4a〕第四个参数是**前缀里写的那台远端名**（本机 ⇒ `null`，没有前缀），不是 origin：
 * 「是不是本机」由调用方经 `ipc/origin.ts` 判完再传进来 —— 本文件只有类型，零运行期依赖（`tabs-split-graph` 钉着）。
 *
 * Subagent 不再独立 Tab（嵌入到父 session 的 Task 折叠卡），所以没有 `↳` 前缀分支。
 */
export function computeTitleFor(
  sessionId: string,
  cwd: string | null,
  aiTitle: string | null,
  remoteLabel: string | null = null,
  kind: string | null = null,
  bgName: string | null = null,
  forkedFromSessionId: string | null = null,
): string {
  // issue #63①:fork 会话在最终标题前加 `↳ ` 血缘徽标——与原会话(同名)区分开。
  const mark = (s: string): string => (forkedFromSessionId ? `↳ ${s}` : s);
  const project = cwd ? projectNameFromCwd(cwd) : null;
  // Batch7-F24：bg 任务 → ⚙ + 任务名（〔BG1〕原先还有缩进 / ⌞ 的 `.tab-bg` 样式，随树一起删了）
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
