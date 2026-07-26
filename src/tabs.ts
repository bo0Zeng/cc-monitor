import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { MessageStream } from "./stream";
import {
  reconcilePendingToolResults,
  isCompactRecord,
  type RenderContext,
} from "./cards";
import { BranchFolder } from "./branch-fold";
import { fetchSessionTasks, type TaskEntry, type TasksPanel } from "./tasks-panel";
import type { JsonlLinePayload } from "./events";
import {
  sessionBadge,
  shouldShowAccountBadge,
  detectAccountMismatch,
  fetchAccounts,
  isSelectable,
  withAccount,
  type SessionAccount,
} from "./accounts";
import { restartWithAccount, DEFAULT_EXIT_WAIT_MS } from "./account-restart";
import { accountAvatarEl } from "./account-color";
import type { BehaviorConfig } from "./behavior";
import { showActionFailureToast } from "./error-toast";
import { RecordTimeline } from "./record-timeline";
import { TailWindow } from "./live-window";
import {
  renderContentRecord,
  routeMetaAndBranch,
  type StreamSink,
} from "./render-stream-record";
import type { BranchRecord } from "./branching";
import { isAgentTool } from "./cards/subagent";
import type { AgentsPanel, AgentEntry } from "./agents-panel";
import { LS_KEYS, safeSet } from "./local-storage";
import {
  runRemoteResume,
  runRemoteResumeTmux,
  runRemoteResumeIntoExistingTmux,
  runRemoteAttach,
} from "./remote-launch-run";
import { pickFreshTmuxName } from "./remote-launch";
import { AGENT_PROFILE } from "./agent-profile";
import { collectEditedFiles } from "./panorama/session-files";
import { openPanePreview } from "./views/pane-preview";
import { turnEndNotifier } from "./turn-notify";
import { getBehavior } from "./behavior";
// F78：远端会话「打开工作目录」→ 用该机配置开 SFTP 面板进入远端 cwd（而非只提示打不开）。
import { openSftpPanelDir } from "./sftp/panel";
import { readRemoteConfig, findHostByOrigin } from "./settings/remote-section";
import { activityLightClass, type GridSessionSnapshot, type SessionPeek } from "./session-status";
import { contextPercent } from "./views/pricing";

/**
 * Tab 生命周期：
 * - `live`：session 进程还在跑（`~/.claude/sessions/<PID>.json` 存在且 PID 探活通过）
 * - `archived`：session 进程退出，Tab 灰显但保留内容；用户可主动关
 *
 * 历史：设计文档原本规划过 `idle`（5min 无消息变灰），但实际未落地，
 * 移除以免误用。
 */
export type TabStatus = "live" | "archived";

export interface Tab {
  sessionId: string;
  /** Batch7-F24：会话类型（"interactive"/"bg"/null=未知视为交互）。bg → ⚙ 标题 + 树状挂宿主后。 */
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
   * issue #15：数据来源主机标签。null = 本地（标题无前缀）；非空（如 "raspberrypi.local"）
   * = 远端 SSH 主机名，标题加 `[origin]` 前缀以区分本地/远端。首条 line 帧的 origin
   * 决定，之后不变（同一 sid 只来自一个来源）。
   */
  origin: string | null;
  status: TabStatus;
  /**
   * issue #23：红绿灯（与 TabStatus 正交，不碰 archived 门控）。null=未知（旧版 CC
   * 无 status 字段 / 远端 v1 暂无透传）→ 维持现状绿点。
   */
  activity: { status: string; waitingFor: string | null } | null;
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
   * 按 seq 去重集合。一个 Tab == 一个 jsonl path == 一个 seq 空间（本地 watcher 的
   * per-path seqs / 远端 daemon 的 per-process SeqCounter）。SSH 重连后新 daemon 会从
   * seq 0 重发整个会话 → 命中即丢，避免 Tab 内容翻倍。本地 seq 全程唯一 → 永不命中（no-op）。
   *
   * 注意：本集合**只防同 seq 重投**。本地 watcher 截断重读是**换新 seq** 重投整个
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
   * F40b R-1:批期「窗口内中部插入」缓冲——大增量批(>600 行切块,末块先发)落在
   * 已渲染 tab 上时,老块 seq≥floor 但 <timeline.maxSeq,逐条挂 DOM 会跨帧上方
   * 插入(§21 病根)。缓冲到 onBatchEnd 一次 batchInsert 挂载。
   */
  midBatchBuffer: JsonlLinePayload[];
  /** F40b:上翻补批 scroll listener 引线(closeTab 摘) */
  fillHandler: (() => void) | null;
  /**
   * issue #26：已处理记录的 uuid 集——onLine 入口的 at-least-once 幂等
   * （违反此约束见 doc/INVARIANTS.md § 25）。截断重读换新 seq 重投时 seenSeqs 放行，
   * 若不按 uuid 拒掉，每条记录会以更大的 seq 在 timeline 末尾再渲染一遍（整段内容
   * 翻倍），且 trackAgents/unread 等副作用也会被重投误触发——故在入口整体拒掉。
   * 无 uuid 的记录（ai-title/mode 等元信息）不占集合、照常处理（它们本身幂等；
   * 已知微小残留：无 uuid 的 system 细条理论上可翻倍，影响面可忽略）。closeTab 时 clear。
   */
  processedUuids: Set<string>;
}

/** Tab 数量摘要，发给宿主用于状态栏 / empty-state 等外部 UI */
export interface TabsSummary {
  total: number;
  live: number;
  archived: number;
}

/** TabButton 的 DOM 引用：refreshTabBar 局部更新依赖这些 ref 避免重新创建 button */
interface TabButtonRefs {
  root: HTMLButtonElement;
  label: HTMLSpanElement;
  badge: HTMLSpanElement;
  /** A3：账号徽章（该会话属于哪个账号；本地会话不显示，未知显 —）。 */
  acctBadge: HTMLSpanElement;
  /** account-ux U6：⇄ 换号对齐按钮（仅活跃 && 账号≠当前工作账号时显，点=用当前账号重启对齐）。 */
  alignBtn: HTMLSpanElement;
  cwdBtn: HTMLSpanElement;
}

/** F51：远端 tmux 会话(`list_remote_tmux` 后端返回,反查 attach 用)。 */
interface TmuxSession {
  name: string;
  path: string;
  command: string;
  attached: boolean;
  windows: number;
  /** F74：`@ccm_sid` user option——此 tmux 当前所跑 CC 会话 sid（`__ccm_rbind` 写，随 /branch
   * 漂移实时更新）。未设置（老会话 / 未装 wrapper）→ null。用它精确认「哪个 tmux 跑目标 sid」，
   * 取代按 cwd 取第一个（同目录多 claude 会撞错会话）。 */
  sid: string | null;
}
/** tmux 反查缓存 TTL:菜单打开按需查,短缓存避免重复右键狂拉 ssh。 */
const TMUX_CACHE_TTL_MS = 8000;
/** F51：tmux 前台命令是否算 claude 会话。真机 tmux 多报 `claude`(调研 03 §2c 实测),
 * 但视启动路径也可能报解释器 `node`(claude 是 Node CLI)——两者都认,叠加 cwd 精确匹配
 * 收窄误配(D-正确性 Sug2:只认 claude 会在报 node 的环境静默失效)。 */
function isClaudeTmuxCommand(cmd: string): boolean {
  return AGENT_PROFILE.livenessProcessNames.has(cmd);
}

/**
 * F74：在 tmux 会话列表里定位「正跑目标 sid 的活 claude」。**优先 `@ccm_sid` 精确匹配**——
 * 同目录多个 claude tmux（原会话 + `/branch` 出来的分支…）只有 `@ccm_sid` 能分清哪个是目标
 * 会话，且不被漂移骗（`__ccm_rbind` 随 /branch 实时更新它）。
 *
 * 精确没命中时：**只有当整张列表没有任何会话带 `@ccm_sid`**（老 wrapper / 未装）才回退按
 * `path===cwd` 猜（向后兼容）。只要有会话带了 sid、却没一个等于目标 sid，就说明目标会话不在
 * 任何 tmux 里（已结束 / 已漂移到别的 sid）——此时**绝不**按 cwd 抓一个同目录的别的 claude
 * （那正是撞错会话的老 bug），宁可返 undefined（SS-5/SS-9：找不到就报「不在」，不静默换一个）。
 * 契约与铁律见 doc/INVARIANTS.md §30。
 */
export function findClaudeTmux(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
  cwd: string,
): TmuxSession | undefined {
  const byS = sessions?.find((s) => s.sid === sid && isClaudeTmuxCommand(s.command));
  if (byS) return byS;
  const anySidKnown = sessions?.some((s) => s.sid != null);
  if (anySidKnown) return undefined;
  return cwd
    ? sessions?.find((s) => s.path === cwd && isClaudeTmuxCommand(s.command))
    : undefined;
}

/**
 * audit-fixes F03（idle-tmux）：找目标 sid 的**空 tmux**——`@ccm_sid` 精确命中该 sid、但当前
 * 前台命令**不是** claude（交互 shell，claude 已退出）。即三态里的 idle-tmux：会话还在、可 attach/
 * 就地 resume，但没在跑 claude。**只按 @ccm_sid 精确命中**（绝不按 cwd 猜，免撞同目录别的会话）。
 * F03.1 的就地复用 resume 与 F03.3 的 attach-into-idle 共用此判据（与 `findClaudeTmux` 互斥：
 * 后者要 command=claude，本函数要 command≠claude）。纯函数（node/jsdom 可测）。
 */
export function findIdleTmux(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
): TmuxSession | undefined {
  return sessions?.find((s) => s.sid === sid && !isClaudeTmuxCommand(s.command));
}

/**
 * audit-fixes F05：前端版「本工具 tmux 会话名」判定（镜像后端 `tmux.rs::is_ccm_tmux_name`：
 * `cc-` 前缀 + 长度 >3 + 只含 `[A-Za-z0-9_-]`）。清理孤儿只认 `cc-*`——`<project>_cc`（cc-bus 资产）
 * 天然排除，且与 F02 的 kill 白名单同一判据（kill 会拒非 cc-*）。纯函数（node/jsdom 可测）。
 */
export function isCcmTmuxName(name: string): boolean {
  return name.startsWith("cc-") && name.length > 3 && /^[A-Za-z0-9_-]+$/.test(name);
}

/**
 * audit-fixes F05：**真孤儿** = 本工具命名（`cc-*`）、带 `@ccm_sid` 身份、但该 sid **无对应活 tab**
 * 的 tmux 会话（tab 已关但 tmux 会话残留：关了 tab 的 `cc-<sid8>` / resume 撞名产的 `cc-<sid8>-N`）。
 * **保守**：只认**带 @ccm_sid 的**（身份确凿，绝不误杀有 tab 的会话或用户别的会话）；无标记的 cc-*
 * 不误判为孤儿。`<project>_cc`（cc-bus）经 `isCcmTmuxName` 天然排除。纯函数（node/jsdom 可测）。
 */
export function findOrphanTmux(
  sessions: TmuxSession[] | null | undefined,
  tabSids: ReadonlySet<string>,
): TmuxSession[] {
  return (sessions ?? []).filter(
    (s) => isCcmTmuxName(s.name) && s.sid != null && !tabSids.has(s.sid),
  );
}

/**
 * F74c(#60-B)：`findClaudeTmux` 对给定 sid 是否会走 **cwd 回退**（= 无精确 `@ccm_sid` 命中
 * **且**整张列表都无任何会话带 sid）。回退命中的会话是「同目录里的某个 claude」，可能不是目标
 * 会话——未装 / 老 `ccm` wrapper 的向后兼容路径。用户 2026-07-17 拍板：保留回退但**命中时显式提示**
 * （attach 那一刻 toast，别静默串味）。纯函数（node/jsdom 可测），判据与 `findClaudeTmux` 回退分支对齐。
 */
export function isCwdFallbackMatch(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
): boolean {
  const exact = sessions?.some((s) => s.sid === sid && isClaudeTmuxCommand(s.command));
  if (exact) return false;
  const anySidKnown = sessions?.some((s) => s.sid != null);
  return !anySidKnown; // 无精确命中 + 无任一 sid → findClaudeTmux 会走 cwd 回退
}

/**
 * A5+ 优雅退出检测：目标 sid 的 claude 是否已**不在**（本工具）tmux 里精确命中——前台回到 shell
 * （CC 退出）或会话已没。判据与破坏性重启的守卫 `!live || live.sid !== sid` 完全一致：不再精确命中
 * = 已退出。**注**：`sessions == null`（list 失败）时也返回 true，故轮询方（`awaitExitFor`）**只在
 * list 成功时**调用它，list 失败当「未知」继续轮询、不误判成已退出。纯函数（node/jsdom 可测）。
 */
export function claudeExited(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
  cwd: string,
): boolean {
  const live = findClaudeTmux(sessions, sid, cwd);
  return !live || live.sid !== sid;
}

/** F74c(#60-B)：cwd 回退串味风险提示（attach 到可能是同目录别的会话前）。 */
function warnCwdFallbackAttach(): void {
  showActionFailureToast(
    "未检测到会话身份标记",
    "该 tmux 会话没有 @ccm_sid 标记，可能连到同目录里其它会话；建议在远端重装 ccm 助手以精确匹配。",
    { level: "info", durationMs: 8000 },
  );
}

export class TabManager {
  private tabs = new Map<string, Tab>();
  /** F51：per-origin tmux 会话短缓存(反查 attach)。null=该 origin 无 tmux。 */
  private tmuxCache = new Map<string, { ts: number; sessions: TmuxSession[] | null }>();
  /** 按插入顺序的 sessionId 数组，与 this.tabs.keys() 顺序一致但避免每次 Array.from */
  private orderedIds: string[] = [];
  /** sessionId → button DOM refs，避免 refreshTabBar 每次重建整个 bar */
  private tabButtons = new Map<string, TabButtonRefs>();
  /** A3：远端 live 探测的会话账号归属（sid → 探测行）。main.ts 定期喂。 */
  private sessionAccountsByS = new Map<string, SessionAccount>();
  /** A3：账号名 → 邮箱（徽章 tooltip 用）。 */
  private accountEmailByName = new Map<string, string>();
  /** A4：sid → lastAccount（history-metadata）。徽章源②：live 探测不到时兜底。main.ts 定期喂。 */
  private accountLastByS = new Map<string, string>();
  /** A4/§7：账号可查询的远端 origin 集（available 且非 daemonless）。只有这些 origin 的会话才显徽章。 */
  private accountReadyOrigins = new Set<string>();
  /** account-ux U5：origin → 当前工作账号名。徽章「信息才显」比对：会话账号==它 → 不挂徽章。main.ts 定期喂。
   *  **只放 isSelectable 的账号**（main.ts 侧过滤）：不可选的当前账号对齐必失败，指着它说"你不一致"
   *  是假信息，且与 U1 `resolveFollowAccount`「不可选就下沉」的语义保持一致。 */
  private currentByOrigin = new Map<string, string>();
  /** account-ux U6：正在换号重启中的 sid（防同一会话并发重启：新起的进程被后一条编排杀掉）。 */
  private restartingSids = new Set<string>();
  /** account-ux U6：批量对齐进行中（防重入：⚠k 要等下一拍轮询才降，用户很容易再点一次）。 */
  private aligningBatch = false;
  /** A5：换号重启时「等旧号 compact 完成」的 per-sid 回调。onLine 见该 sid 的 compact 摘要行即 resolve。 */
  private compactWaiters = new Map<string, () => void>();
  private activeId: string | null = null;
  /**
   * v2.2 (issue #12): 当前是否在 batch 模式（启动重放 jsonl-batch 期间）。
   * batch 模式中 ensureTab 创建的新 Tab 也要把 BranchFolder 设成 batch。
   *
   * P5.2 B 重构：inPrependMode / pendingPrependFragment / source flag 全删 —— 前端
   * 改用 RecordTimeline 按 seq binary-insert，DOM 位置由 seq 决定不受 emit 顺序影响。
   * 仍保留 inBatch 是因为它控两件事：(1) lazy hljs 注册 (2) BranchFolder.batchMode。
   */
  private inBatch = false;
  /**
   * issue #11: 每个 sid 当前 task 列表（由 ensureTab 拉初次快照 + task-update 事件
   * 更新）。切 Tab 时把对应 sid 的快照喂给全局 TasksPanel。
   */
  private tasksBySid = new Map<string, TaskEntry[]>();
  /**
   * issue #19：归档信号（session-ended）可能早于 replay 把该 sid 的 Tab 建出来。
   * archiveTab 时若 Tab 还不存在，记进这里；ensureTab 建 Tab 时回查、落实归档。
   *
   * issue #20 后 session-ended 已改进 events.ts 的 queue 与行同序处理（否则补发
   * 归档会被后续 drain 的远端行 un-archive 吃掉），正常路径下 ended 不会再早于
   * 行到达——本集合降级为防御层（§ 17a 双层防御），保留兜“ended 先于该 sid 任何
   * 行”的异常序。
   */
  private pendingArchive = new Set<string>();
  /**
   * issue #23：红绿灯信号早于 Tab 建出来时暂存（同 pendingArchive 的时序竞争模式：
   * session-activity 同步派发，而建 Tab 的行走异步 queue/drain）。ensureTab 时落实。
   */
  private pendingActivity = new Map<
    string,
    { status: string; waitingFor: string | null }
  >();
  /**
   * v2.4 issue #2：用户在终端真敲键 → 自动切到对应 Tab 的开关。
   * 默认 true，从 config.json (autoFollowUserActive) 加载。
   */
  private autoFollowUserActive: boolean = true;
  /**
   * v2.4 issue #2：自动切 tab 时是否同时把 monitor 窗口拉前台。默认 false。
   */
  private bringMonitorToFront: boolean = false;
  /**
   * v2.4 issue #2：用户**手动**点 Tab Bar / Ctrl+Tab 后 5s 内拒绝任何 user-active
   * 自动切。表示"我现在主动在看另一个 tab，请别抢回去"。
   *
   * 跟 v1 早期 user-lock 的区别：v1 阻塞 OS focus 检测（已废），v2.4 阻塞
   * watcher 反推的 type=user 信号；信号语义不同，5s 经验值复用合理。
   *
   * 0 = 没有 override 中。每次 manual switchTo 时更新为 now+5000。
   */
  private manualOverrideUntil: number = 0;
  /** Manual override 窗口长度（ms）。issue #2 钦定 5s。 */
  private static readonly MANUAL_OVERRIDE_MS = 5000;
  /** Batch13-F40a:物化/后台 tab 尾段条数(与 F39 viewer TAIL_INITIAL 同语义) */
  private static readonly MATERIALIZE_TAIL_K = 150;
  /** F40b:上翻补批批量/触发距离(沿用 F39 实测值) */
  private static readonly FILL_BATCH = 200;
  private static readonly TOP_TRIGGER_PX = 800;
  /** F40b:补批防重入(补偿测量期间嵌套触发会算错差值) */
  private renderingFill = false;

  /**
   * Tab 撕离（tear-off）拖拽状态机。同一时刻只允许一个拖拽，整段存这里。
   * - mousedown（左键，非子动作按钮）记录起点 → 候选拖拽（dragging=false）
   * - document mousemove 越过 6px 阈值 → dragging=true，建 ghost、源 Tab 变暗
   * - 指针拖离 tab 栏右缘（clientX > barRight + 16，F33 竖栏后为横向判定）→ armed=true（松手即弹窗）
   * - document mouseup：armed → openInNewWindow(落点)；否则取消。两种情况都抑制后续 click
   * null = 当前无拖拽。
   */
  private drag: {
    sid: string;
    startX: number;
    startY: number;
    barRight: number;
    root: HTMLElement;
    dragging: boolean;
    armed: boolean;
    ghost: HTMLElement | null;
    onMove: (e: MouseEvent) => void;
    onUp: (e: MouseEvent) => void;
  } | null = null;
  /**
   * 拖拽撕离阈值：指针移动超过此像素才判定为"拖"，否则视为普通点击。
   */
  private static readonly DRAG_THRESHOLD_PX = 6;
  /**
   * 拖拽结束后需抑制掉紧随 mouseup 的那次 click 的 sid（避免拖完又误切 Tab）。
   * null = 不抑制。click handler 命中后清零（一次性）。
   */
  private suppressClickSid: string | null = null;

  constructor(
    private barEl: HTMLElement,
    private streamRootEl: HTMLElement,
    /** 任何 Tab 增/减/状态变化后回调；宿主用它驱动状态栏等外部 UI */
    private onTabsChanged?: (summary: TabsSummary) => void,
    /** issue #11: 全局 TasksPanel，切 Tab / 收事件时由 TabManager 喂数据 */
    private tasksPanel?: TasksPanel,
    /** issue #23: 全局 AgentsPanel（subagent 列表 + 各自状态灯），喂数方式同 tasksPanel */
    private agentsPanel?: AgentsPanel,
  ) {}

  private notifyChanged(): void {
    if (!this.onTabsChanged) return;
    let live = 0;
    let archived = 0;
    for (const t of this.tabs.values()) {
      if (t.status === "archived") archived += 1;
      else live += 1;
    }
    this.onTabsChanged({ total: this.tabs.size, live, archived });
  }

  /**
   * v2.2 (issue #12): 启动重放（jsonl-batch）开始时调一次。所有现有 Tab 的
   * BranchFolder 切到 batch 模式 —— 后续 recordAdded 只 push 不算 mainBranch。
   * 重放期 ensureTab 新创建的 Tab 也会自动进 batch（看 this.inBatch）。
   *
   * P5.2 B 重构：删了 inPrependMode / pendingToolGroup 清零 —— 前端用 timeline
   * 按 seq 排序，tool-group 合并改后处理（看左邻居），不再需要 chunk 边界协调。
   */
  onBatchStart(): void {
    this.inBatch = true;
    // P5.5 B 重构：lazy 通过 ctx.lazy 传到 renderMarkdown —— onLine 构造 ctx 时
    // 用 this.inBatch 设置。不再依赖 setRenderLazyMode 全局开关。
    for (const t of this.tabs.values()) {
      t.branchFolder.setBatchMode(true);
      // Batch13-F40a:deferMode 已退役——重放期旧记录根本不建卡(收纳进 tab.window),
      // "视口上方插入"次数为 0,比"延后到一帧"更强(INVARIANTS §21.3)。
    }
  }

  /**
   * v2.2 (issue #12): 重放批次完结。各 Tab 调 flushPending 一次性算 + rebuild，
   * 然后切回 live 模式。后续真实时新消息按 timeline 路径走。
   */
  onBatchEnd(): void {
    this.inBatch = false;
    for (const t of this.tabs.values()) {
      // F40b R-1:先把批期缓冲的中部插入一次挂载(内含 unwrapAll/rebuildNow),
      // 再走既有 flushPending/reconcile
      this.flushMidBatchBuffer(t);
      t.branchFolder.flushPending();
      t.branchFolder.setBatchMode(false);
      // 切块场景下，老块的 tool_use 现在已渲染 → 重试匹配早到的 fallback result
      const ctx: RenderContext = {
        parentPath: t.parentPath,
        origin: t.origin,
        toolUseNames: t.toolUseNames,
        toolUseElements: t.toolUseElements,
        pendingToolResults: t.pendingToolResults,
      };
      // S-6:孤儿卡出 DOM 的同时出账,防悬空 anchor
      for (const el of reconcilePendingToolResults(ctx)) t.timeline.removeByElement(el);
    }
    // Batch13-F40a:active tab 不足一屏(或还是 virgin)→ 立即补物化到可见;
    // 其余 virgin 后台 tab 进空闲物化队列(逐个串行,避免并发建卡风暴)。
    const active = this.activeId !== null ? this.tabs.get(this.activeId) : undefined;
    if (active && active.window.pendingCount > 0) {
      const el = active.streamEl;
      const notScrollable = el.scrollHeight - el.clientHeight <= 1;
      if (active.window.floorSeq === null || notScrollable) {
        this.materializeUntilFilled(active);
        active.stream.scrollToBottom();
      }
    }
    // D 审计 S-5:archived 死会话不进后台物化队列(纯浪费;switchTo 命中 virgin
    // 已有同步物化兜底)。
    this.materializeQueue = [...this.tabs.entries()]
      .filter(
        ([sid, t]) =>
          sid !== this.activeId &&
          t.status !== "archived" &&
          t.window.floorSeq === null &&
          t.window.pendingCount > 0,
      )
      .map(([sid]) => sid);
    this.scheduleIdleMaterialize();
    // F40b:active tab 未走物化分支(尾块已可滚)时也要挂哨兵
    if (active) this.updateSentinel(active);
    // F88b：批期 trackUsage 只更了 tab 字段没喂 chip，这里对活跃 tab 单次 flush 到 HUD
    // （批内多条 assistant 记录只刷一次，消视觉抖动）。
    this.onActiveUsageChanged?.(active?.latestModel ?? null, active?.latestPromptTokens ?? null);
  }

  /**
   * D 审计 R-3:一次 150 条 payload 可能只产出几张卡(tool-group 合并成单卡 34px、
   * skip 记录占配额不产卡)——工具密集会话一轮物化后屏幕仍近空,而 F40a 没有上翻
   * 补批兜底。有界循环补到可滚动或账本弹尽(≤4 轮防病态会话空转)。
   */
  private materializeUntilFilled(tab: Tab): void {
    for (let round = 0; round < 4; round++) {
      if (tab.window.pendingCount === 0) return;
      const el = tab.streamEl;
      if (round > 0 && el.scrollHeight - el.clientHeight > 1) return;
      this.materializeTail(tab);
    }
  }

  /**
   * Batch13-F40a/b:批量渲染内核(物化 / 上翻补批 / R-1 缓冲 flush 共用)。
   * - sink 不接 onRealUserInput(历史 user 卡不得触发自动切 tab)、branch/queue/title
   *   为 no-op(收纳/缓冲时 routeMetaAndBranch 已喂过,BranchFolder.seenUuids 双保险);
   * - 不计 unread(重放历史不是"未读新消息",修 backlog S-2;R-1 缓冲的 unread
   *   在缓冲时已计);
   * - lazy hljs + observe(批量渲染的是历史内容,滚入视口再高亮);
   * - 插卡前 unwrapAll 摊平(邻居可能在 fold wrap 内,F39 实证的不变量)、批内
   *   暂停逐卡 snap(S-7,防 150 次强制 reflow)、插完 reconcile 孤儿 tool_result
   *   (S-6 同步出账)、rebuildNow 无条件重折(flushPending 的 setsEqual 短路会把
   *   摊平永久化)。
   */
  private renderPayloadsBatch(tab: Tab, payloads: JsonlLinePayload[]): void {
    if (payloads.length === 0) return;
    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      origin: tab.origin,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      pendingToolResults: tab.pendingToolResults,
      lazy: true,
    };
    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: () => {},
      onQueueOperation: () => {},
      observeForLazyEnhance: true,
    };
    tab.branchFolder.unwrapAll();
    tab.stream.batchInsert(() => {
      for (const p of payloads) {
        try {
          renderContentRecord(p, ctx, sink);
        } catch (e) {
          console.error("[tabs] 批量渲染单条失败,跳过:", p.seq, e);
        }
      }
    });
    for (const el of reconcilePendingToolResults(ctx)) tab.timeline.removeByElement(el);
    tab.branchFolder.rebuildNow();
  }

  /**
   * Batch13-F40a:物化 tab 的尾段——从窗口账本弹出 seq 最高的 ≤k 条建卡。
   * 物化目标是 virgin/近 virgin tab(无滚动位置可保),不需要滚动补偿——上翻补批
   * 的手动补偿在 fillAbove(F40b)。
   */
  private materializeTail(tab: Tab, k = TabManager.MATERIALIZE_TAIL_K): void {
    this.renderPayloadsBatch(tab, tab.window.takeTail(k));
    this.updateSentinel(tab);
  }

  /**
   * F40b R-1:批期缓冲的「窗口内中部插入」(大增量批老块)一次性挂载。
   * 排序后走渲染内核(含 unwrapAll/rebuildNow——不能依赖随后 flushPending,
   * 它的 setsEqual 短路会把摊平永久化)。在 onBatchEnd 的 flushPending/reconcile
   * 之前调。
   */
  private flushMidBatchBuffer(tab: Tab): void {
    if (tab.midBatchBuffer.length === 0) return;
    const payloads = tab.midBatchBuffer.sort((a, b) => a.seq - b.seq);
    tab.midBatchBuffer = [];
    // 已知取舍(D 审计):批末 flush 不做选区守卫——unwrap/rebuild 会杀进行中选区,
    // 但 flush 不可延迟(数据必须落),且触发面(选中文本时恰逢远端大增量批)极窄。
    this.renderPayloadsBatch(tab, payloads);
    this.refreshTabBar(); // 缓冲期攒下的 unread 徽标一次刷新
  }

  /**
   * F40b:上翻补批。守卫:防重入 / 账空 / 选区进行中(补批 unwrap/rebuild 会杀
   * 进行中的选区,等下次 scroll 再试)。补偿:临时关原生锚定(防 WebView2 与手动
   * 补偿 double-shift;WebKitGTK 本就无锚定),测量→渲染→scrollTop 回写在同一
   * 同步任务内(不许 await/rAF 打断)。rAF 自链:零高批/不足一屏无 scroll 事件
   * (F39-R1 场景),补完复检直到离开触发区或账尽。
   */
  private fillAbove(tab: Tab): void {
    if (this.renderingFill) return;
    if (tab.window.pendingCount === 0) return;
    const sel = document.getSelection();
    if (sel && !sel.isCollapsed) return;
    const el = tab.streamEl;
    this.renderingFill = true;
    try {
      el.style.overflowAnchor = "none";
      const beforeH = el.scrollHeight;
      const beforeTop = el.scrollTop;
      this.renderPayloadsBatch(tab, tab.window.takeTail(TabManager.FILL_BATCH));
      // 哨兵刷新必须在补偿回写**之前**:账尽移除的 ±30px 计入 Δ 一并吃掉——
      // 移除若在补偿后,dev(无锚定)会在"会话第一条"处一次性跳 30px(D 审计)。
      this.updateSentinel(tab);
      el.scrollTop = beforeTop + (el.scrollHeight - beforeH);
    } finally {
      // 还原必须在 finally:渲染内核抛出时留下 overflow-anchor:none = 该 tab 永久
      // 失去原生锚定,违反 §21.2 且无自愈(D 审计,两家共识)
      el.style.overflowAnchor = "";
      this.renderingFill = false;
    }
    requestAnimationFrame(() => {
      if (this.activeId !== tab.sessionId) return;
      const t = this.tabs.get(tab.sessionId);
      if (!t || t.window.pendingCount === 0) return;
      const e = t.streamEl;
      if (e.scrollTop <= TabManager.TOP_TRIGGER_PX || e.scrollHeight - e.clientHeight <= 1) {
        this.fillAbove(t);
      }
    });
  }

  /**
   * F40c DEV 探针用:active tab 状态一行 JSON——无 devtools 环境下 E2E 断言的
   * 唯一出口(经 e2e-probe 热键 → fe_perf 日志)。生产不接线,方法本身无副作用。
   */
  debugSnapshot(): string {
    const tab = this.activeId !== null ? this.tabs.get(this.activeId) : undefined;
    if (!tab) return JSON.stringify({ active: null });
    const el = tab.streamEl;
    const statusText = document.getElementById("status-bar")?.textContent ?? "";
    return JSON.stringify({
      sid: tab.sessionId.slice(0, 8),
      scrollTop: Math.round(el.scrollTop),
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
      distBottom: Math.round(el.scrollHeight - el.scrollTop - el.clientHeight),
      pending: tab.window.pendingCount,
      midBuffer: tab.midBatchBuffer.length,
      timeline: tab.timeline.size,
      foldWraps: tab.stream.contentElement.querySelectorAll(":scope > .branch-fold-wrap").length,
      sentinel:
        tab.stream.contentElement.querySelector(":scope > .stream-more-above")?.textContent ??
        null,
      err: statusText.startsWith("ERR") || statusText.startsWith("REJ") ? statusText : null,
    });
  }

  /**
   * F40b:顶端哨兵——账本非空时置顶「还有 N 条更早消息」,账尽移除。
   * 非 timeline 实体、无 data-uuid(BranchFolder 视作断 run,天然免疫 fold);
   * 二分插入的 anchor 恒为 timeline 元素,最老卡 insertBefore(首卡) 自然落哨兵后。
   */
  private updateSentinel(tab: Tab): void {
    const content = tab.stream.contentElement;
    let el = content.querySelector(":scope > .stream-more-above") as HTMLElement | null;
    const n = tab.window.pendingCount;
    if (n === 0) {
      el?.remove();
      return;
    }
    if (!el) {
      el = document.createElement("div");
      el.className = "stream-more-above";
      content.prepend(el);
    }
    el.textContent = `↑ 还有 ${n} 条更早消息 · 上翻加载`;
  }

  /** F40a:virgin 后台 tab 的空闲物化队列(串行;rIC 缺失时 setTimeout 兜底) */
  private materializeQueue: string[] = [];
  private materializeScheduled = false;

  private scheduleIdleMaterialize(): void {
    if (this.materializeScheduled) return;
    const sid = this.materializeQueue.shift();
    if (sid === undefined) return;
    this.materializeScheduled = true;
    const run = (): void => {
      this.materializeScheduled = false;
      const tab = this.tabs.get(sid);
      // 只物化仍是 virgin 的(switchTo 可能已同步物化过);二次 batch 开始则原样跳过,
      // 账本继续收纳,批结束会重新排队。
      if (tab && !this.inBatch && tab.window.floorSeq === null) {
        this.materializeTail(tab);
      }
      this.scheduleIdleMaterialize();
    };
    if (typeof window.requestIdleCallback === "function") {
      window.requestIdleCallback(run, { timeout: 2000 });
    } else {
      window.setTimeout(run, 200);
    }
  }

  /**
   * 收到一行 JSONL 时调用。
   *
   * P5.2 B 重构：路由到 renderStreamRecord（三 caller 共享管线）。
   * - timeline.insert 按 seq 决定 DOM 位置（不再有 source/inPrependMode 区分）
   * - tool-group 后处理合并基于 timeline 左邻居
   * - ai-title 走 sink.onTitleUpdate
   * - 真用户输入走 sink.onRealUserInput → this.userActive
   */
  onLine(payload: JsonlLinePayload): void {
    const tab = this.ensureTab(
      payload.session_id,
      payload.cwd,
      payload.path,
      payload.seq,
      payload.origin ?? null,
    );

    // SSH 重连后远端 daemon 从 seq 0 重发该 session 整段 jsonl → 按 seq 去重。必须在
    // renderStreamRecord 之前、且覆盖 skip 记录（attachment/isMeta/空 user 有 seq 但不入
    // timeline，timeline.has 漏判）。本地 seq 全程唯一 → 此 set 永不命中（本地 no-op）。
    if (tab.seenSeqs.has(payload.seq)) return;
    tab.seenSeqs.add(payload.seq);

    // issue #26：按 uuid 去重——截断重读换新 seq 重投时上面的 seq 去重放行，这里把
    // "同一记录再来一遍"整体拒掉（不渲染、不 trackAgents），否则内容在 timeline 末尾
    // 翻倍（INVARIANTS § 25 的渲染层履约点）。必须放在 ensureTab 之后（远端
    // un-archive 靠"收到行"翻转，重投行也要触发它）、seq 去重之后。
    const uuid = (payload.message as { uuid?: unknown }).uuid;
    if (typeof uuid === "string" && uuid.length > 0) {
      if (tab.processedUuids.has(uuid)) return;
      tab.processedUuids.add(uuid);
    }

    // A5：换号重启的 compact 完成检测。仅当有该 sid 的等待者才判（常态零开销）：见 compact 摘要
    // 行即 resolve 该等待者（换号重启编排随即从 compact 步进入 kill 步）。
    if (this.compactWaiters.size > 0) {
      const waiter = this.compactWaiters.get(payload.session_id);
      if (waiter && isCompactRecord(payload.message)) {
        this.compactWaiters.delete(payload.session_id);
        waiter();
      }
    }

    // issue #63①：首条带 forkedFrom 的记录 → 锁定血缘、给 tab 标题加 `↳` 徽标(与原会话区分)。
    // 放在双重去重之后、turnEndNotifier 之前——这样首条即含徽标的标题也进 turn-end 通知(审计 建议)。
    this.applyForkedFrom(tab, payload.message);

    // Batch14-F42：turn-end 系统通知。放在双重去重之后（重投行不重报）、
    // 渲染管线之前（通知与渲染/收纳互相独立）。批量重放由 inBatch 短路。
    turnEndNotifier.observe(payload.session_id, tab.title, payload, this.inBatch);

    // issue #23（第二增量）：配对 agent 工具调用，喂 AgentsPanel
    this.trackAgents(tab, payload.message);

    // F88b：捕获带 usage 的 assistant 记录 → 更新本会话最新 prompt token+model（供 HUD context%）。
    // 与 trackAgents 同处（双重去重之后，重投不重复累）。活跃会话则即时推给 HUD。
    this.trackUsage(tab, payload.message, payload.seq);

    // F70：累进本会话改动集（写类工具 file_path）——放在双重去重之后（重投不重复累），
    // 渲染/收纳门控之前（连"收纳不建卡"的记录也计入）。纯增量、无 DOM。
    // F91b-fix(batch18 审计修)：re-touch 时 delete+add 把它移到末尾 = **近因序**，让 F91b peek 的
    // slice(-8) 显「最近改的 8 个」（原 Set 只记首触序，此刻正猛改的老文件被埋）。Set 成员/size 不变，
    // F70 全景高亮按成员判定、与序无关，安全。O(1)/文件。
    for (const f of collectEditedFiles(payload.message)) {
      tab.touchedFiles.delete(f);
      tab.touchedFiles.add(f);
    }

    const sink: StreamSink = {
      timeline: tab.timeline,
      onBranchRecord: (rec: BranchRecord) => tab.branchFolder.recordAdded(rec),
      // issue #36：队列消息内容 → 折叠豁免集合
      onQueueOperation: (content: string) => tab.branchFolder.addQueuedContent(content),
      onTitleUpdate: (title: string) => this.applyAiTitle(tab, title),
      onRealUserInput: (sid: string) => this.userActive(sid),
      observeForLazyEnhance: this.inBatch,
    };

    // Batch13-F40a:meta/branch 收集与渲染解耦——收纳(不建卡)的记录也要喂
    // title/queue/branch 数据(routeMetaAndBranch 是两条路径的单一来源,账本 §3)。
    if (routeMetaAndBranch(payload, sink) === "consumed") return;

    // 门控(单洞后缀不变量,纯 seq 判定):
    // - virgin + batch:active tab 首条 content 钉 floor(尾块直渲,进步式首屏
    //   与 deferMode 时代一致);后台 tab 恒收纳(virgin,批后空闲物化)。
    // - virgin + live:新开 tab 直渲并钉 floor。
    // - seq >= floor:渲染(live 追加/尾块);seq < floor:收纳(旧块/F30 尾部
    //   优先回填/迟到块——无论 inBatch,与 batch 哨兵解耦)。
    // D 审计 C-1(近 virgin 竞态):批后 rIC 队列还没轮到该 tab 就来了真 live 行——
    // 若直接 pinFloor(新行 seq),账本里整段历史会被钉死滞留(F40a 无再物化入口)。
    // 先物化尾段(takeTail 顺带钉 floor),live 行(seq 恒更新)再照常 admit。
    if (tab.window.floorSeq === null && !this.inBatch && tab.window.pendingCount > 0) {
      this.materializeTail(tab);
    }
    const floor = tab.window.floorSeq;
    let render: boolean;
    if (floor === null) {
      render = !this.inBatch || this.activeId === tab.sessionId;
      if (render) tab.window.pinFloor(payload.seq);
    } else {
      render = tab.window.admit(payload.seq);
    }
    // F40b R-1:批期落在渲染窗口内的**中部**插入(seq≥floor 且 <已渲染最高 seq
    // ——大增量批的老块)→ 缓冲,onBatchEnd 一次挂载,消逐帧上方插入(§21)。
    // 离线期真新消息:unread 照计(粒度=记录,与逐条渲染的 inserted 判定在
    // tool-group 合并上略有偏差,99+ 封顶下可接受)。
    if (render && this.inBatch && payload.seq < tab.timeline.maxSeq) {
      tab.midBatchBuffer.push(payload);
      // 徽标刷新攒到批末 flush 一次(D 审计:600 条缓冲 = 600 次全 bar 巡检)
      if (this.activeId !== tab.sessionId) tab.unread += 1;
      return;
    }
    if (!render) {
      tab.window.defer(payload);
      // 仪表(jsdom 单测无 main.ts,须防 undefined)
      if (window.__ccmPerf) window.__ccmPerf.recordsDeferred = (window.__ccmPerf.recordsDeferred ?? 0) + 1;
      // 收纳不计 unread——重放历史不是"未读新消息"(修 backlog S-2)
      return;
    }

    const ctx: RenderContext = {
      parentPath: tab.parentPath,
      origin: tab.origin,
      toolUseNames: tab.toolUseNames,
      toolUseElements: tab.toolUseElements,
      pendingToolResults: tab.pendingToolResults,
      // P5.5：batch 期间走 lazy hljs（代码块占位 + IntersectionObserver 触发再补跑）
      lazy: this.inBatch,
    };
    const beforeSize = tab.timeline.size;
    renderContentRecord(payload, ctx, sink);
    const inserted = tab.timeline.size > beforeSize;

    // unread 计数：只有真新 entry 入 timeline 才算（tool-group 合并到旧 group 不算）
    if (inserted && this.activeId !== tab.sessionId) {
      tab.unread += 1;
      this.refreshTabBar();
    }
  }

  /**
   * Batch5-F18：骨架 Tab——活跃清单（本地 IPC / 远端 session_added 事件）一到
   * 即建，不等首条内容行。复用 ensureTab 全部语义：cwd 以 MAX_SAFE_INTEGER 的
   * seq 记入 → 任何真实行的 cwd（更小 seq）照常覆盖为项目根；parentPath 空由
   * 首条行回填；pendingArchive/pendingActivity 落实、batch 模式继承均沿用。
   * 已存在同 sid Tab 时为 no-op（幂等，重连重发 session_added 无害）。
   */
  /**
   * Batch7-F24 树状排序：bg tab 插到同 (cwd, origin) 交互宿主（及其既有 bg 子项）
   * 之后；无宿主则追加末尾。交互 tab 创建时反向重锚——把已存在的同 (cwd, origin)
   * bg tab 拉到自己身后（骨架清单里 bg 可能先于宿主出现）。父子判定 v1 = cwd
   * 归属（pidfile 无 parentSessionId 字段，精确父子留 backlog）。
   */
  private placeInOrder(tab: Tab): void {
    const isBg = tab.kind !== null && tab.kind !== "interactive";
    const sameHost = (t: Tab | undefined): boolean =>
      !!t && t.cwd !== null && t.cwd === tab.cwd && t.origin === tab.origin;
    if (isBg && tab.cwd) {
      // 找宿主（交互 + 同 cwd/origin）——插到宿主连同其已有 bg 子串之后
      for (let i = 0; i < this.orderedIds.length; i++) {
        const t = this.tabs.get(this.orderedIds[i]);
        if (sameHost(t) && (t!.kind === null || t!.kind === "interactive")) {
          let j = i + 1;
          while (j < this.orderedIds.length) {
            const c = this.tabs.get(this.orderedIds[j]);
            if (sameHost(c) && c!.kind !== null && c!.kind !== "interactive") j++;
            else break;
          }
          this.orderedIds.splice(j, 0, tab.sessionId);
          return;
        }
      }
      this.orderedIds.push(tab.sessionId);
      return;
    }
    // 交互 tab：追加，再把**真孤儿** bg 子项拉到身后（保持原相对序）。
    // 已紧跟在先到宿主（同 cwd/origin 交互 tab）之后的 bg 子串不动——
    // 计划契约"多宿主取第一个"（审计 D-R3：第二个同 cwd 交互会话不许搬走
    // 第一个宿主已挂好的子树）。
    this.orderedIds.push(tab.sessionId);
    if (tab.cwd) {
      const orphans: string[] = [];
      let anchored = false; // 当前扫描位置是否处于"sameHost 宿主的 bg 子串"内
      for (const sid of this.orderedIds) {
        if (sid === tab.sessionId) continue;
        const t = this.tabs.get(sid);
        const isBg = !!t && t.kind !== null && t.kind !== "interactive";
        if (!isBg) {
          anchored = sameHost(t) && (t!.kind === null || t!.kind === "interactive");
          continue;
        }
        if (sameHost(t)) {
          if (!anchored) orphans.push(sid);
          // anchored 保持——宿主的 bg 子串延续
        } else {
          anchored = false; // 异族 bg 打断子串
        }
      }
      if (orphans.length) {
        this.orderedIds = this.orderedIds.filter((sid) => !orphans.includes(sid));
        const at = this.orderedIds.indexOf(tab.sessionId) + 1;
        this.orderedIds.splice(at, 0, ...orphans);
      }
    }
  }

  createSkeletonTab(
    sessionId: string,
    cwd: string | null,
    origin: string | null,
    kind: string | null = null,
    name: string | null = null,
  ): void {
    this.ensureTab(sessionId, cwd, "", Number.MAX_SAFE_INTEGER, origin, kind, name);
  }

  /** Batch5-F19：启动 active 选择用（last-active 是否已有 tab）。 */
  hasTab(sessionId: string): boolean {
    return this.tabs.has(sessionId);
  }

  /**
   * Batch15-P2：活跃 tab 的仓信息（cwd + origin），供全景视图判断索引哪个本地仓。
   * 无活跃 tab / 活跃 tab 无 cwd → 返 null。origin!==null = 远端会话（代码在远端机，
   * 本地 code-picture 索引不到，全景侧据此显式提示不索引）。**additive getter，只读，不改既有逻辑。**
   */
  activeRepoInfo(): { cwd: string; origin: string | null } | null {
    const tab = this.activeId !== null ? this.tabs.get(this.activeId) : undefined;
    if (!tab || !tab.cwd) return null;
    return { cwd: tab.cwd, origin: tab.origin };
  }

  /**
   * F70：某会话在全景图上可高亮的「改动集」——cwd（定仓）+ 它写类工具碰过的文件。
   * **本地会话专属**：origin!==null（远端）/ 无 cwd → 返 null（远端代码不在本机、code-picture
   * 索引不到，高亮不可用——门控就地做，呼应 activeRepoInfo）。**只读 getter，不落盘。**
   */
  touchedFilesFor(
    sid: string,
  ): { cwd: string; origin: null; files: string[] } | null {
    const tab = this.tabs.get(sid);
    if (!tab || !tab.cwd || tab.origin !== null) return null;
    return { cwd: tab.cwd, origin: null, files: [...tab.touchedFiles] };
  }

  /**
   * F91（#27）：跨会话监控快照——`GridMonitorView` 消费的**只读派生 DTO 列表**（本地 + 所有远端会话）。
   * 纯派生：不外泄任何内部 DOM / Map 引用（防外部改到 TabManager 内部状态）。插入序（同 tab-bar）。
   * context% 复用 pricing.ts `contextPercent`（上限未知 / 无 usage → null）。
   */
  /**
   * A3：喂入远端 live 探测的会话账号归属（来自 daemon `--session-accounts`）+ 账号邮箱表。
   * main.ts 定期聚合各远端调用。喂完刷新所有 tab 的账号徽章。
   */
  setSessionAccounts(
    rows: SessionAccount[],
    emailByName: Map<string, string>,
    lastAccountByS: Map<string, string> = new Map(),
    readyOrigins: Set<string> = new Set(),
    currentByOrigin: Map<string, string> = new Map(),
  ): void {
    this.sessionAccountsByS = new Map();
    for (const r of rows) {
      if (r.sessionId) this.sessionAccountsByS.set(r.sessionId, r);
    }
    this.accountEmailByName = emailByName;
    this.accountLastByS = lastAccountByS;
    this.accountReadyOrigins = readyOrigins;
    this.currentByOrigin = currentByOrigin;
    for (const [sid, refs] of this.tabButtons) {
      const tab = this.tabs.get(sid);
      if (tab) this.updateAccountBadge(refs, sid, tab);
    }
  }

  /**
   * account-ux U5「信息才显」：只在会话账号**不在当前工作账号**（或未知当前时不猜）时挂徽章——
   * 一致=不挂（chip 已代言，tab 栏保持干净）；未知(源③)=不挂（退 hover tooltip，消 `—` 墙）；
   * 不一致→彩色头像：source=live 实心（硬真相）/ source=last 幽灵（软来源）。§7 readyOrigins 门控不变。
   */
  private updateAccountBadge(refs: TabButtonRefs, sid: string, tab: Tab): void {
    const hide = (): void => {
      refs.acctBadge.textContent = "";
      refs.acctBadge.className = "tab-acct-badge";
      refs.acctBadge.style.display = "none";
      refs.alignBtn.classList.remove("is-eligible");
    };
    if (!shouldShowAccountBadge(tab.origin, this.accountReadyOrigins)) return hide();
    // U8 休眠**不作用于这里**（D 审计阻塞项）：本徽章是「信息才显」——只有 detectAccountMismatch
    // 为真才渲染，所以它从来不是"单账号时的颜色噪音"，而是**唯一的 per-session 不一致信号**；
    // 只有 1 个可选账号时这条信息同样成立、甚至更要紧。若在此休眠，就会出现 chip 报 ⚠k、
    // Ctrl+K 有对齐命令，而所有 tab 上一个徽章一个 ⇄ 都没有的鬼影。
    // 规则:**颜色可以睡，信息和操作不能睡** —— 休眠只留给 chip 那个常显的身份头像。
    const b = sessionBadge(
      sid,
      tab.origin,
      this.sessionAccountsByS,
      this.accountEmailByName,
      this.accountLastByS,
    );
    if (!b || !b.account) return hide(); // 未知账号（源③）→ 退 hover；顺带把 b.account 窄化为 string
    const current = tab.origin ? this.currentByOrigin.get(tab.origin) ?? null : null;
    // detectAccountMismatch（U1 纯函数）：当前未就绪 / 二者相等 → 均返 false → 不挂徽章
    //（信息才显：未知退 hover、一致靠 chip 代言、当前未就绪不猜）。仅确知不一致才挂。
    if (!detectAccountMismatch(b.account, current)) return hide();
    // 不一致 → 挂彩色头像（live 实心 / lastAccount 幽灵）。
    refs.acctBadge.textContent = "";
    refs.acctBadge.className = "tab-acct-badge";
    refs.acctBadge.appendChild(accountAvatarEl(b.account, { size: 14, ghost: b.source === "last" }));
    // live（活会话）→ ⇄ 一键对齐（重启）；last（死会话/归档）→ 无 ⇄，但 tooltip 得指出路，
    //（D 审计：曾把这句指路删了，幽灵徽章就再无任何地方说明怎么对齐）。
    const alignable = this.alignableCurrent(sid, tab);
    refs.acctBadge.title =
      `${b.tooltip} · 与当前工作账号「${current}」不一致` +
      (alignable ? "" : b.source === "last" ? "（右键「用账号 … resume」可对齐）" : "");
    refs.acctBadge.style.display = "";
    // 够格与否由 JS 打 .is-eligible；**何时露面**（hover）交给 CSS，见 styles.css。
    if (alignable) {
      refs.alignBtn.title = `用当前工作账号「${current}」重启对齐此会话（中断当前回合、丢进程内状态）`;
      refs.alignBtn.setAttribute("aria-label", `用当前工作账号 ${current} 重启对齐此会话`);
      refs.alignBtn.classList.add("is-eligible");
    } else {
      refs.alignBtn.classList.remove("is-eligible");
    }
  }

  snapshotSessions(): GridSessionSnapshot[] {
    const out: GridSessionSnapshot[] = [];
    for (const tab of this.tabs.values()) {
      let running = 0;
      for (const a of tab.agents.values()) {
        if (a.status === "running") running += 1;
      }
      out.push({
        sessionId: tab.sessionId,
        title: tab.title,
        origin: tab.origin,
        cwd: tab.cwd,
        status: tab.status,
        activityStatus: tab.activity?.status ?? null,
        waitingFor: tab.activity?.waitingFor ?? null,
        runningAgents: running,
        totalAgents: tab.agents.size,
        contextPct:
          tab.latestPromptTokens != null
            ? contextPercent(tab.latestModel, tab.latestPromptTokens)
            : null,
        unread: tab.unread,
        kind: tab.kind,
        account: this.sessionAccountsByS.get(tab.sessionId)?.account ?? null,
      });
    }
    return out;
  }

  /**
   * F91b（batch17）：监控板选中 cell 的「内容 peek」补充数据（纯读派生，无写/无落盘）。
   * 只给 `snapshotSessions` 之外的细节：model / 改过的文件 / subagent 名单（运行中优先）。
   * 未知 sid → null（选中会话恰好消失时调用方据此清选中）。
   */
  peekSession(sessionId: string): SessionPeek | null {
    const tab = this.tabs.get(sessionId);
    if (!tab) return null;
    const agents = [...tab.agents.values()]
      .map((a) => ({ label: a.label, status: a.status }))
      .sort((x, y) => (x.status === "running" ? 0 : 1) - (y.status === "running" ? 0 : 1));
    return {
      model: tab.latestModel,
      recentFiles: [...tab.touchedFiles],
      agents,
    };
  }

  /** Batch5-F19：switchTo 是否写回 last-active（viewer/tear-off 窗口置 false）。 */
  persistLastActive = true;

  /** Batch5-F19（G 验收）：用户手动切 tab 时回调——main.ts 用它清 pendingStartupActive，
   *  防迟到的远端宣告补切抢走用户已选的焦点。 */
  onManualSwitch: (() => void) | null = null;

  /** F70：右键「在全景高亮本会话改动」回调——main.ts 注入（TabManager 不直接持有
   *  PanoramaView，走注入回调，同 onManualSwitch 范式）。仅本地会话菜单出现该项。 */
  requestPanoramaHighlight: ((sid: string) => void) | null = null;

  /** F88b：活跃会话最新 usage 变化回调——main.ts 注入喂 UsageHud（context% chip）。
   *  两处触发：onLine 捕到活跃会话新 assistant 记录；switchTo 切到别的会话。
   *  (model=null 或 promptTokens=null → chip 显 `?` 或隐藏)。同 requestPanoramaHighlight 注入范式。 */
  onActiveUsageChanged: ((model: string | null, promptTokens: number | null) => void) | null =
    null;

  ensureTab(
    sessionId: string,
    cwd: string | null,
    sourcePath: string,
    seq: number,
    origin: string | null = null,
    kind: string | null = null,
    bgName: string | null = null,
  ): Tab {
    let tab = this.tabs.get(sessionId);
    if (tab) {
      // SSH 重连：远端会话掉线时被 flush 归档过，现在又收到它的行 = daemon 在重放 = 会话仍
      // 活着 → 复活成 live。必须放在 ensureTab 里（在 onLine 的 seq 去重 return 之前），否则整段
      // 重放全被去重时连第一条行都走不到翻转。**仅远端**：本地归档由 PID 判活驱动，不靠「收到行」
      // 翻转，避免会话退出时尾写把已归档的本地 Tab 误复活（远端掉线归档是连接驱动，无此风险）。
      if (tab.status === "archived" && tab.origin !== null) {
        tab.status = "live";
        this.refreshTabBar();
      }
      // v2.22.2 kind 冲突消解:同一 sid 可能有多份 pidfile(实证:cc-daemon 的
      // bg-spare 备用进程复用**父会话的 sid**写 kind=bg)——宣告到达顺序不定,
      // bg 先到会把真交互会话降格成 ⚙ 且树状挂到别的宿主下(用户截图实锤)。
      // 规则:**interactive 恒压过 bg**——后到的 interactive 宣告在此升格纠正
      // (重新按宿主定位 + 把同 cwd 孤儿 bg 拉回身后);反向(bg 后到)绝不降格。
      if (kind === "interactive" && tab.kind !== null && tab.kind !== "interactive") {
        tab.kind = "interactive";
        tab.bgName = null;
        tab.title = this.computeTitle(tab);
        const i = this.orderedIds.indexOf(sessionId);
        if (i >= 0) this.orderedIds.splice(i, 1);
        this.placeInOrder(tab);
        this.refreshTabBar();
      }
      // Batch5-F18：骨架 Tab（无行创建）的 parentPath 为空——首条带路径的行回填，
      // 保住「在新窗口打开」等依赖 jsonl 路径的功能。
      if (!tab.parentPath && sourcePath) {
        tab.parentPath = sourcePath;
      }
      // cwd 取**最早（最小 seq）**那条记录的 —— 即项目根 / 启动目录。
      // 不能用「第一个到达的」：启动重放末块先发，最先到的是最新记录，而会话的 cwd
      // 可能在过程中漂移（如工作目录切到子目录）→ 会抓到子目录而非项目根。与历史
      // 浏览器 quick_extract_cwd（读最早 cwd）口径一致。
      if (cwd && seq < tab.cwdSeq) {
        tab.cwd = cwd;
        tab.cwdSeq = seq;
        tab.title = this.computeTitle(tab);
        this.refreshTabBar();
      }
      return tab;
    }

    const title = computeTitleFor(sessionId, cwd, null, origin, kind, bgName);

    const streamEl = document.createElement("div");
    streamEl.className = "stream"; // 默认 .stream 已含 visibility:hidden（见 styles.css）
    this.streamRootEl.appendChild(streamEl);

    const stream = new MessageStream(streamEl);
    const branchFolder = new BranchFolder(stream.contentElement);
    const timeline = new RecordTimeline(stream);
    // v2.2 issue #12: 重放期创建的新 Tab 也进 batch 模式，避免每条 record 都
    // 触发 O(N) computeMainBranch。批结束时 onBatchEnd 会统一 flush。
    if (this.inBatch) {
      branchFolder.setBatchMode(true);
    }

    // v2.3.0 issue #11: 异步 fetch 初始 task 快照。task-update 事件路径并行更新
    // tasksBySid，两路收敛到同一份数据；若 sid 是 active 同步推给全局 panel。
    void fetchSessionTasks(sessionId).then((tasks) => {
      this.tasksBySid.set(sessionId, tasks);
      if (this.activeId === sessionId) {
        this.tasksPanel?.setSession(sessionId, tasks);
      }
    });

    tab = {
      sessionId,
      kind,
      bgName,
      title,
      cwd,
      // 记下当前 cwd 来源的 seq；后续更早（更小 seq）的记录可覆盖（取项目根）。
      cwdSeq: cwd ? seq : Number.POSITIVE_INFINITY,
      aiTitle: null,
      forkedFromSessionId: null, // issue #63①:onLine 见首条 forkedFrom 记录时锁定
      origin,
      status: "live",
      streamEl,
      stream,
      parentPath: sourcePath,
      unread: 0,
      timeline,
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      branchFolder,
      pendingToolResults: new Map(),
      seenSeqs: new Set(),
      window: new TailWindow(),
      midBatchBuffer: [],
      fillHandler: null,
      processedUuids: new Set(),
      // issue #23：红绿灯信号若先于建 Tab 到达，从暂存取（否则 null=未知→绿）
      activity: this.pendingActivity.get(sessionId) ?? null,
      agents: new Map(),
      touchedFiles: new Set(), // F70：会话改动集，onLine 增量累进
      latestPromptTokens: null, // F88b：HUD context% 数据；onLine 捕获带 usage 的 assistant 记录
      latestModel: null,
      latestUsageSeq: -1,
    };
    this.pendingActivity.delete(sessionId);
    // F40b:上翻补批触发器(passive 只读滚动位置;handler 内判 active,后台 tab
    // 的程序化滚动/尺寸变化不触发补批)
    const fillHandler = (): void => {
      if (this.activeId !== sessionId) return;
      const t = this.tabs.get(sessionId);
      if (t && t.streamEl.scrollTop <= TabManager.TOP_TRIGGER_PX) this.fillAbove(t);
    };
    streamEl.addEventListener("scroll", fillHandler, { passive: true });
    tab.fillHandler = fillHandler;
    // issue #19：若该 sid 的归档信号先于本次建 Tab 到达（见 archiveTab），落实归档，
    // 避免重载后已结束会话复活成关不掉的 live Tab。本地 un-archive（上方 origin!==null
    // 那条）不适用，故归档后续 replay 行也不会把它复活。
    if (this.pendingArchive.delete(sessionId)) {
      tab.status = "archived";
      tab.activity = null; // 同 archiveTab：死会话不留陈旧灯/tooltip
    }
    this.tabs.set(sessionId, tab);
    this.placeInOrder(tab);

    if (this.activeId === null) {
      // "auto"：首个 Tab 的激活不是用户手势，不该占用 5s manualOverride 抑制
      // auto-follow（G5 验收 S-1）
      this.switchTo(sessionId, "auto");
    } else {
      this.refreshTabBar();
    }
    return tab;
  }

  /** 应用 ai-title：锁定语义标题，并按 [项目名] aiTitle 格式更新 Tab 标题 */
  private applyAiTitle(tab: Tab, aiTitle: string): void {
    const trimmed = aiTitle.trim();
    if (!trimmed) return;
    if (tab.aiTitle === trimmed) return;
    tab.aiTitle = trimmed;
    tab.title = this.computeTitle(tab);
    this.refreshTabBar();
  }

  /**
   * issue #63①：从记录里取 `forkedFrom.sessionId`，锁定血缘并给标题加 `↳` 徽标（同 aiTitle:出现一次
   * 就锁,后续记录/重投不覆盖）。fork 会话的首条记录带 `forkedFrom`（Claude 原生 `/branch` 格式,
   * 后端 history.rs 也读它）——但活 tab 层此前完全不看它,fork 与原会话是同名独立 tab、分不清。
   */
  private applyForkedFrom(tab: Tab, message: unknown): void {
    if (tab.forkedFromSessionId) return; // 已锁定
    const fk = (message as { forkedFrom?: { sessionId?: unknown } }).forkedFrom;
    const sid = fk?.sessionId;
    if (typeof sid !== "string" || sid.length === 0) return;
    tab.forkedFromSessionId = sid;
    tab.title = this.computeTitle(tab);
    this.refreshTabBar();
  }

  /** 根据 tab.cwd + tab.aiTitle + sessionId 算出展示标题（远端 Tab 加 `[origin]` 前缀） */
  private computeTitle(tab: Tab): string {
    return computeTitleFor(
      tab.sessionId,
      tab.cwd,
      tab.aiTitle,
      tab.origin,
      tab.kind,
      tab.bgName,
      tab.forkedFromSessionId,
    );
  }

  /** session 退出（~/.claude/sessions/<PID>.json 被删）—— 灰显归档，内容保留 */
  archiveTab(sessionId: string): void {
    const tab = this.tabs.get(sessionId);
    if (!tab) {
      // issue #19：Tab 还没被 ensureTab 建出来（归档信号早于 replay 行到达）——
      // 记下待归档，建 Tab 时落实。否则这里直接 return 会静默丢弃归档 → 僵尸 live Tab。
      this.pendingArchive.add(sessionId);
      this.pendingActivity.delete(sessionId); // issue #23：死会话的暂存灯一并清
      return;
    }
    if (tab.status === "archived") return;
    tab.status = "archived";
    // issue #23：会话结束 → 灯灭（CSS 上 archived 本就隐藏 .live-dot，这里保持状态干净）
    tab.activity = null;
    this.sweepRunningAgents(tab); // 会话死了，running agent 必然中止
    // P5.2 B 重构后无 pendingToolGroup —— archive 不需要打断 tool-group 累积
    // （tool-group 合并改后处理，看 timeline 邻居；archive 后无新 record 入 timeline）。
    this.refreshTabBar();
  }

  /**
   * 会话（重新）变活 → 复活已归档 Tab。archiveTab 的对称面，由后端 SESSION_STARTED
   * 事件驱动（见 events.ts；后端已用 is_session_active 门控，只在 PID 真活时发）。
   *
   * **仅本地**（origin===null）：本地归档/复活由 PID 探活驱动，不靠「收到行」翻转——
   * 避免会话退出尾写误复活（见 ensureTab 行 372 的远端-only 复活注释）。远端 Tab 复活
   * 仍走 ensureTab「掉线归档→重连重放见行复活」路径，与本方法正交、互不触发。
   *
   * 先撤 pendingArchive：归档信号若还停在那（Tab 尚未由 ensureTab 建出），不撤的话
   * 随后建 Tab 会按 pendingArchive 落实归档（行 442）→ 复活被吞。Tab 不存在（全新
   * 会话首启、jsonl 行尚未建 Tab）则 no-op：随后 jsonl-batch 建的新 Tab 默认即 live。
   */
  reviveTab(sessionId: string): void {
    this.pendingArchive.delete(sessionId);
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.origin !== null) return; // 仅本地；远端复活走 ensureTab 见行路径
    if (tab.status !== "archived") return;
    tab.status = "live";
    this.refreshTabBar();
  }

  /**
   * issue #23：红绿灯状态更新（session-activity 事件 / 启动快照两路汇入）。
   * status=null（旧版 CC 无字段）视为未知 → 清空回绿点现状。Tab 还没建则暂存
   * （pendingActivity，ensureTab 落实）。无变化不重绘。
   */
  updateActivity(
    sessionId: string,
    status: string | null,
    waitingFor: string | null,
  ): void {
    const act = status === null ? null : { status, waitingFor };
    const tab = this.tabs.get(sessionId);
    if (!tab) {
      if (act) this.pendingActivity.set(sessionId, act);
      else this.pendingActivity.delete(sessionId);
      return;
    }
    // archived 不更新（审计：心跳清死会话后磁盘残留 PID.json 被重扫会推陈旧
    // activity，archived tab 会挂上过期的 waiting tooltip——灯本身被 CSS 隐藏）。
    if (tab.status === "archived") return;
    if (
      tab.activity?.status === act?.status &&
      tab.activity?.waitingFor === act?.waitingFor
    ) {
      return;
    }
    tab.activity = act;
    // issue #23（第二增量）：turn 结束（idle/shell）→ 没等到 tool_result 的 agent
    // 必然不会再回来（ESC 打断/异常），标 aborted。waiting 不清——其他 agent 可能
    // 还在并行跑（waitingFor "worker request" 正是 agent 在要权限）。
    if (act && (act.status === "idle" || act.status === "shell")) {
      this.sweepRunningAgents(tab);
    }
    this.refreshTabBar();
  }

  /**
   * issue #23（第二增量）：从 jsonl 流配对 agent 工具调用。
   * - assistant 的 Task/Agent tool_use → 注册 running（label 取 input.description，
   *   回退 prompt 首行 / 工具名）
   * - user 的 tool_result（按 tool_use_id 命中）→ done
   * 防 spam：只在真有变化时刷新面板。结构防御：message 形态全 unknown 窄化，
   * 任何不匹配静默跳过（§18 同源精神）。
   */
  /**
   * F88b：从 assistant 记录抽 usage → 更新 tab.latestPromptTokens/latestModel（供 HUD context%）。
   * prompt token = input + cache_creation + cache_read（本轮喂进模型的总量，即 context 占用近似）。
   * 只认带 usage 的 assistant 记录（user/system/无 usage 的一律跳过，保留上一次值）。
   *
   * 审计加固：
   * - **seq 单调**：onLine 投递序不保证升序（重放/远端重投），故只在 `seq >= tab.latestUsageSeq`
   *   时覆盖——「最新」= 最大 seq 而非最后到达，防低 seq 历史记录盖掉高 seq 实时值（会误显低占用%）。
   * - **批期不刷 chip**：重放/大增量批（inBatch）里逐条 assistant 记录都触发 setActive 会视觉抖动
   *   （10%→20%→…），故批期只更 tab 字段、不喂回调；onBatchEnd 对活跃 tab 单次 flush。
   */
  private trackUsage(tab: Tab, message: unknown, seq: number): void {
    const rec = message as {
      type?: string;
      message?: {
        model?: unknown;
        usage?: {
          input_tokens?: unknown;
          cache_creation_input_tokens?: unknown;
          cache_read_input_tokens?: unknown;
        };
      };
    };
    if (rec?.type !== "assistant") return;
    const usage = rec.message?.usage;
    if (!usage || typeof usage !== "object") return;
    const num = (v: unknown): number => (typeof v === "number" && v >= 0 ? v : 0);
    const prompt =
      num(usage.input_tokens) +
      num(usage.cache_creation_input_tokens) +
      num(usage.cache_read_input_tokens);
    // 全 0（无任何 token 字段）→ 视为无效 usage，不覆盖上一次有效值。
    if (prompt <= 0) return;
    // seq 回退（更老的记录晚到）→ 不覆盖更新的值。
    if (seq < tab.latestUsageSeq) return;
    tab.latestUsageSeq = seq;
    tab.latestPromptTokens = prompt;
    tab.latestModel = typeof rec.message?.model === "string" ? rec.message.model : null;
    // 批期不即时喂 chip（onBatchEnd 单次 flush）；实时流则即时刷活跃会话。
    if (!this.inBatch && tab.sessionId === this.activeId) {
      this.onActiveUsageChanged?.(tab.latestModel, tab.latestPromptTokens);
    }
  }

  private trackAgents(tab: Tab, message: unknown): void {
    const rec = message as {
      type?: string;
      timestamp?: unknown;
      message?: { content?: unknown };
    };
    const content = rec?.message?.content;
    if (!Array.isArray(content)) return;
    // F77：这条 assistant 记录的 timestamp——存进 AgentEntry 供「点进 agent 看记录」的 load_subagent 定位。
    const recTimestamp = typeof rec.timestamp === "string" ? rec.timestamp : "";
    let changed = false;
    if (rec.type === "assistant") {
      for (const b of content) {
        const blk = b as {
          type?: string;
          id?: string;
          name?: string;
          input?: { description?: unknown; prompt?: unknown; subagent_type?: unknown };
        };
        if (
          blk?.type !== "tool_use" ||
          typeof blk.id !== "string" ||
          typeof blk.name !== "string" ||
          !isAgentTool(blk.name)
        ) {
          continue;
        }
        // F77：desc **trim 后**（镜像卡片 `input.description?.trim()`），供 load_subagent 精确匹配。
        const desc = (
          typeof blk.input?.description === "string" ? blk.input.description : ""
        ).trim();
        const prompt =
          typeof blk.input?.prompt === "string" ? blk.input.prompt : "";
        const label =
          desc || prompt.split("\n")[0]?.slice(0, 80) || blk.name;
        const agentType =
          typeof blk.input?.subagent_type === "string"
            ? blk.input.subagent_type
            : null;
        tab.agents.set(blk.id, {
          id: blk.id,
          label,
          agentType,
          status: "running",
          timestamp: recTimestamp, // F77：供 load_subagent 定位子 agent
          desc, // F77：load_subagent 精确匹配的 description（trim 后，非展示 label）
        });
        changed = true;
      }
      // 上限 30：超出删最老的非 running（Map 保持插入序）
      if (tab.agents.size > 30) {
        for (const [id, a] of tab.agents) {
          if (tab.agents.size <= 30) break;
          if (a.status !== "running") tab.agents.delete(id);
        }
      }
    } else if (rec.type === "user") {
      for (const b of content) {
        const blk = b as { type?: string; tool_use_id?: string };
        if (blk?.type !== "tool_result" || typeof blk.tool_use_id !== "string") {
          continue;
        }
        const a = tab.agents.get(blk.tool_use_id);
        if (a && a.status === "running") {
          a.status = "done";
          changed = true;
        }
      }
    }
    if (changed) this.agentsChanged(tab);
  }

  /** issue #23：会话不再 busy（idle/shell/归档）→ 仍 running 的 agent 标 aborted
   *（ESC 打断/崩溃不会有 tool_result，防僵尸"运行中"）。 */
  private sweepRunningAgents(tab: Tab): void {
    let changed = false;
    for (const a of tab.agents.values()) {
      if (a.status === "running") {
        a.status = "aborted";
        changed = true;
      }
    }
    if (changed) this.agentsChanged(tab);
  }

  /** agents 变化 → 若是 active Tab 同步给全局面板 */
  private agentsChanged(tab: Tab): void {
    if (this.activeId === tab.sessionId) {
      this.agentsPanel?.setSession(tab.sessionId, [...tab.agents.values()]);
    }
  }

  /**
   * issue #23：启动/F5 后拉一次红绿灯快照做初始收敛——session-activity 是稀疏
   * 事件、不进 replay buffer，重载会丢（同 fetchSessionTasks 的双路收敛模式）。
   * 失败静默（灯保持未知绿，不影响主功能）。
   */
  async syncActivitySnapshot(): Promise<void> {
    try {
      const list = await invoke<
        { session_id: string; status: string | null; waiting_for: string | null }[]
      >("list_session_activity");
      for (const a of list) {
        this.updateActivity(a.session_id, a.status, a.waiting_for);
      }
    } catch (e) {
      console.warn("list_session_activity failed:", e);
    }
  }

  /**
   * 关闭 Tab：销毁 stream DOM、从 Map 中移除、通知后端 forget 历史、必要时切到相邻 Tab。
   * 仅允许关闭 archived 状态的 Tab，避免误关运行中的会话。
   * forget 后该 session 不会在下次 F5 刷新时被 event_replay 重放复活。
   */
  closeTab(sessionId: string): void {
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status !== "archived") return;

    const wasActive = this.activeId === sessionId;
    const idx = this.orderedIds.indexOf(sessionId);
    // 优先切到后一个 Tab，否则前一个
    const fallbackId =
      this.orderedIds[idx + 1] ?? this.orderedIds[idx - 1] ?? null;

    tab.stream.dispose();
    tab.streamEl.remove();
    // 显式清 Map：释放对已卸载 DOM 节点的强引用，让 GC 可早回收
    // （Map 本身也会随 Tab 对象一起回收，但显式 clear 让 DOM 引用计数立即归零）
    tab.toolUseNames.clear();
    tab.toolUseElements.clear();
    tab.pendingToolResults.clear();
    tab.seenSeqs.clear();
    tab.processedUuids.clear();
    // F40a/b:窗口账本与缓冲持整段历史 payload(大会话数十 MB 级),断引用;摘 fill listener
    tab.window.dispose();
    tab.midBatchBuffer = [];
    if (tab.fillHandler) tab.streamEl.removeEventListener("scroll", tab.fillHandler);
    tab.timeline.dispose();
    tab.branchFolder.dispose();
    this.tasksBySid.delete(sessionId);
    this.tabs.delete(sessionId);
    if (idx >= 0) this.orderedIds.splice(idx, 1);

    // 让后端 event_replay 把这个 session 的历史也丢掉
    void invoke("forget_session", { sessionId }).catch((e) => {
      console.warn(`forget_session ${sessionId} failed:`, e);
    });

    if (wasActive) {
      if (fallbackId !== null) {
        this.switchTo(fallbackId);
      } else {
        this.activeId = null;
        // issue #11: 关掉最后一个 Tab → panel 进入 null session 状态
        this.tasksPanel?.setSession(null, []);
        this.agentsPanel?.setSession(null, []);
        // F88b（审计）：无 fallback 时 switchTo 不会跑 → 手动隐藏 HUD chip，否则残留死会话的 ctx%
        this.onActiveUsageChanged?.(null, null);
        this.refreshTabBar();
      }
    } else {
      this.refreshTabBar();
    }
  }

  /**
   * 切到上 / 下一个 Tab。delta=+1 下一个、-1 上一个。环回。
   * 快捷键 Ctrl+Tab / Ctrl+Shift+Tab 用。
   */
  cycleActive(delta: 1 | -1): void {
    const ids = this.orderedIds;
    if (ids.length === 0) return;
    const idx = this.activeId ? ids.indexOf(this.activeId) : -1;
    const nextIdx = ((idx + delta) % ids.length + ids.length) % ids.length;
    const targetId = ids[nextIdx];
    if (targetId && targetId !== this.activeId) {
      this.switchTo(targetId);
    }
  }

  /**
   * 跳到第 N 个 Tab（1-indexed，issue #5 快捷键 Ctrl+1..9 用）。
   * N 大于现有 Tab 数 → 静默忽略；N 对应 Tab 已经 active → 无操作。
   */
  jumpToIndex(oneBasedIdx: number): void {
    const ids = this.orderedIds;
    if (oneBasedIdx < 1 || oneBasedIdx > ids.length) return;
    const targetId = ids[oneBasedIdx - 1];
    if (targetId && targetId !== this.activeId) {
      this.switchTo(targetId);
    }
  }

  /**
   * issue #11: 后端 `task-update` 事件路由——总是更新内存 map（即使 Tab 还没建），
   * 只有 sid 是当前 active 时才推全局 panel 重渲染。
   *
   * 不需要 "Tab 不存在就丢弃"——task 文件先于 jsonl 出现是合法时序，
   * 之后 ensureTab 时会从 tasksBySid 拿数据；fetchSessionTasks 拿到的也是同样数据。
   */
  updateTasks(sessionId: string, tasks: TaskEntry[]): void {
    this.tasksBySid.set(sessionId, tasks);
    if (this.activeId === sessionId) {
      this.tasksPanel?.setSession(sessionId, tasks);
    }
  }

  /**
   * v2.4 issue #2：把 behavior config 应用到 TabManager。
   * 启动时由 main.ts 调一次拉初值；设置面板 toggle 改了也调一次同步。
   */
  applyBehavior(cfg: BehaviorConfig): void {
    this.autoFollowUserActive = cfg.autoFollowUserActive;
    this.bringMonitorToFront = cfg.bringMonitorToFrontOnUserActive;
  }

  /**
   * v2.4 issue #2：watcher 反推识别到"用户在终端真敲了一行回车"（type=user
   * 且不是 tool_result 回灌 / CLI noise，由 tabs.onLine 的 result.kind 判定）
   * 时调用本方法。
   *
   * **跳过条件**（任一命中就 silently no-op）：
   * 1. autoFollowUserActive=false（设置面板关了）
   * 2. manualOverrideUntil > now（用户 5s 内手动点过 tab，明确意图保护）
   * 3. sid 不存在 / 已 archive（防御）
   * 4. sid 已经是 active（无操作）
   *
   * 通过后调 switchTo(sid, "auto")，可选 invoke bring_monitor_to_front。
   */
  userActive(sessionId: string): void {
    // v2.6 修回归：B 重构后 render-stream-record 删了 source="live" 过滤参数，
    // chunked replay batch 期间的历史 user 消息会触发本方法 → 反复自动切 tab。
    // 在这里加 inBatch 守卫等价 v2.5 的 source==="live" 检查。
    if (this.inBatch) return;
    if (!this.autoFollowUserActive) return;
    if (Date.now() < this.manualOverrideUntil) return;
    const tab = this.tabs.get(sessionId);
    if (!tab) return;
    if (tab.status === "archived") return;
    if (this.activeId === sessionId) {
      // 已经在这个 tab 但用户开了"拉前 monitor"也照拉
      if (this.bringMonitorToFront) {
        void invoke("bring_monitor_to_front").catch((e) => {
          console.warn("bring_monitor_to_front failed:", e);
        });
      }
      return;
    }
    this.switchTo(sessionId, "auto");
    if (this.bringMonitorToFront) {
      void invoke("bring_monitor_to_front").catch((e) => {
        console.warn("bring_monitor_to_front failed:", e);
      });
    }
  }

  /** 快捷键 Ctrl+W：当前活跃 Tab 是 archived 才关，live 不动 */
  closeActiveIfArchived(): void {
    if (!this.activeId) return;
    const tab = this.tabs.get(this.activeId);
    if (tab && tab.status === "archived") {
      this.closeTab(this.activeId);
    }
  }

  /** 快捷键 Ctrl+` ：把当前活跃 Tab 对应的终端窗口拉到前台（live 本地 / 远端均可） */
  bringActiveTerminalToFront(): void {
    if (!this.activeId) return;
    const tab = this.tabs.get(this.activeId);
    if (!tab || tab.status === "archived") return;
    // Feature ②：远端 Tab → 后端按 ccm-rbind HWND 缓存拉前；本地 Tab → 原 sid_hwnd_cache 路径。
    if (tab.origin !== null) {
      void bringRemoteTerminalToFront(this.activeId);
    } else {
      void bringTerminalToFront(this.activeId);
    }
  }

  /** 快捷键 Ctrl+Shift+E：打开当前活跃 Tab 的工作目录到系统文件管理器 */
  openActiveTabCwd(): void {
    if (!this.activeId) return;
    void this.openTabCwd(this.activeId);
  }

  /** F77：活跃 tab 的子 agent 加载上下文（parentPath + origin）——main.ts 点 agent 行时用它
   *  调 `load_subagent`。无活跃 tab / 无 parentPath → null；远端会话 origin!==null（不支持，调用方提示）。 */
  getActiveSubagentContext(): { parentPath: string; origin: string | null } | null {
    const tab = this.activeId !== null ? this.tabs.get(this.activeId) : undefined;
    if (!tab || !tab.parentPath) return null;
    return { parentPath: tab.parentPath, origin: tab.origin };
  }

  /** issue #10 快捷键 Ctrl+Shift+N：把当前活跃 Tab 在独立只读窗口打开 */
  openActiveInNewWindow(): void {
    if (this.activeId) void this.openInNewWindow(this.activeId);
  }

  /**
   * issue #10：在独立只读窗口打开指定 session（Tab 右键 / 快捷键 / 拖拽撕离）。
   *
   * `screenX` / `screenY`（可选）= 拖拽撕离的落点屏幕坐标（来自 mouseup 的
   * `e.screenX/screenY`）。两者都给出时透传给后端在该处摆放新窗口；右键 / 快捷键
   * 不传则后端走默认居中。
   */
  private async openInNewWindow(
    sid: string,
    screenX?: number,
    screenY?: number,
  ): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab) return;
    try {
      await invoke("open_session_in_new_window", {
        sessionId: sid,
        title: tab.title,
        ...(screenX !== undefined && screenY !== undefined
          ? { x: screenX, y: screenY }
          : {}),
      });
    } catch (e) {
      showActionFailureToast("打开新窗口失败", String(e));
    }
  }

  /**
   * Tab 撕离拖拽起点（左键 mousedown）。只是"候选"：记录起点 + 挂 document 级
   * mousemove/mouseup，等指针越过阈值才真正进入拖拽。子动作按钮（📂/↗/×）的
   * mousedown 已 stopPropagation，不会走到这里。
   */
  private beginTabDrag(e: MouseEvent, sid: string, root: HTMLElement): void {
    // 已有拖拽在进行（理论上不会，因 mouseup 会清）—— 防御性忽略。
    if (this.drag) return;
    // 新一轮交互开始：清掉可能残留的抑制标记，避免陈旧 flag 误吞下次 click。
    this.suppressClickSid = null;

    const barRight = this.barEl.getBoundingClientRect().right;
    const onMove = (ev: MouseEvent): void => this.onDragMove(ev);
    const onUp = (ev: MouseEvent): void => this.onDragUp(ev);
    this.drag = {
      sid,
      startX: e.clientX,
      startY: e.clientY,
      barRight,
      root,
      dragging: false,
      armed: false,
      ghost: null,
      onMove,
      onUp,
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }

  /** document mousemove：阈值判定 → 起拖（建 ghost / 变暗），随后跟随 + arm 检测。 */
  private onDragMove(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;

    // 容错：主键已松开（mouseup 在窗口外丢失，比如拖到别的 app 上释放）→ 收尾取消，
    // 不弹窗（落点不可信），避免 ghost 残留 + 拖拽状态卡死。下次按下会重新开始。
    if ((e.buttons & 1) === 0) {
      const wasDragging = d.dragging;
      const sid = d.sid;
      this.teardownDrag();
      if (wasDragging) this.suppressClickSid = sid;
      return;
    }

    if (!d.dragging) {
      const dx = e.clientX - d.startX;
      const dy = e.clientY - d.startY;
      if (Math.hypot(dx, dy) <= TabManager.DRAG_THRESHOLD_PX) return;
      // 越过阈值 → 正式起拖：阻止文本选区、建 ghost、源 Tab 变暗。
      e.preventDefault();
      d.dragging = true;
      d.root.classList.add("dragging");
      const ghost = document.createElement("div");
      ghost.className = "tab-drag-ghost";
      ghost.textContent = this.tabs.get(d.sid)?.title ?? "";
      document.body.appendChild(ghost);
      d.ghost = ghost;
    }

    // 跟随光标（偏右下避免压在指针正下方）。
    if (d.ghost) {
      d.ghost.style.left = `${e.clientX + 8}px`;
      d.ghost.style.top = `${e.clientY + 8}px`;
    }

    // arm：指针拖离竖栏右缘一段距离 = 松手即弹独立窗口（F33 前是下缘判定）。
    const armed = e.clientX > d.barRight + 16;
    if (armed !== d.armed) {
      d.armed = armed;
      if (d.ghost) {
        d.ghost.classList.toggle("armed", armed);
        d.ghost.textContent = armed
          ? "松开 → 独立窗口"
          : (this.tabs.get(d.sid)?.title ?? "");
      }
    }
  }

  /** 收尾：拆 document listener、清 ghost / 源 Tab 变暗、清空拖拽状态。 */
  private teardownDrag(): void {
    const d = this.drag;
    if (!d) return;
    document.removeEventListener("mousemove", d.onMove);
    document.removeEventListener("mouseup", d.onUp);
    d.ghost?.remove();
    d.root.classList.remove("dragging");
    this.drag = null;
  }

  /** document mouseup：收尾；armed 则在落点弹独立窗口。 */
  private onDragUp(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;
    const { dragging, armed, sid } = d;
    this.teardownDrag();

    if (!dragging) return; // 没越阈值 = 纯点击，交给 click handler 正常切 Tab。

    // 起过拖（无论 armed 与否）都抑制紧随的 click —— 拖完不该顺带切 Tab。
    this.suppressClickSid = sid;
    if (armed) {
      void this.openInNewWindow(sid, e.screenX, e.screenY);
    }
  }

  /**
   * audit-fixes F01（修 B1，full-audit 阻塞）：resume 前**现读磁盘** pin，不读内存镜像
   * `accountLastByS`。后者是 tab 徽章的 10s 刷新数据源，在①启动首轮刷新前（空 Map）②
   * `list_last_accounts` 抛错被 main.ts 无条件覆写成空 ③刚显式钉 pin 后 10s 内还没轮询到，
   * 这三种窗口里读它 → `withAccount` 的不-clobber 守卫拿到假 priorPin=null → 把磁盘真实
   * pin 静默覆盖成全局当前工作账号。与 history.ts:1489 的「现读」同口径，三处 resume 一致。
   * 读不到（无 pin / 查询失败）→ undefined → withAccount 落全局账号/基座，与旧行为一致
   * （区别只是不再"错误地覆盖"既有 pin）。
   */
  private async readSessionPin(sid: string): Promise<string | undefined> {
    try {
      const map = await invoke<Record<string, string>>("list_last_accounts");
      return map?.[sid];
    } catch {
      return undefined;
    }
  }

  /**
   * F37：手动 resume 一个已结束（灰）的 Tab。与历史浏览器 ↺ 同一套语义：
   * 本地 → 新终端窗口跑 resume（尊重 F34 自定义命令，缺省 cc 检测→claude）；
   * 远端 → F41 一键拉起 wt.exe/PowerShell 跑 `ssh -t …`，失败回退复制命令。
   * resume 成功后 CC 续写同一 jsonl，既有「会话复活」路径会自动把灰 Tab 点亮。
   */
  private async resumeTab(sid: string, accountName?: string, useBase = false): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab) return;
    const behavior = await getBehavior();
    if (tab.origin !== null) {
      // A4：带账号统一走 withAccount（点击时重解析 configDir + 记 lastAccount 源②，与 history 同口径）。
      // 本地账号切换是 A7，此处忽略（withAccount 只在远端调）。
      const origin = tab.origin;
      const cwd = tab.cwd ?? "";
      await withAccount(
        origin,
        accountName ?? null,
        (cd) => runRemoteResume(origin, sid, cwd, behavior.resumeCommandRemote, cd),
        {
          sessionId: sid,
          // audit-fixes F07（I 建议）：显式选号解析不到（登出/目录消失且缓存恰过期）→ 提示而非静默
          // 落基座（对齐 history.ts:1502；此前 resumeTab 缺此回调，用户明点的"用账号 X resume"被无声吞掉）。
          onUnselectable: (n) =>
            showActionFailureToast(
              "账号不可用",
              `账号「${n}」当前不可选（未登录 / 非隔离 / 目录缺失），改用该会话上次的账号 / 当前工作账号 resume。`,
              { level: "info", durationMs: 6000 },
            ),
          // account-ux U3:未显式选号 → 跟随(lastAccount sticky → 当前工作账号 → 基座)。显式选号维持 A4。
          // audit-fixes F01(修 B1):pin 现读磁盘,不读内存镜像 accountLastByS（见 readSessionPin）。
          // F01 步骤2:useBase = 显式「用基座 resume」——不注入、不跟随(老会话住基座,别被 follow
          //   注入全局当前账号导致 claude --resume 在错数据目录找不到会话，即 #75 主因的逃生口)。
          follow: accountName || useBase ? undefined : { lastAccount: await this.readSessionPin(sid) },
        },
      );
      return;
    }
    try {
      await invoke("resume_history_session", {
        sessionId: sid,
        cwd: tab.cwd ?? "",
        launcher: behavior.resumeCommandLocal || null,
      });
    } catch (err) {
      showActionFailureToast("恢复失败", String(err));
    }
  }

  /**
   * F52：tmux 版 resume（远端专用）——在远端 tmux 会话 `cc-<sid8>` 里幂等 resume Claude。
   * 与 resumeTab 的直连版并列;本地 tab（origin===null）无 tmux 用例,直接 return。
   */
  private async resumeTabTmux(sid: string, useBase = false): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab || tab.origin === null) return;
    const behavior = await getBehavior();
    const origin = tab.origin;
    const cwd = tab.cwd ?? "";
    // F74：先查该 origin 的 tmux 列表，据 @ccm_sid 分两路。**attach 决策对新鲜度最敏感**——
    // 8s 缓存里的 @ccm_sid 可能已被 /branch 漂移（快照记 N=A，N 此刻跑 B）→ 据陈旧快照 attach
    // 又会撞进漂移会话，正是本刀要修的 bug。故这里**总是新查、不读缓存**（用户主动 resume，一次
    // ssh 可接受；与 resolveAttachMenuItem 的 attach 一律新查对齐），查回来仍写缓存惠及其它路径。
    let sessions: TmuxSession[] | null = null;
    try {
      sessions = await invoke<TmuxSession[] | null>("list_remote_tmux", { origin });
      this.tmuxCache.set(origin, { ts: Date.now(), sessions });
    } catch {
      sessions = null; // 查询失败 → 走下面 fresh 分支（沿用旧幂等 resume 名，退化不变砖）
    }
    // ① 目标 sid 正活在某 tmux（@ccm_sid 命中）→ 直接 attach 它，回到活的后端，别重开一个。
    const live = findClaudeTmux(sessions, sid, cwd);
    if (live && live.sid === sid) {
      await runRemoteAttach(origin, live.name);
      return;
    }
    // ①.5 audit-fixes F03（idle-tmux 就地复用）：目标 sid 的 tmux 还在（@ccm_sid 精确命中）但
    // command≠claude —— 即 claude 已退、只剩交互 shell 的**空 cc-<sid8>**。往它**就地** resume
    // （复用原会话名，不 new-session）→ 不产 `cc-<sid8>-N` 孤儿（治 #76 根因）+ 空 shell 起得了 claude
    // （治 create-gate 短路只 attach 空 shell 的 #75 一条）。仅 @ccm_sid 精确命中才复用（不按 cwd 猜，
    // 免撞同目录漂移会话）。
    const idle = findIdleTmux(sessions, sid);
    if (idle) {
      await withAccount(
        origin,
        null,
        async (cd) => {
          await runRemoteResumeIntoExistingTmux(origin, sid, idle.name, behavior.resumeCommandRemote, cd);
        },
        // F04:useBase = 显式「用基座 resume（tmux）」——不跟随、不注入（与直连版 resumeTab 的基座逃生口
      // 对称，两后端一致；老会话住基座、别被 follow 注入全局账号 → #75）。
      // F04:useBase = 显式「用基座 resume（tmux）」——不跟随、不注入（与直连版 resumeTab 的基座逃生口
      // 对称，两后端一致；老会话住基座、别被 follow 注入全局账号 → #75）。
      { sessionId: sid, follow: useBase ? undefined : { lastAccount: await this.readSessionPin(sid) } },
      );
      return;
    }
    // ② 目标会话不在任何 tmux（已结束 / 已漂移到别的 sid）→ 起**全新** resume。tmux 名从现有
    // 名里挑一个不撞的，避免复用被 /branch 漂移占着的 cc-<sid8>（那正是「resume 进 branch」老 bug）。
    const existing = new Set((sessions ?? []).map((s) => s.name));
    const name = pickFreshTmuxName(sid, existing);
    // account-ux U3:tmux 版归档 resume 也跟随账号(注入 configDir)。① attach 活会话分支不动(账号焊死)。
    await withAccount(
      origin,
      null,
      // runRemoteResumeTmux 现在返回 boolean（Phase G）；withAccount 的 run 要 Promise<void>，
      // 这条归档 resume 路径不消费成败（失败已由它自己 toast + 剪贴板回退），故丢弃返回值。
      async (cd) => {
        await runRemoteResumeTmux(origin, sid, cwd, behavior.resumeCommandRemote, name, cd);
      },
      // audit-fixes F01(修 B1):pin 现读磁盘,不读内存镜像 accountLastByS（见 readSessionPin）。
      // F04:useBase = 显式「用基座 resume（tmux）」——不跟随、不注入（与直连版 resumeTab 的基座逃生口
      // 对称，两后端一致；老会话住基座、别被 follow 注入全局账号 → #75）。
      // F04:useBase = 显式「用基座 resume（tmux）」——不跟随、不注入（与直连版 resumeTab 的基座逃生口
      // 对称，两后端一致；老会话住基座、别被 follow 注入全局账号 → #75）。
      { sessionId: sid, follow: useBase ? undefined : { lastAccount: await this.readSessionPin(sid) } },
    );
  }

  /**
   * F51：菜单打开后异步反查 tmux——查该 origin 的会话列表(短缓存),按 `path===cwd &&
   * command==="claude"` 反查该 tab 的 Claude 所在 tmux 会话。命中 → 把禁用占位「检测中」
   * 换成可点的 Attach;无 tmux / 无匹配 / 查询失败 → 移除占位。菜单已关则 update/remove no-op。
   */
  private async resolveAttachMenuItem(
    origin: string,
    cwd: string,
    sid: string,
  ): Promise<void> {
    const gen = tabMenuGeneration; // 捕获发起查询的那一代菜单(R-1 守卫)
    let sessions: TmuxSession[] | null;
    try {
      sessions = await invoke<TmuxSession[] | null>("list_remote_tmux", { origin });
      // 只缓存确定结果(成功列表 / NO_TMUX=null);瞬时 ssh 失败不缓存,免 8s 内抑制重试(D-Sug3)。
      this.tmuxCache.set(origin, { ts: Date.now(), sessions });
    } catch {
      // 查询失败(纯 ssh exec 抖动)→ 移除占位,不缓存。
      if (gen === tabMenuGeneration) {
        removeTabContextMenuItem("attach");
        removeTabContextMenuItem("preview"); // F60：预览占位一并移除
        removeTabContextMenuItem("kill"); // F79：杀会话占位一并移除
      }
      return;
    }
    // 菜单已换/已关(新代次)→ 别动别的菜单(R-1 跨 tab 串味)。
    if (gen !== tabMenuGeneration) return;
    const match = findClaudeTmux(sessions, sid, cwd);
    const viaCwd = isCwdFallbackMatch(sessions, sid); // F74c：回退命中 attach 前提示串味风险
    if (match) {
      updateTabContextMenuItem("attach", {
        id: "attach",
        label: `Attach（tmux: ${match.name}）`,
        onClick: () => {
          if (viaCwd) warnCwdFallbackAttach();
          void runRemoteAttach(origin, match.name);
        },
      });
      // F60：预览项与 attach 同门(同一 tmux 会话),一并就绪。
      updateTabContextMenuItem("preview", {
        id: "preview",
        label: "预览画面",
        onClick: () => void openPanePreview(origin, match.name),
      });
      // F79：杀死会话项与 attach 同门（同一 tmux 会话）。回退命中 viaCwd 传入加强 caveat。
      updateTabContextMenuItem("kill", {
        id: "kill",
        label: "杀死会话（kill tmux）",
        danger: true,
        onClick: () => this.killRemoteTmux(origin, match.name, viaCwd),
      });
    } else {
      // audit-fixes F03.3（attach-into-idle）：无活 claude，但目标 sid 的**空 tmux**（@ccm_sid 命中、
      // command≠claude）还在 → 提供 attach 进那个空 shell（用户可在里面自己敲/看，或就地 resume）。
      const idle = findIdleTmux(sessions, sid);
      if (idle) {
        updateTabContextMenuItem("attach", {
          id: "attach",
          label: `Attach（空 tmux ${idle.name}，无 claude）`,
          onClick: () => void runRemoteAttach(origin, idle.name),
        });
        removeTabContextMenuItem("preview"); // 空 shell 无 claude 画面可预览
        removeTabContextMenuItem("kill");
      } else {
        removeTabContextMenuItem("attach");
        removeTabContextMenuItem("preview");
        removeTabContextMenuItem("kill");
      }
    }
  }

  /** A4/A5：远端 tab 菜单开后**异步追加**账号项——归档 tab → 每可选账号「用账号 X resume」；
   *  活 tab → 每可选账号「用账号 X 重启…」+「…（先压缩上下文）」(danger，§5)。复用 F51 代次守卫
   *  （gen !== tabMenuGeneration 则菜单已换/已关，整体 no-op，防 R-1 跨 tab 串味）。账号库不可用（§7
   *  daemonless/旧/未启用）/ <2 可选 → 不追加（默认 Resume 仍在）。异步 fetch 用新鲜值，无冷缓存分裂。 */
  private async appendAccountMenuItems(
    origin: string,
    sid: string,
    status: TabStatus,
  ): Promise<void> {
    const gen = tabMenuGeneration; // 捕获这一代菜单
    let state;
    try {
      state = await fetchAccounts(origin);
    } catch {
      return;
    }
    if (gen !== tabMenuGeneration) return; // 菜单已换/已关
    if (!state.available) return; // §7 降级
    const selectable = state.accounts.filter(isSelectable);
    // F01 步骤2:有 ≥1 可选账号时,follow 默认会注入某号 → 给归档会话一个显式「用基座 resume」
    // 逃生口(不隔离/原始 ~/.claude),让装账号前的老会话不被注错号(#75)。<1 账号时默认 Resume 本就走基座。
    if (status === "archived" && selectable.length >= 1) {
      appendTabContextMenuItem({
        id: "acct-resume-base",
        label: "用基座 resume（直连，不隔离）",
        title: "不注入任何账号，用原始 ~/.claude 直连 resume——装账号功能前的老会话住这里",
        onClick: () => void this.resumeTab(sid, undefined, true),
      });
      // F04：tmux 后端也给基座逃生口（与直连对称，两后端一致）。
      appendTabContextMenuItem({
        id: "acct-resume-base-tmux",
        label: "用基座 resume（tmux，不隔离）",
        title: "不注入任何账号，用原始 ~/.claude 在 tmux 里 resume",
        onClick: () => void this.resumeTabTmux(sid, true),
      });
    }
    if (selectable.length < 2) return; // 无可切换选择就不加噪（per-account 项）
    for (const a of selectable) {
      if (!a.configDir) continue;
      const name = a.name;
      if (status === "archived") {
        appendTabContextMenuItem({
          id: `acct-resume-${name}`,
          label: `用账号 ${name} resume`,
          onClick: () => void this.resumeTab(sid, name),
        });
      } else {
        // 活跃会话 → 换号破坏性重启（§5）。两条：直接重启 / 先在旧号压缩上下文再重启。
        appendTabContextMenuItem({
          id: `acct-restart-${name}`,
          label: `用账号 ${name} 重启…`,
          danger: true,
          title: `杀掉旧进程，用账号「${name}」resume 同一会话（中断当前回合）`,
          onClick: () => void this.restartTabWithAccount(sid, name, false),
        });
        appendTabContextMenuItem({
          id: `acct-restart-compact-${name}`,
          label: `用账号 ${name} 重启（先压缩上下文）`,
          danger: true,
          title: `先在【旧账号】上 /compact（命中旧缓存更省）再换号重启——比换号后再压缩便宜`,
          onClick: () => void this.restartTabWithAccount(sid, name, true),
        });
      }
    }
  }

  /** A5：造一个「等该 sid compact 完成」的 awaitCompact——注册 waiter 与超时竞速，两路都清理 waiter
   *  防泄漏。resolve(true)=onLine 检测到 compact 摘要行 / resolve(false)=超时（编排器照 §5.2 不阻断、续 kill）。
   *  默认 5min（§5）。 */
  private awaitCompactFor(sid: string, timeoutMs = 300_000): () => Promise<boolean> {
    return () =>
      new Promise<boolean>((resolve) => {
        let settled = false;
        const finish = (v: boolean): void => {
          if (settled) return;
          settled = true;
          this.compactWaiters.delete(sid);
          clearTimeout(timer);
          resolve(v);
        };
        this.compactWaiters.set(sid, () => finish(true));
        const timer = setTimeout(() => finish(false), timeoutMs);
      });
  }

  /**
   * A5+ 优雅退出等待器：轮询该 origin 的 tmux 列表，`claudeExited` 报「目标 sid 前台不再是 claude」
   * 即 resolve(true)；`timeoutMs`（默认 DEFAULT_EXIT_WAIT_MS=10s）到仍未退出 → resolve(false)（编排器
   * 据此降级 kill）。list 失败当「未知」跳过本轮（不误判已退出）。注入 `restartWithAccount.awaitExit`。
   */
  private awaitExitFor(
    origin: string,
    cwd: string,
    sid: string,
    timeoutMs = DEFAULT_EXIT_WAIT_MS,
    pollMs = 1000,
  ): () => Promise<boolean> {
    return () =>
      new Promise<boolean>((resolve) => {
        let stopped = false;
        let pollTimer: ReturnType<typeof setTimeout> | undefined;
        const stop = (v: boolean): void => {
          if (stopped) return;
          stopped = true;
          clearTimeout(timer);
          if (pollTimer) clearTimeout(pollTimer); // 清掉挂起的下一轮轮询，干净收尾
          resolve(v);
        };
        const timer = setTimeout(() => stop(false), timeoutMs);
        const tick = async (): Promise<void> => {
          if (stopped) return;
          let sessions: TmuxSession[] | null = null;
          let ok = false;
          try {
            sessions = await invoke<TmuxSession[] | null>("list_remote_tmux", { origin });
            ok = true;
          } catch {
            ok = false; // list 失败 → 本轮跳过（不误判已退出）
          }
          if (stopped) return;
          if (ok && claudeExited(sessions, sid, cwd)) {
            stop(true);
            return;
          }
          pollTimer = setTimeout(() => void tick(), pollMs);
        };
        void tick();
      });
  }

  /** A5：活跃远端会话换号重启——先解析该会话当前所在的 tmux 名（send-keys/kill 目标），再走
   *  `restartWithAccount` 编排（§5）。会话不在本工具 tmux（非本工具起/已漂移）→ 提示无法重启。 */
  private async restartTabWithAccount(
    sid: string,
    accountName: string,
    compactFirst: boolean,
    confirmFn?: (msg: string) => boolean,
  ): Promise<boolean> {
    const tab = this.tabs.get(sid);
    if (!tab || tab.origin === null) return false; // 本地会话 A7 前不支持
    // D 审计（重要）：同一 sid 的并发重启会互相打架——A 已 kill+resume 起了新 claude，B 的
    // awaitExit 看到新 claude 仍在 → 超时降级 kill → 把刚起来的新会话又杀了再 resume 一遍
    // （还多弹一个终端窗口）。点击到弹确认之间有多个 await（getBehavior/list_remote_tmux/
    // fetchAccounts/checkTrust）且无反馈，双击很自然 → 在**所有**入口（⇄/右键/批量）上游拦住。
    if (this.restartingSids.has(sid)) return false;
    this.restartingSids.add(sid);
    this.refreshAccountBadgeFor(sid); // ⇄ 立刻隐去，别让用户对着可点的按钮再点
    try {
      return await this.restartTabWithAccountInner(sid, tab, accountName, compactFirst, confirmFn);
    } finally {
      this.restartingSids.delete(sid);
      this.refreshAccountBadgeFor(sid);
    }
  }

  /** 单个 tab 的账号徽章/⇄ 就地重刷（in-flight 状态变化时用；tab 已没了就静默跳过）。 */
  private refreshAccountBadgeFor(sid: string): void {
    const refs = this.tabButtons.get(sid);
    const tab = this.tabs.get(sid);
    if (refs && tab) this.updateAccountBadge(refs, sid, tab);
  }

  private async restartTabWithAccountInner(
    sid: string,
    tab: Tab,
    accountName: string,
    compactFirst: boolean,
    confirmFn?: (msg: string) => boolean,
  ): Promise<boolean> {
    if (tab.origin === null) return false;
    const origin = tab.origin;
    const cwd = tab.cwd ?? "";
    const behavior = await getBehavior();
    // 解析该会话当前 tmux 名，一律新查（对齐 resumeTabTmux：attach/重启对新鲜度最敏感，防据陈旧快照误伤）。
    let sessions: TmuxSession[] | null = null;
    try {
      sessions = await invoke<TmuxSession[] | null>("list_remote_tmux", { origin });
      this.tmuxCache.set(origin, { ts: Date.now(), sessions });
    } catch {
      sessions = null;
    }
    const live = findClaudeTmux(sessions, sid, cwd);
    // A5 阻塞修（D 审计）：破坏性重启**必须**精确命中 @ccm_sid（`live.sid === sid`）。无 @ccm_sid 的
    // 降级远端会走 findClaudeTmux 的 cwd 回退 → 可能抓到同目录**别的** claude（live.sid=null/异号）→
    // kill 错会话 + 对目标 sid 起新进程 = 双进程 / jsonl 双写（§5.2 要防的严重态）。对齐 resumeTabTmux
    // 的 `live.sid === sid` 守卫，回退命中即拒（不猜）——破坏性操作不接受"按目录猜"。
    if (!live || live.sid !== sid) {
      showActionFailureToast(
        "无法换号重启",
        "该会话不在（本工具的）tmux 里、或无法精确定位（缺 @ccm_sid 会话标记）——可先归档后用「用账号 X resume」。",
        { level: "info", durationMs: 8000 },
      );
      return false;
    }
    return await restartWithAccount({
      origin,
      sessionId: sid,
      cwd,
      tmuxName: live.name,
      accountName,
      launcher: behavior.resumeCommandRemote,
      compactFirst,
      // account-ux U6：批量对齐已在批量层两步确认过 → 传 `() => true` 免得逐会话再弹 N 次；
      // 单会话入口（右键菜单 / tab ⇄）不传 → 仍走 restartWithAccount 自带的破坏性二次确认。
      confirm: confirmFn,
      // A5 step5：真检测器——onLine 见该 sid 的 compact 摘要行即 resolve，超时（5min）按 §5.2 续 kill。
      awaitCompact: this.awaitCompactFor(sid),
      // A5+ 优雅退出：轮询 tmux 前台不再是 claude 即 resolve，10s 超时按 §5.2 ④ 降级 kill。
      awaitExit: this.awaitExitFor(origin, cwd, sid),
    });
  }

  /** account-ux U6：**唯一**的「这个会话现在可否一键对齐」谓词——⇄ 显隐、⚠k 计数、批量枚举、
   *  Ctrl+K 命令(U8) 全走它，避免同一条件写多遍后漂移（D 审计：曾写了两遍半）。
   *  返回可对齐时的目标账号名，否则 null。in-flight（正在重启）也算不可对齐 → ⇄ 置灰、不重复计数。 */
  private alignableCurrent(sid: string, tab: Tab): string | null {
    if (tab.origin === null) return null; // 远端优先：本地会话 A7 前不支持
    if (tab.status === "archived") return null; // 用户已停跟随 → 不弹破坏性动作
    if (this.restartingSids.has(sid)) return null; // 正在重启 → 防重入
    if (!shouldShowAccountBadge(tab.origin, this.accountReadyOrigins)) return null;
    const current = this.currentByOrigin.get(tab.origin) ?? null;
    if (!current) return null; // 当前工作账号未知/不可选（main.ts 已过 isSelectable）→ 不猜
    const live = this.sessionAccountsByS.get(sid);
    if (!live || !live.alive || !live.account) return null; // 仅活跃 live（死会话走 resume）
    if (!detectAccountMismatch(live.account, current)) return null;
    return current;
  }

  /** account-ux U6：单会话「用当前工作账号重启对齐」——tab ⇄ 按钮 / U8 的 Ctrl+K 命令共用入口。
   *  复用 restartTabWithAccount（含 @ccm_sid 精确守卫 + §5.2 失败语义），不新增编排。
   *  @returns 是否真的走到了 resume（false=被守卫拒/账号不可用/用户取消/kill 失败）。 */
  async alignSessionToCurrentAccount(sid: string): Promise<boolean> {
    const tab = this.tabs.get(sid);
    if (!tab) return false;
    const current = this.alignableCurrent(sid, tab);
    if (!current) return false;
    return await this.restartTabWithAccount(sid, current, false);
  }

  /** account-ux U8：当前活跃会话 sid（只读投影，供 Ctrl+K / 快捷键判定"对当前会话做某事"）。 */
  activeSessionId(): string | null {
    return this.activeId;
  }

  /** account-ux U6：可对齐会话的 sid 列表。⚠k 用 `.length`，U8 的 Ctrl+K/汇总浮层用列表本身。 */
  accountMismatchSids(): string[] {
    const out: string[] = [];
    for (const [sid, tab] of this.tabs) {
      if (this.alignableCurrent(sid, tab)) out.push(sid);
    }
    return out;
  }

  /** account-ux U6：枚举可对齐会话 + 各自目标账号 + 是否空闲（批量分桶用）。 */
  private accountMismatches(): { sid: string; current: string; idle: boolean }[] {
    const out: { sid: string; current: string; idle: boolean }[] = [];
    for (const [sid, tab] of this.tabs) {
      const current = this.alignableCurrent(sid, tab);
      if (!current) continue;
      // 「空闲」用**白名单**判定：会话状态枚举是 busy/idle/shell/waiting（bridge.rs
      // SessionActivityPayload 透传 CC 官方值，null=旧 CC/远端 v1 无该字段）——只有 idle/shell
      // （都在等输入）才算"几乎无感"；busy（跑回合中）/ waiting（等权限对话框，重启会丢掉待决策
      // 弹窗）/ null（未知）一律落第二步确认。破坏性动作上未知即保守。
      // 注意：`"running"` 是 **subagent** 的状态串（AgentEntry），不是会话状态，别混用。
      const st = tab.activity?.status ?? null;
      out.push({ sid, current, idle: st === "idle" || st === "shell" });
    }
    return out;
  }

  /** account-ux U6：不一致活会话数（状态栏 chip ⚠k 计数用）。in-flight 的已被谓词扣掉。 */
  countAccountMismatches(): number {
    return this.accountMismatchSids().length;
  }

  /** account-ux U6：批量把不一致活会话按各自 origin 的当前工作账号重启对齐。两步确认：先空闲（几乎
   *  无感），回合进行中的单独第二步确认（默认不含、会打断）。逐会话串行走 restartTabWithAccount
   *  （继承 §5.2：某会话 kill 失败只中止那一个；@ccm_sid 精确守卫防杀错）。破坏性——不新增语义。 */
  async alignAllToCurrentAccount(): Promise<void> {
    // D 审计（重要）：批量可跑数分钟，而 ⚠k 要等下一拍 10s 轮询才降 → 用户很容易再点一次，
    // 第二批拿着同一份陈旧列表并发 kill/resume，可能把第一批刚 resume 出来的新进程又杀掉。
    // 且批量传了 confirm:()=>true，第二批全程无人工拦截 → 这里必须自己挡住重入。
    if (this.aligningBatch) {
      showActionFailureToast("批量对齐进行中", "上一批还没跑完，等它结束再来。", {
        level: "info",
        durationMs: 4000,
      });
      return;
    }
    const all = this.accountMismatches();
    if (all.length === 0) return;
    const label = (m: { sid: string; current: string }): string =>
      `· ${this.tabs.get(m.sid)?.title ?? m.sid} → ${m.current}`;
    const idle = all.filter((m) => m.idle);
    const busy = all.filter((m) => !m.idle);
    const targets: { sid: string; current: string }[] = [];
    // 文案必须说真话（D 审计重-2）：对齐 = kill tmux + 重新拉起，**每个会话都会新开一个终端窗口**，
    // 被 kill 的旧窗口不会自动关（-NoExit）；对话内容从 jsonl 续写，但进程内存态（队列里的输入、
    // plan 模式、/model、MCP 连接、后台 bash 任务）会丢；正 attach 着的终端会断开。
    const COST =
      `\n\n代价：每个会话会新开一个终端窗口（旧窗口被结束但不会自动关闭）；对话内容从 jsonl 续写，` +
      `但队列里的输入、后台任务、/model 等进程内状态会丢失。批量模式不再逐个确认；` +
      `若某账号未信任该目录，会在弹出的终端里询问。`;
    if (idle.length > 0) {
      const ok = window.confirm(
        `将按各自的当前工作账号重启这 ${idle.length} 个**空闲**会话（它们当前在等输入）：\n` +
          idle.map(label).join("\n") +
          COST +
          `\n\n继续？`,
      );
      if (!ok) return;
      targets.push(...idle);
    }
    if (busy.length > 0) {
      const ok2 = window.confirm(
        `${idle.length > 0 ? "另有 " : ""}${busy.length} 个会话正在回合中或等待弹窗决策，` +
          `重启会打断当前回合、丢掉待决策的对话框。${idle.length > 0 ? "也" : ""}一起对齐吗？\n` +
          busy.map(label).join("\n") +
          (idle.length > 0 ? "" : COST),
      );
      if (ok2) targets.push(...busy);
    }
    if (targets.length === 0) return;
    // 串行对齐（重启是重操作，避免同时轰远端；各自成功/失败由 restartTabWithAccount 自身 toast）。
    // confirm 传 `() => true`：批量层已两步确认，不让 restartWithAccount 再弹 N 个框。
    this.aligningBatch = true;
    let done = 0;
    try {
      for (const m of targets) {
        // per-iteration catch：决策④「逐会话独立」目前靠被调方各自 catch，无结构保证；
        // 将来链上任一步改成抛，整批不该从中间静默断掉。
        try {
          if (await this.restartTabWithAccount(m.sid, m.current, false, () => true)) done += 1;
        } catch (e) {
          console.warn("alignAll: 会话对齐失败", m.sid, e);
        }
      }
    } finally {
      this.aligningBatch = false;
    }
    // 只报**真结果**：守卫拒绝 / 账号不可用 / kill 失败中止都不算成功（D 审计：曾按发起数报成功，
    // 最坏是"0 成功 + 一句成功汇总"）。
    const failed = targets.length - done;
    showActionFailureToast(
      failed === 0 ? "批量对齐完成" : "批量对齐部分完成",
      `已重启对齐 ${done} 个会话` +
        (failed > 0 ? `；${failed} 个未执行（原因见各自提示）。` : "。"),
      { level: failed === 0 ? "info" : "error", durationMs: failed === 0 ? 5000 : 8000 },
    );
  }

  /** F79(#38)：杀死远端 tmux 会话——二次确认后 kill-session。变灰由 #60-A 对账兜（不主动 archive，守 §24）。
   *  @param viaCwd findClaudeTmux 是否走了 cwd 回退命中（无 @ccm_sid）——此时会话名是按目录猜的、可能
   *  不是本 tab 的会话（可能杀到同目录别的 Claude）。破坏性操作，回退命中时在确认框里加强 caveat
   *  （比 attach 的 toast 更强，因为在用户必须点的确认里）。守 F74c「保留回退+显式提示」的取舍。 */
  private killRemoteTmux(origin: string, tmuxName: string, viaCwd: boolean): void {
    const caveat = viaCwd
      ? `\n\n⚠ 未检测到会话身份标记（@ccm_sid）——「${tmuxName}」是按工作目录猜的，可能不是本 tab 的会话，甚至可能是同目录里另一个正在运行的 Claude。建议在远端重装 ccm 助手后再操作。`
      : "";
    const ok = window.confirm(
      `杀死会话「${tmuxName}」（机器 ${origin}）？\n\n将终止远端这个 tmux 会话里正在运行的 Claude，未保存的交互会中断。\n此操作不可恢复。${caveat}`,
    );
    if (!ok) return;
    void (async () => {
      try {
        await invoke("kill_remote_tmux", { origin, target: tmuxName });
        showActionFailureToast(
          "已杀死会话",
          `远端 [${origin}] 的 tmux 会话「${tmuxName}」已终止；tab 稍后自动变灰。`,
          { level: "info", durationMs: 6000 },
        );
      } catch (err) {
        showActionFailureToast("杀死会话失败", String(err));
      }
    })();
  }

  /**
   * audit-fixes F05：手动清理**真孤儿**会话——点击才查该 origin 的 tmux 列表，筛出无对应活 tab 的
   * `cc-*`(带 @ccm_sid)会话，列出 + 二次确认后逐个 `kill_remote_tmux`。**不自动**(用户拍板孤儿仅手动)。
   * 判据见 `findOrphanTmux`(保守：只杀身份确凿、且当前无 tab 的；绝不碰有 tab 的会话或非 cc-* 的用户会话)。
   */
  async cleanupOrphanTmux(origin: string): Promise<void> {
    let sessions: TmuxSession[] | null;
    try {
      sessions = await invoke<TmuxSession[] | null>("list_remote_tmux", { origin });
    } catch (e) {
      showActionFailureToast("清理孤儿失败", `查询远端 [${origin}] 的 tmux 列表失败：${String(e)}`);
      return;
    }
    const orphans = findOrphanTmux(sessions, new Set(this.tabs.keys()));
    if (orphans.length === 0) {
      showActionFailureToast("没有孤儿会话", `[${origin}] 上没有需要清理的残留 cc-* 会话。`, {
        level: "info",
        durationMs: 5000,
      });
      return;
    }
    const list = orphans.map((s) => `  · ${s.name}（${s.command}）`).join("\n");
    const ok = window.confirm(
      `在 [${origin}] 上发现 ${orphans.length} 个孤儿 tmux 会话（tab 已关、tmux 会话残留）：\n\n${list}\n\n` +
        `逐个 kill 掉？不可恢复——**只影响这些无 tab 的残留会话**，不碰你还有 tab 的会话、也不碰非本工具的会话。`,
    );
    if (!ok) return;
    let killed = 0;
    const failed: string[] = [];
    for (const s of orphans) {
      try {
        await invoke("kill_remote_tmux", { origin, target: s.name });
        killed += 1;
      } catch (e) {
        failed.push(`${s.name}: ${String(e)}`);
      }
    }
    showActionFailureToast(
      "清理孤儿完成",
      `已 kill ${killed}/${orphans.length} 个残留会话${failed.length ? `；失败：\n${failed.join("\n")}` : "。"}`,
      { level: failed.length ? "error" : "info", durationMs: 8000 },
    );
  }

  /** 打开指定 Tab 的 cwd。本地 → 系统文件管理器；远端 → SFTP 面板进入该远端目录（F78）。无 cwd 忽略。 */
  private async openTabCwd(sid: string): Promise<void> {
    const tab = this.tabs.get(sid);
    if (!tab?.cwd) return;
    // F78：远端 Tab 的 cwd 是远端路径，本地 openPath 打不开——改成用该机配置开 SFTP 进入该目录
    // （Batch9-F29 曾从静默 no-op 改成 info 提示；现进一步真能浏览）。找不到该机配置才回退提示。
    if (tab.origin !== null) {
      const host = findHostByOrigin((await readRemoteConfig()).hosts, tab.origin);
      if (host && host.host.trim() !== "" && host.user.trim() !== "") {
        openSftpPanelDir(host, tab.cwd);
        return;
      }
      // 找到但缺 host/user = 配置不完整；没找到 = 未配置——分开措辞（审计建议）。
      const why = host
        ? "该机的远端配置缺 host / user（在设置 → 连接 补全后可用）"
        : "未找到该机的远端配置（在设置 → 连接 添加后可用）";
      showActionFailureToast(
        "远端目录无法本地打开",
        `该会话在远端机器 [${tab.origin}]，工作目录 ${tab.cwd} 不在本机；${why}。`,
        { level: "info" },
      );
      return;
    }
    try {
      await openPath(tab.cwd);
    } catch (e) {
      console.warn(`[tabs] openPath ${tab.cwd} failed:`, e);
    }
  }

  /**
   * 切到目标 Tab。
   *
   * v2.4 issue #2：`source` 区分用户主动 vs 自动跟随。
   * - `"manual"`（默认）：Tab Bar 点击 / Ctrl+Tab / 中键关 / 内部 fallback 切。
   *   设置 manualOverrideUntil = now+5s，期间拒绝 userActive 自动切。
   * - `"auto"`：watcher 反推 user-active 触发的自动切。不更新 override，
   *   不互相锁死（不然 auto 调 switchTo 又设 override 自己就被锁了）。
   */
  switchTo(sessionId: string, source: "manual" | "auto" = "manual"): void {
    if (!this.tabs.has(sessionId)) return;
    if (this.activeId === sessionId) return;

    // 切 active 走 .active class（CSS visibility 控制），避免 display:none/block
    // 触发整棵子树重建 layout tree 卡顿。详 styles.css 的 .stream 注释。
    for (const [sid, t] of this.tabs) {
      t.streamEl.classList.toggle("active", sid === sessionId);
    }
    const next = this.tabs.get(sessionId);
    if (next) next.unread = 0;
    this.activeId = sessionId;
    // Batch13-F40a:命中 virgin tab(启动重放全收纳,还没建过卡)→ 同步物化尾段,
    // 避免切过去一片空白(R-3:有界循环补到可滚,防工具密集会话一轮近空屏)。
    // 非 virgin tab 不动(上翻补批属 F40b)。
    if (next && next.window.floorSeq === null && next.window.pendingCount > 0) {
      this.materializeUntilFilled(next);
    }
    // F40b:切入即刷新哨兵(非 virgin 但账本非空的 tab 也要见到「还有 N 条」)
    if (next) this.updateSentinel(next);
    // D 审计 R-2:非 virgin + 不可滚 + 账本有余的 tab 没有 fill 入口(不可滚元素
    // 不产生 scroll 事件,哨兵可见却"上翻物理不可达")——切入时踢一次,rAF 自链
    // 接管直到可滚或账尽。
    if (next && next.window.pendingCount > 0) {
      const el = next.streamEl;
      if (el.scrollHeight - el.clientHeight <= 1) this.fillAbove(next);
    }
    // Batch5-F19：记住所在 tab——下次启动 active 选择 + replay 优先该 session。
    // viewer/tear-off 窗口共享同 origin 的 localStorage（INVARIANT § 14），它们的
    // TabManager 置 persistLastActive=false，防独立窗口看会话 X 污染主窗口记忆。
    if (this.persistLastActive) {
      safeSet(LS_KEYS.lastActiveSid, sessionId);
    }
    if (source === "manual") {
      this.manualOverrideUntil = Date.now() + TabManager.MANUAL_OVERRIDE_MS;
      this.onManualSwitch?.();
    }
    this.refreshTabBar(); // active 高亮 + badge 立即更新（廉价，不阻塞）

    // 切 Tab 卡顿优化：把会**强制同步 reflow** 的 scrollToBottom（读 scrollHeight）+
    // 面板整表 re-render 推到下一帧——让 .active 的 visibility 切换先绘制出来（切 Tab 即时
    // 跟手），重活下一帧再做。期间又切走则跳过（不把面板/滚动落到已非 active 的会话上）。
    requestAnimationFrame(() => {
      if (this.activeId !== sessionId) return;
      next?.stream.scrollToBottom();
      // issue #11: 切换 task panel 数据源到新 active Tab 的 sid
      this.tasksPanel?.setSession(sessionId, this.tasksBySid.get(sessionId) ?? []);
      // issue #23: agents 面板同步切到新 active Tab
      this.agentsPanel?.setSession(
        sessionId,
        [...(this.tabs.get(sessionId)?.agents.values() ?? [])],
      );
      // F88b：HUD context% chip 切到新 active 会话的最新 usage（无带 usage 记录 → null → 隐藏）
      const nt = this.tabs.get(sessionId);
      this.onActiveUsageChanged?.(nt?.latestModel ?? null, nt?.latestPromptTokens ?? null);
    });
  }

  /**
   * 局部更新策略（避免每次 onLine 都 replaceChildren）：
   *   1. 删除：tabButtons 缓存里有但 orderedIds 已没的 sid → 摘 DOM + 清缓存
   *   2. 创建：orderedIds 里有但缓存没的 sid → createTabButton 一次（含所有 5 个子
   *      元素 + 事件 listener），visibility 全交 CSS 控制
   *   3. 更新：updateTabButton 同步 active / archived / has-unread class + label/badge 文本
   *   4. 排序：iterate orderedIds + insertBefore，确保 DOM 顺序 = orderedIds 顺序
   *
   * CSS 配合（styles.css）：
   *   .tab.archived .live-dot { display: none }
   *   .tab:not(.archived) .tab-close { display: none }
   *   .tab .tab-badge { display: none }
   *   .tab.has-unread:not(.active) .tab-badge { display: inline-block }
   */
  private refreshTabBar(): void {
    // 1. 删
    const wanted = new Set(this.orderedIds);
    for (const sid of Array.from(this.tabButtons.keys())) {
      if (!wanted.has(sid)) {
        const refs = this.tabButtons.get(sid)!;
        refs.root.remove();
        this.tabButtons.delete(sid);
      }
    }

    // 2 + 3 + 4. 创建 / 更新 / 排序
    let prev: ChildNode | null = null;
    for (const sid of this.orderedIds) {
      const tab = this.tabs.get(sid);
      if (!tab) continue;
      let refs = this.tabButtons.get(sid);
      if (!refs) {
        refs = this.createTabButton(sid);
        this.tabButtons.set(sid, refs);
      }
      this.updateTabButton(refs, sid, tab);
      // 排序：希望此 button 出现在 prev 之后
      const targetNext: ChildNode | null = prev
        ? prev.nextSibling
        : this.barEl.firstChild;
      if (refs.root !== targetNext) {
        this.barEl.insertBefore(refs.root, targetNext);
      }
      prev = refs.root;
    }

    this.notifyChanged();
  }

  private createTabButton(sid: string): TabButtonRefs {
    const root = document.createElement("button");
    root.className = "tab";

    const dot = document.createElement("span");
    dot.className = "live-dot";
    root.appendChild(dot);

    const label = document.createElement("span");
    label.className = "tab-title";
    root.appendChild(label);

    // A3：账号徽章（该会话属于哪个账号）。默认隐藏，updateTabButton 按 sessionBadge 填。
    const acctBadge = document.createElement("span");
    acctBadge.className = "tab-acct-badge";
    acctBadge.style.display = "none";
    root.appendChild(acctBadge);

    // account-ux U6：⇄ 换号对齐按钮。默认隐藏，updateAccountBadge 仅对"活跃 live && 账号≠当前工作账号"显。
    const alignBtn = document.createElement("span");
    alignBtn.className = "tab-align-btn";
    alignBtn.textContent = "⇄";
    alignBtn.setAttribute("role", "button");
    alignBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      void this.alignSessionToCurrentAccount(sid);
    });
    alignBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    root.appendChild(alignBtn);

    const badge = document.createElement("span");
    badge.className = "tab-badge";
    root.appendChild(badge);

    // 📂 打开工作目录（cwd）—— 系统默认文件管理器
    const cwdBtn = document.createElement("span");
    cwdBtn.className = "tab-cwd";
    cwdBtn.textContent = "📂";
    cwdBtn.title = "打开工作目录 (E)";
    cwdBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      void this.openTabCwd(sid);
    });
    // 子动作按钮自己处理点击：吞掉 mousedown 避免在它们身上起 Tab 拖拽。
    cwdBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    root.appendChild(cwdBtn);

    // ↗ 拉对应终端窗口（v1.7 用 sid_hwnd_cache）
    const focusBtn = document.createElement("span");
    focusBtn.className = "tab-focus";
    focusBtn.textContent = "↗";
    focusBtn.title = "调出对应终端 (`)";
    focusBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      const t = this.tabs.get(sid);
      if (!t || t.status === "archived") return;
      // Feature ②：远端 Tab → 走后端按 ccm-rbind 标题缓存的 HWND 拉前（未绑定则 toast no-op）；
      // 本地 Tab → 走原 sid_hwnd_cache 路径。
      if (t.origin !== null) {
        void bringRemoteTerminalToFront(sid);
      } else {
        void bringTerminalToFront(sid);
      }
    });
    focusBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    root.appendChild(focusBtn);

    const closeBtn = document.createElement("span");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭 Tab";
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      this.closeTab(sid);
    });
    closeBtn.addEventListener("mousedown", (e) => e.stopPropagation());
    root.appendChild(closeBtn);

    root.addEventListener("click", () => {
      // 拖拽刚结束的那次 click 不切 Tab（drag-then-release ≠ 选中）。一次性消费。
      if (this.suppressClickSid === sid) {
        this.suppressClickSid = null;
        return;
      }
      this.switchTo(sid);
    });
    // 左键 mousedown：候选 Tab 撕离拖拽（越过阈值才真拖，否则仍是普通 click）。
    root.addEventListener("mousedown", (e) => {
      if (e.button !== 0) return;
      this.beginTabDrag(e, sid, root);
    });
    // 中键点击归档 Tab 也关闭（常见 UX）
    root.addEventListener("mousedown", (e) => {
      if (e.button !== 1) return;
      const t = this.tabs.get(sid);
      if (t?.status === "archived") {
        e.preventDefault();
        this.closeTab(sid);
      }
    });
    // issue #10：右键菜单「在新窗口打开」（双屏 / 并排）
    root.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      const t = this.tabs.get(sid);
      const items: TabMenuItem[] = [
        { label: "在新窗口打开", onClick: () => void this.openInNewWindow(sid) },
      ];
      // F70（护城河）：本地会话 + 有改动集 → 「在全景高亮本会话改动」。远端（代码不在本机、
      // code-picture 索引不到）/ 无改动 都不显示（门控之一，另两道在 touchedFilesFor + highlightSession）。
      if (t && t.origin === null && t.touchedFiles.size > 0) {
        items.push({
          label: "在全景高亮本会话改动",
          onClick: () => this.requestPanoramaHighlight?.(sid),
        });
      }
      // F37：灰 tab（会话已结束）右键手动 resume——不用绕去历史浏览器。
      // F41 起本地与远端都是一键拉起新终端（远端=wt.exe 跑 ssh -t，失败才回退复制命令）。
      // F52：远端归档 tab 再并列一个「Resume（tmux）」——在远端 tmux 会话里幂等 resume,
      // resume 完人在 tmux 里(断线可 F51 attach 回来);本地归档仍单「Resume」。
      if (t?.status === "archived") {
        if (t.origin !== null) {
          items.push({
            label: "Resume（直连）",
            onClick: () => void this.resumeTab(sid),
          });
          items.push({
            label: "Resume（tmux）",
            onClick: () => void this.resumeTabTmux(sid),
          });
          // A4/A5：账号项（归档→「用账号 X resume」）由 showTabContextMenu 后**异步追加**
          // （appendAccountMenuItems，复用 F51 代次守卫），消除同步 peek 的冷缓存分裂。
        } else {
          items.push({
            label: "Resume",
            onClick: () => void this.resumeTab(sid),
          });
        }
      }
      // F51：远端 tab（有 cwd）——反查该 cwd 正跑 claude 的 tmux 会话 → Attach。
      // 缓存命中同步定夺(无占位闪烁);未命中先禁用占位「检测中」+ 异步查询就绪。
      const origin = t?.origin ?? null;
      const cwd = t?.cwd ?? null;
      let needAsyncAttach = false;
      if (origin !== null && cwd) {
        const cached = this.tmuxCache.get(origin);
        if (cached && Date.now() - cached.ts < TMUX_CACHE_TTL_MS) {
          const m = findClaudeTmux(cached.sessions, sid, cwd);
          const viaCwd = isCwdFallbackMatch(cached.sessions, sid); // F74c：回退命中提示串味
          if (m) {
            items.push({
              id: "attach",
              label: `Attach（tmux: ${m.name}）`,
              onClick: () => {
                if (viaCwd) warnCwdFallbackAttach();
                void runRemoteAttach(origin, m.name);
              },
            });
            // F60：同一 tmux 会话可只读预览画面（capture-pane 快照，不 attach）。
            items.push({
              id: "preview",
              label: "预览画面",
              onClick: () => void openPanePreview(origin, m.name),
            });
            // F79：杀死会话（同一 tmux 会话，破坏性 + 二次确认）。回退命中 viaCwd 传入加强 caveat。
            items.push({
              id: "kill",
              label: "杀死会话（kill tmux）",
              danger: true,
              onClick: () => this.killRemoteTmux(origin, m.name, viaCwd),
            });
          } else {
            // audit-fixes F03.3：缓存命中、无活 claude，但有目标 sid 的空 tmux（idle-tmux）→ 同步给 attach。
            const idle = findIdleTmux(cached.sessions, sid);
            if (idle) {
              items.push({
                id: "attach",
                label: `Attach（空 tmux ${idle.name}，无 claude）`,
                onClick: () => void runRemoteAttach(origin, idle.name),
              });
            }
          }
        } else {
          items.push({
            id: "attach",
            label: "Attach（检测 tmux…）",
            enabled: false,
            onClick: () => {},
          });
          items.push({
            id: "preview",
            label: "预览画面（检测 tmux…）",
            enabled: false,
            onClick: () => {},
          });
          items.push({
            id: "kill",
            label: "杀死会话（检测 tmux…）",
            enabled: false,
            danger: true,
            onClick: () => {},
          });
          needAsyncAttach = true;
        }
      }
      showTabContextMenu(e.clientX, e.clientY, items);
      if (needAsyncAttach && origin !== null && cwd) {
        void this.resolveAttachMenuItem(origin, cwd, sid);
      }
      // A4/A5：远端 tab → 异步追加账号项（归档=「用账号 X resume」/ 活=「用账号 X 重启…」）。
      if (origin !== null && t) {
        void this.appendAccountMenuItems(origin, sid, t.status);
      }
    });

    return { root, label, badge, acctBadge, alignBtn, cwdBtn };
  }

  private updateTabButton(refs: TabButtonRefs, sid: string, tab: Tab): void {
    refs.root.classList.toggle("active", sid === this.activeId);
    refs.root.classList.toggle("archived", tab.status === "archived");
    refs.root.classList.toggle("has-cwd", !!tab.cwd);
    // FIX 5 / Feature ②（issue #15）：远端 Tab（origin 非 null）的 cwd 是 Pi 上的路径，
    // 本地不存在，故 .remote 类只隐藏「打开工作目录」📂（CSS）。「调出终端」↗ 现在保留
    // 给远端 —— 点击走 bringRemoteTerminalToFront（后端按 ccm-rbind 拉本地 ssh 窗口）。
    refs.root.classList.toggle("remote", tab.origin !== null);
    // Batch7-F24：bg 任务 tab——缩进 + ⌞ 前缀由 CSS 承担
    refs.root.classList.toggle("tab-bg", tab.kind !== null && tab.kind !== "interactive");
    // issue #23 红绿灯：busy=绿（.live-dot 默认色）/ idle·shell=红 / waiting=黄。
    // activity 为 null（旧版 CC / 远端 v1）不加类 → 维持现状绿点。
    // F91：语义抽到 session-status.ts 供 tab-bar 与 mission-control grid 共用（逐字节等价）。
    const actStatus = tab.activity?.status ?? null;
    const lightClass = activityLightClass(actStatus);
    refs.root.classList.toggle("act-idle", lightClass === "act-idle");
    refs.root.classList.toggle("act-waiting", lightClass === "act-waiting");
    const titleParts: string[] = [];
    if (actStatus === "waiting" && tab.activity?.waitingFor) {
      titleParts.push(`等待操作：${tab.activity.waitingFor}`);
    }
    // issue #63①：fork 会话在 tooltip 里标出血缘(徽标 `↳` 在标题上、来源 sid 在此)。
    if (tab.forkedFromSessionId) {
      titleParts.push(`↳ 从 ${tab.forkedFromSessionId.slice(0, 8)} fork 而来`);
    }
    refs.root.title = titleParts.join("\n");
    const unread = tab.unread > 0 && sid !== this.activeId;
    refs.root.classList.toggle("has-unread", unread);

    if (refs.label.textContent !== tab.title) {
      refs.label.textContent = tab.title;
    }
    if (unread) {
      const text = tab.unread > 99 ? "99+" : String(tab.unread);
      if (refs.badge.textContent !== text) {
        refs.badge.textContent = text;
      }
    }
    this.updateAccountBadge(refs, sid, tab); // A3：账号徽章随 tab 更新一并刷新
  }
}

/**
 * issue #10：极简一次性上下文菜单（Tab 右键用）。挂 document.body 作 fixed 浮层，
 * 点任意项 / 点外部 / Esc 即关。一次只允许一个（开新的前先关旧的）。
 *
 * B14-F51：升级为 action 注册表 lite——项带可选 `id`/`enabled`,并可对**已打开**菜单按 id
 * `update`/`remove`(承载异步就绪项,如 attach 的 tmux 反查回来才可点）。
 */
interface TabMenuItem {
  id?: string;
  label: string;
  enabled?: boolean; // 缺省 true;false = 禁用占位
  danger?: boolean; // F79：破坏性项（杀会话）红色样式
  title?: string; // A5：hover tooltip（如 compact 顺序说明）
  onClick: () => void;
}
let activeTabMenu: HTMLElement | null = null;
const activeTabMenuItems = new Map<string, HTMLButtonElement>();
/** F51：菜单代次令牌——每次开/关菜单自增。在飞的异步就绪(attach 反查)回来时比对代次,
 * 只作用于发起它的那一代菜单;换/关菜单后旧查询整体 no-op(防 R-1 跨 tab 串味错配)。 */
let tabMenuGeneration = 0;

function makeTabMenuButton(it: TabMenuItem): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "tab-context-menu-item";
  if (it.danger) btn.classList.add("is-danger");
  btn.textContent = it.label;
  if (it.title) btn.title = it.title;
  const enabled = it.enabled !== false;
  btn.disabled = !enabled;
  if (enabled) {
    btn.addEventListener("click", () => {
      closeTabContextMenu();
      it.onClick();
    });
  }
  return btn;
}

function showTabContextMenu(x: number, y: number, items: TabMenuItem[]): void {
  closeTabContextMenu();
  const menu = document.createElement("div");
  menu.className = "tab-context-menu";
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
  for (const it of items) {
    const btn = makeTabMenuButton(it);
    if (it.id) activeTabMenuItems.set(it.id, btn);
    menu.appendChild(btn);
  }
  document.body.appendChild(menu);
  activeTabMenu = menu;
  tabMenuGeneration++; // 新一代菜单 → 让上一代在飞的异步就绪回调失效
  // 下一拍再挂关闭监听，避免本次右键触发的事件立刻把菜单关掉
  window.setTimeout(() => {
    window.addEventListener("pointerdown", onDocPointerForMenu, true);
    window.addEventListener("keydown", onKeyForMenu, true);
  }, 0);
}

/** F51：把已打开菜单里某 id 项替换为新项(异步就绪→可点);菜单已关或无此 id 则 no-op。 */
function updateTabContextMenuItem(id: string, item: TabMenuItem): void {
  const old = activeTabMenuItems.get(id);
  if (!old || !activeTabMenu) return;
  const btn = makeTabMenuButton(item);
  activeTabMenuItems.set(item.id ?? id, btn);
  old.replaceWith(btn);
}

/** F51：移除已打开菜单里某 id 项(异步查无匹配);无此 id 则 no-op。 */
function removeTabContextMenuItem(id: string): void {
  const old = activeTabMenuItems.get(id);
  if (!old) return;
  old.remove();
  activeTabMenuItems.delete(id);
}

/** A4/A5：往已打开菜单**追加**一项(异步就绪,如账号列表 fetch 回来)。菜单已关则 no-op。 */
function appendTabContextMenuItem(item: TabMenuItem): void {
  if (!activeTabMenu) return;
  const btn = makeTabMenuButton(item);
  if (item.id) activeTabMenuItems.set(item.id, btn);
  activeTabMenu.appendChild(btn);
}

function closeTabContextMenu(): void {
  if (!activeTabMenu) return;
  activeTabMenu.remove();
  activeTabMenu = null;
  activeTabMenuItems.clear();
  tabMenuGeneration++; // 关菜单也让在飞的异步就绪回调失效(不改别的菜单)
  window.removeEventListener("pointerdown", onDocPointerForMenu, true);
  window.removeEventListener("keydown", onKeyForMenu, true);
}
function onDocPointerForMenu(e: PointerEvent): void {
  if (activeTabMenu && !activeTabMenu.contains(e.target as Node)) {
    closeTabContextMenu();
  }
}
function onKeyForMenu(e: KeyboardEvent): void {
  if (e.key === "Escape") closeTabContextMenu();
}

function projectNameFromCwd(cwd: string): string | null {
  const normalized = cwd.replace(/\\/g, "/").replace(/\/+$/, "");
  const last = normalized.split("/").filter(Boolean).pop();
  return last ?? null;
}

/**
 * 标题格式（决策见 project_monitor_decisions.md）：
 *   aiTitle 有 + cwd 有 → `[项目] aiTitle`
 *   aiTitle 有 + cwd 无 → `aiTitle`
 *   aiTitle 无 + cwd 有 → `项目`
 *   都没有 → `<sid 前 8 位>`
 *
 * issue #15：`origin`（远端 SSH 主机名）非空时，在以上结果前再加 `[origin] ` 前缀，
 * 让用户一眼区分本地 / 远端 Tab（如 `[raspberrypi.local] [proj] aiTitle`）。本地
 * （origin=null）行为与历史完全一致，不加任何前缀。
 *
 * Subagent 不再独立 Tab（嵌入到父 session 的 Task 折叠卡），所以没有 `↳` 前缀分支。
 */
function computeTitleFor(
  sessionId: string,
  cwd: string | null,
  aiTitle: string | null,
  origin: string | null = null,
  kind: string | null = null,
  bgName: string | null = null,
  forkedFromSessionId: string | null = null,
): string {
  // issue #63①:fork 会话在最终标题前加 `↳ ` 血缘徽标——与原会话(同名)区分开。
  const mark = (s: string): string => (forkedFromSessionId ? `↳ ${s}` : s);
  const project = cwd ? projectNameFromCwd(cwd) : null;
  // Batch7-F24：bg 任务 → ⚙ + 任务名（缩进/⌞ 由 .tab-bg 样式承担）
  if (kind !== null && kind !== "interactive") {
    const base = `⚙ ${bgName ?? aiTitle ?? project ?? sessionId.slice(0, 8)}`;
    return mark(origin ? `[${origin}] ${base}` : base);
  }
  let base: string;
  if (aiTitle) {
    base = project ? `[${project}] ${aiTitle}` : aiTitle;
  } else if (project) {
    base = project;
  } else {
    base = sessionId.slice(0, 8);
  }
  return mark(origin ? `[${origin}] ${base}` : base);
}

// P5.2 B 重构：markCardUuid + feedBranchFolder 已搬到 render-stream-record.ts
// （三 caller 共用 renderStreamRecord 函数内部调用）。tabs.ts 不再持有这两个 helper。

/**
 * 拉对应终端到前台。v1.7 实现：后端查 sid_hwnd_cache + 复合指纹校验 + SetForegroundWindow。
 *
 * 失败模式（任一都会显示在 toast 上）：
 *   - "未绑定窗口"：该 session 启动时没经过 cc function 握手（直接跑 claude 而非 cc）
 *   - "窗口已不存在"：用户关掉了对应 PS/WT 窗口
 *   - "HWND 复用"：原窗口关闭后 HWND 被另一个无关窗口拿到
 *   - "invoke 超时"：极端情况下 Win32 调用卡住
 */
function bringTerminalToFront(sessionId: string): Promise<void> {
  const timeoutMs = 5000;
  return Promise.race([
    invoke<void>("bring_terminal_to_front", { sessionId }),
    new Promise<never>((_, reject) =>
      window.setTimeout(
        () => reject(new Error(`invoke 超时 ${timeoutMs}ms（后端 Win32 调用可能卡住）`)),
        timeoutMs,
      ),
    ),
  ]).catch((e) => {
    console.warn(`bring_terminal_to_front ${sessionId} failed:`, e);
    // P4.5: 改走统一 toast stack（去掉单例 #bring-terminal-toast 的"先到先被覆盖"问题）。
    showActionFailureToast("拉前失败", String(e?.message ?? e));
  });
}

/**
 * Feature ②：拉远端 Tab 对应的本地 ssh 窗口到前台。后端按 `ccm-rbind-<sid>` 窗口标题
 * 标记缓存的 HWND + SetForegroundWindow。
 *
 * 失败模式（任一都会显示在 toast 上）：
 *   - "未绑定窗口…"：远端 session 没经过 ccm wrapper 握手（直接跑 claude 而非 ccm），
 *     后端找不到 ccm-rbind 标记的窗口。设置面板「远端 ↗ 拉前」里有 ccm 函数贴远端。
 *   - "窗口已不存在"：用户关掉了对应 ssh 窗口
 *   - "invoke 超时"：极端情况下 Win32 调用卡住
 */
function bringRemoteTerminalToFront(sessionId: string): Promise<void> {
  // #41:后端现扫重试窗口抬到 4s(ON_DEMAND_BIND_*,覆盖首次 attach 的标题四跳传播),故前端超时须
  // 抬到其上、留 Win32 activate 余量——5s→8s,否则前端超时会和后端重试撞车(刚要绑上就被判超时)。
  const timeoutMs = 8000;
  return Promise.race([
    invoke<void>("bring_remote_terminal_to_front", { sessionId }),
    new Promise<never>((_, reject) =>
      window.setTimeout(
        () => reject(new Error(`invoke 超时 ${timeoutMs}ms（后端 Win32 调用可能卡住）`)),
        timeoutMs,
      ),
    ),
  ]).catch((e) => {
    console.warn(`bring_remote_terminal_to_front ${sessionId} failed:`, e);
    showActionFailureToast("拉前失败", String(e?.message ?? e));
  });
}

