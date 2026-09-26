/**
 * 历史会话浏览视图。
 *
 * 设计：全屏接管 `#message-stream` 区域（不破坏 tab-bar / status-bar）。
 *  - 显示时把原 message-stream 的 children 临时挪走 → 视图 mount 进去
 *  - 关闭时反向还原
 *
 * 两级懒加载（性能优化）：
 *   1. open / refresh：问本机后端 `history-projects` 只拿项目级元数据（不读 jsonl 内容）。
 *      所有组**默认折叠**。〔C4d〕本机与远端都问本机常驻后端（远端那台由它沿 SSH 去问）。
 *   2. 用户展开某个项目组：问本机后端 `history-sessions`（带 projectDir 与 origin）拿该项目
 *      下所有会话详情，缓存到 `sessionCache`。下次展开同项目直接读缓存。
 *
 * 搜索两种模式（issue #6）：
 *   - "项目"（默认）：本地即时过滤项目级字段（name / path）+ 已展开项目内的会话标题。
 *   - "全文"：回车逐台（本机也是）经通道问那台后端的 `history-search`，全文搜索所有会话**内容**（user 输入 +
 *     Claude 回复；可勾选"含工具内容"附加 tool_use/result/thinking）。结果按 session
 *     分组 + snippet <mark> 高亮，点击进 viewer 滚动定位到命中消息。
 *
 * 操作：star / 重命名 / 删除 / 恢复 全部走单条 IPC，本地状态在响应回来后同步。
 *  - star/hide 改变会更新缓存中那条 entry，并同步对应 project 的 starred/hidden_count
 *  - delete 从缓存移除条目，并 -1 project.sessionCount
 */

import { resolveResumeCommand } from "../remote-config";
import { commands } from "../ipc/commands";
// 〔步 12·C〕本机那个 origin 的**唯一住址**（Rust 侧是 `inbound_client::LOCAL_ORIGIN`，
// 两侧由 `origin_tests::the_sentinel_agrees_with_the_two_existing_homes` 两向钉着）。
import { LOCAL_ORIGIN } from "../backend-policy";
import { originFromWire } from "../ipc/origin";
import { searchAllMachines } from "./history-search";
// 〔FE1〕本机 resume 的编排只有一份（铸名 · 账号 · 记 pin 都在里面）。
import { resumeLocalSession } from "../local-resume";
import { SessionViewer, type ViewerOptions } from "./session-viewer";
// `K-R92`：那三格是三态（`null` = 不知道，不是 0）。排序档与加减都只许从这里走 ——
// JS 会安静地把 `null` 当 0（`Number(null)` / `null > 0` / `null + 1`），那正是本件在治的病。
import { liveRank, starRank, bumpCounted, isKnown } from "./counted";
import { dispatcher } from "../keybindings/registry";
import { showActionFailureToast } from "../error-toast";
import { runRemoteResume, runNewSessionRemote } from "../remote-launch-run";
import { isSelectable } from "../accounts";
import { fetchAccounts } from "../account-reads";
import { withAccount, localLaunchAccountSync, localLaunchAccountNameSync, primeLocalLaunchAccounts } from "../launch-account";
import { rememberLocalLaunch } from "../local-launch-backfill";
import {
  actionsFor,
  type HistoryActionCtx,
  type HistoryActionId,
} from "./history-actions";
import { getBehavior } from "../behavior";
import { LS_KEYS, safeGetJson, safeSetJson, safeRemove } from "../local-storage";
import { formatTimestampSmart } from "../format";
import { copyText } from "../copy-table";
import {
  shouldRefetchRemote,
  HISTORY_REMOTE_TTL_MS,
  type RemoteSourceCache,
} from "./history-cache";
import {
  resolveOriginOpen,
  nextOverrides,
  sameOverrides,
  normalizeOverrides,
  normalizeOriginKeys,
  type OriginOpenOverrides,
} from "./history-prefs";

/** 项目级元数据，从本机后端 `history-projects` 拿（〔C4d〕`../history-reads::fetchLocalProjects`）。不含 session 内容。 */

/** issue #16：项目缓存/展开态的 key。本地 = projectDir；远端用 origin 命名空间隔离
 *  （本地与远端可能有相同的编码目录名，裸 projectDir 会撞 key）。 */
function projectKey(p: { origin?: string; projectDir: string }): string {
  return p.origin ? `${p.origin}\u0000${p.projectDir}` : p.projectDir;
}

/** 会话级详情，从本机后端 `history-sessions` 拿（〔C4d〕`../history-reads::fetchSessions`，一次交全）。 */

/**
 * issue #12: session tree node。child 关系由 forkedFromSessionId 建。
 * 项目内独立树（跨项目 fork 不连接）。parent 不在本项目时 child 当 root + marker。
 */
// C04d 批 6c：六个线上类型全部换成生成物（源 `history.rs` / `remote_history.rs` / `search.rs`）。
// 手写版与生成物**逐字等价** ⇒ 零漂移，价值是防将来漂。
// TS 侧原来的 `SearchHit` / `SearchSessionHits` 只是名字不同，用别名对上 `Hit` / `SessionHits`。
//
// `SessionTreeNode` **留手写**：它是前端自己的树形模型（Rust 不认识它），
// 同账本第 4 行「IR 是前端的意图模型，别拖过边界」。
// 〔C4d · 第四波 4B〕项目 / 会话两行的类型从 ts-rs 生成物改成手写（Rust 那份随 monitor 的 join 一起删了；形状由本机后端的成品 ＋
//   跨语言金样 `tests/__fixtures__/history-products.golden.json` 定），与问法一起住 `../history-reads`。
import {
  annotate,
  fetchLocalProjects,
  fetchRemoteProjects,
  fetchSessions,
  forgetAnnotation,
  historyReasonOf,
  lastAccounts,
  type HistoryProject,
  type HistorySessionEntry,
} from "../history-reads";
// 〔LOC1b · 4D〕搜索那三个线上类型的家从 Rust `search.rs` 的生成物换到 `history-search.ts`（本机也改问本机后端，那份 Rust 删了）。
import type { Hit as SearchHit, SearchResult, SessionHits as SearchSessionHits } from "./history-search";
import { askConfirm, askText } from "../ask-dialog";

interface SessionTreeNode {
  entry: HistorySessionEntry;
  children: SessionTreeNode[];
  /** 1 = 本项目里找不到 parent（跨项目 fork / parent 已物理删除）→ root 上加 marker */
  orphan: boolean;
}

/** 〔C4d〕改注解回的那一条（`EntryMetadata`）住 `../history-reads`（本机后端 `history-annotate`）。 */

/** 组内会话排序模式（顶层布局固定按工作目录分组，不是 sort 选项）。 */
type SortMode = "updated_desc" | "started_desc";


/** issue #6: 历史浏览器的两种模式 —— 项目树过滤 vs 内容全文搜索。 */
type SearchMode = "tree" | "fulltext";

/**
 * F96：动作 run 的运行时上下文 = 纯判定 ctx（`HistoryActionCtx`）+ **活的 entry/project 引用**。
 * star/hide/delete 要 mutate 渲染用的同一 `e`/`proj` 对象并同步缓存，故必须是活引用（非拷贝）；
 * 搜索卡片（F85）无 entry/project → 只 resume/new-session（enabled 由 `hasEntry` 判定）。
 */
type RowActionCtx = HistoryActionCtx & {
  entry?: HistorySessionEntry;
  project?: HistoryProject;
  /** A4：非空 = 用指定账号 resume/起会话（远端注入其 CLAUDE_CONFIG_DIR + 记 lastAccount）。 */
  account?: string;
};

export class HistoryView {
  /**
   * 〔FW1 · 第四波 4D · D-e〕「这个会话此刻在 tab 栏里活着吗」—— `main.ts` 装成 `TabManager.isSessionLive`。
   * 缺省答「不活」：没装的时候只看条目自己那一格（判据与独立用法都不必带一个 TabManager）。
   */
  liveInTabs: (sid: string) => boolean = () => false;
  /** fixed overlay 根；open 时挂 document.body，close 时 remove。 */
  private root: HTMLElement;

  /** 项目级数据，初次 open 拉一次（= 本地批 + 远端批缓存合并的派生视图） */
  private projects: HistoryProject[] = [];
  /**
   * F76（#46）：远端「来源列表」缓存，跨 close/open 常驻单例。远端 fan-out 贵
   * （`remote_history.rs` 每台独立 SSH 连接、30s 超时），TTL 内 reopen 复用不重连；
   * 本地批便宜（<100ms）不缓存、每次 `refresh` 重扫。`null` = 从未成功抓过远端。
   * 刷新按钮 = 强制失效（`refresh(true)` 清此缓存重 fan-out）。
   */
  private remoteCache: RemoteSourceCache<HistoryProject> | null = null;
  /** F76：`refresh()` 的代际号，防并发/交叠 refresh 的旧结果覆盖新结果（对齐 ftSeq）。 */
  private refreshSeq = 0;
  /** project_dir → 已加载的会话详情 */
  private sessionCache = new Map<string, HistorySessionEntry[]>();
  /** project_dir → 当前正在加载中的 Promise，防重复触发 */
  private loadingProjects = new Map<string, Promise<void>>();

  private filter = "";
  private sort: SortMode = "updated_desc";
  private showHidden = false;
  private isOpen = false;
  /** "全量加载" 按钮按过后置 true；之后搜索可命中 session 内容（ai-title/excerpt 等） */
  private loadedAll = false;
  /** 全量加载并发上限（控制对后端 IPC 的瞬时压力） */
  private static readonly LOAD_ALL_CONCURRENCY = 4;

  // ===== audit-0805 F07 下半（报告 B-6 前四环）：三个放大器 =====
  //
  // 这三件互相叠乘，所以放在一起改：
  //   ① 搜索框 `input` **无防抖** ⇒ 每敲一个字符走一遍下面两条；
  //   ② `renderList` **自我扇出** —— `appendProjectGroup` 里 `.then(() => this.renderList())`
  //      写在 per-project 循环里，搜索激活时 `expanded` 恒 true ⇒ **P 个未缓存项目各触发一次
  //      完整 renderList**，而每次 renderList 又会再走一遍这个循环；
  //   ③ 那 P 个 `loadProjectSessions` **无并发上限** ⇒ 一次按键 P 条 IPC 齐发（远端项目还含 SSH）。
  //
  // ⇒ 一次按键的代价是 ①×②×③ 相乘，而不是相加。三条各自的判据见
  // `history-fanout.vitest.ts`。
  /** ① 搜索输入去抖句柄。 */
  private searchDebounce: ReturnType<typeof setTimeout> | null = null;
  /** ① 去抖窗口。250ms 照 `views/panorama.ts:382` 的现成范式，不另发明一个数。 */
  private static readonly SEARCH_DEBOUNCE_MS = 250;
  /** ② 「已排程重画」去重位。范式取自 `tabs.ts:713-737`（schedule-once），不是 `:2714` 那个裸 rAF。 */
  private renderScheduled = false;
  /** ③ 懒加载队列 + 在飞计数 + 去重集（同一项目不重复入队）。 */
  private lazyQueue: HistoryProject[] = [];
  private lazyActive = 0;
  private lazyQueued = new Set<string>();
  /** 入队时登记的「这一项加载完之后要做的事」（`details` 展开要重画自己的 body）。 */
  private lazyDone = new Map<string, () => void>();
  /** 等「队列抽干」的人。`loadAll` / 全展开都靠它拿完成信号。 */
  private lazyIdle: (() => void)[] = [];
  /** 判据用：合并之后**真正**跑了几次 renderList / 起了几条 load。 */
  private fanoutStats = { renders: 0, loads: 0, peakConcurrent: 0 };

  // issue #6: 全文搜索状态
  /** 当前模式：项目树过滤 / 内容全文搜索。默认树。 */
  private searchMode: SearchMode = "tree";
  /** 全文搜索是否附带搜 tool 内容（默认否，只搜 user/assistant 文本）。 */
  private includeTools = false;
  /** 全文搜索范围：全部 / 只我的输入(user) / 只 Claude(assistant)。 */
  private searchScope: "all" | "user" | "assistant" = "all";
  /** 全文搜索时间范围下界（epoch ms）；null = 不限。 */
  private searchAfterMs: number | null = null;
  /** 当前全文搜索请求的代际号，防止旧请求的结果覆盖新请求（竞态）。 */
  private ftSeq = 0;

  // 子元素
  private listEl!: HTMLElement;
  private searchInput!: HTMLInputElement;
  private statusEl!: HTMLElement;
  /** 列表模式的工具条+列表整体（切到查看器时整块隐藏） */
  private listShell!: HTMLElement;
  /** issue #6: 全文搜索结果容器（fulltext 模式显示，替代项目树） */
  private resultsEl!: HTMLElement;
  /** 仅树模式显示的工具条控件（sort / 展开 / 全量 / 隐藏 / 刷新） */
  private treeOnlyEls: HTMLElement[] = [];
  /** 仅全文模式显示的工具条控件（含工具内容 / 重新索引） */
  private fulltextOnlyEls: HTMLElement[] = [];
  /** 模式切换按钮 ref（更新 is-active） */
  private modeBtns: Partial<Record<SearchMode, HTMLButtonElement>> = {};
  /** "全量加载" 按钮 ref；加载中要 disable */
  private loadAllBtn!: HTMLButtonElement;
  /** 当前打开的会话查看器（点击条目进入只读视图）；null = 列表模式 */
  private viewer: SessionViewer | null = null;
  /** project_dir → 用户展开状态。默认折叠；用户主动展开的记下来 */
  private expandedProjects = new Set<string>();
  /**
   * F02 多机 #30 → F86(#45)：来源大区折叠**偏好覆盖表**（key=origin ?? ""）。**跨重启持久**。
   * 三态：键缺失 = 无偏好 → 走默认（本地展开 / 远端折叠，见 `defaultOriginOpen`）；键存在 =
   * 用户显式设过的 open 态。取代原「二态 collapsedOrigins Set」——Set 表达不了「远端默认折叠、但
   * 这台用户显式展开过」（显式展开与从没表态都=不在集合里）。仅在 >1 origin 时有分组大区。
   */
  private originOpenOverrides: OriginOpenOverrides = loadOriginOpenOverrides();
  /**
   * F03 多机 #30 → F86(#45)：被**隐藏**的来源 key（origin ?? ""）。**跨重启持久**（照 expandedForks
   * 先例）。chip 点掉某来源 → 加入此集并存盘 → 该来源项目不渲染。仅在 >1 origin 时显示筛选条。
   */
  private hiddenOrigins: Set<string> = loadHiddenOrigins();
  /** F03：来源筛选 chip 行（插在 statusEl 与 listEl 间；≤1 来源时 display:none）。 */
  private originFilterBar!: HTMLElement;
  /**
   * issue #12: fork 树展开状态。session_id ∈ 集合 = 该 session 的 children 展开。
   * **默认折叠** —— 第一次见到 fork 父节点时它的 children 不显示。从 localStorage 恢复。
   */
  private expandedForks: Set<string> = loadExpandedForks();
  /** F96：当前打开的条目右键菜单（单例；开新菜单/点空白/Esc 前先关它）。 */
  private openEntryMenu: HTMLElement | null = null;
  /** F96：菜单的 document 级关闭监听（pointerdown/keydown 共用一个）；closeEntryMenu 反注册。 */
  private entryMenuClose: ((ev: Event) => void) | null = null;

  constructor() {
    this.root = this.build();
    // F76b(#46):从 localStorage hydrate 远端来源快照——让每次启动**首开**也暖(不再只本地)。
    // 逐元素防脏 + loadedAt 归 0(首帧暖绘、首开必刷)见 loadPersistedRemoteCache;会话内 30s 内存 TTL 照旧管 reopen。
    this.remoteCache = loadPersistedRemoteCache();
  }

  async open(): Promise<void> {
    if (this.isOpen) return;
    // v2.5+: 挂 document.body 作为 fixed overlay（详 .history-view CSS 注释）。
    // 不再接管 streamRoot —— history 打开期间 TabManager 仍可正常 ensureTab。
    document.body.appendChild(this.root);
    this.isOpen = true;
    this.searchInput.value = "";
    this.filter = "";
    // issue #6: 每次打开复位到项目树模式（清掉上次的全文结果）
    this.searchMode = "tree";
    this.resultsEl.replaceChildren();
    this.updateModeUI();
    this.closeViewer();
    // 重新打开时清掉会话详情缓存（避免文件已被外部改动后展示陈旧数据）。
    // F76（#46）：**远端「来源列表」缓存 `remoteCache` 刻意不清**——它跨 open 常驻、由
    // `refresh(false)` 的 TTL 门控复用，正是「不再每次重连所有远端」的关键；刷新按钮走
    // `refresh(true)` 才强制失效。session 详情层仍每次清（#46 只讲来源列表，非会话详情）。
    this.sessionCache.clear();
    this.loadingProjects.clear();
    this.loadedAll = false;
    this.updateSearchPlaceholder();
    // issue #5: 注册到 dispatcher 弹层栈，Esc 由 dispatcher 派给我
    dispatcher.pushOverlay(this);
    await this.refresh(); // F76：默认 force=false → TTL 内复用远端缓存
    this.searchInput.focus();
  }

  /** 根据当前模式 / 是否已全量加载更新搜索框 placeholder，告知用户搜索覆盖范围 */
  private updateSearchPlaceholder(): void {
    if (this.searchMode === "fulltext") {
      this.searchInput.placeholder =
        copyText("history.search.placeholderFulltext");
      return;
    }
    if (this.loadedAll) {
      this.searchInput.placeholder = copyText("history.search.placeholderLoaded");
    } else {
      this.searchInput.placeholder =
        copyText("history.search.placeholderTree");
    }
  }

  close(): void {
    if (!this.isOpen) return;
    // ★ audit-0805 F14：关掉视图时递增代际号 —— 还在路上的那一次全文搜索回来时 `seq !== this.ftSeq`，
    //   不把结果写进已 detach 的 DOM。〔LOC1b · 4D〕F14 当年要掐的那条 1 秒重试链（等本机索引建好）随本机内存索引删了，
    //   这一行留下来管的是「在飞的那一问」。
    this.ftSeq++;
    this.closeViewer();
    this.closeEntryMenu(); // F96：菜单挂 document.body（不在 root 内），销毁视图须显式清，防 DOM+监听器泄漏
    this.root.remove();
    this.isOpen = false;
    dispatcher.popOverlay(this);
  }

  /** 打开只读查看器，列表 UI 临时隐藏 */
  private openViewer(entry: HistorySessionEntry): void {
    const displayTitle =
      entry.customTitle ??
      entry.aiTitle ??
      entry.firstUserExcerpt ??
      entry.sessionId.slice(0, 8);
    const proj = entry.projectName || entry.projectPath || copyText("history.project.unknown");
    const subtitle =
      entry.projectPath && entry.projectPath !== proj
        ? `${proj}  ·  ${entry.projectPath}`
        : proj;
    this.openViewerWith({
      jsonlPath: entry.jsonlPath,
      displayTitle,
      subtitle,
      origin: originFromWire(entry.origin),
      cwd: entry.projectPath, // F62：建分支后 resume 用作新终端起始目录
    });
  }

  /** issue #6: 通用打开查看器（树条目 / 搜索命中共用）。可带 scrollToUuid 定位。 */
  private openViewerWith(opts: ViewerOptions): void {
    if (this.viewer) this.viewer.dispose();
    this.viewer = new SessionViewer(() => this.closeViewer());
    this.root.appendChild(this.viewer.element);
    this.listShell.style.display = "none";
    void this.viewer.load(opts);
  }

  private closeViewer(): void {
    if (!this.viewer) return;
    this.viewer.dispose();
    this.viewer.element.remove();
    this.viewer = null;
    this.listShell.style.display = "";
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  /** Esc 优先级：F96 右键菜单 > 查看器 > 整个历史视图。main.ts / overlay dispatcher 调本方法。
   *  ★ 菜单优先必须在这里判——菜单挂 document 冒泡相 keydown，而 overlay dispatcher 挂 window
   *  捕获相（恒先触发），单靠菜单自己的监听拦不住「Esc 关菜单」，会误关整个历史视图。 */
  handleEscape(): void {
    if (this.openEntryMenu) {
      this.closeEntryMenu();
    } else if (this.viewer) {
      this.closeViewer();
    } else {
      this.close();
    }
  }

  /** issue #5 OverlayHandle 接口：跟 handleEscape 同一行为 */
  handleEsc(): void {
    this.handleEscape();
  }

  /**
   * F76（#46）：本地「来源」始终重扫（便宜、防陈旧），远端 fan-out 走 TTL 缓存
   * （`remote_history.rs` 每台独立 SSH、30s 超时，是「每次重新加载」的痛点单点）。
   * @param force `true`（刷新按钮）= 无视 TTL 必重 fan-out；`false`（open）= TTL 内复用远端快照。
   *
   * ★ 承重不变式：`this.projects` 的远端段元素与 `this.remoteCache.projects` 是**同一批对象引用**
   *   （浅 spread）。好处：对远端 proj 的**字段** mutation（star/hide/`sessionCount--`）天然同步进缓存、
   *   与后端持久化一致。代价：**数组结构性移除**（删空项目的 filter）必须两边都做，否则缓存留幽灵——
   *   见 delete handler。若将来把缓存写成 `remote.map(clone)`/深拷贝，这条隐式同步会静默失效。
   */
  private async refresh(force = false): Promise<void> {
    // 代际守卫：并发/交叠的 refresh（如 open 的 fan-out 在飞、用户又点刷新）只让最新一次落地，
    // 避免先发后回的旧结果用更旧数据 + 更小 loadedAt 覆盖新结果（对齐同文件 ftSeq 模式）。
    const seq = ++this.refreshSeq;
    this.statusEl.textContent = copyText("history.refresh.loadingProjects");
    this.listEl.replaceChildren();
    this.sessionCache.clear();
    this.loadingProjects.clear();
    // 本地批：每次重扫。失败即整体失败（本地都读不了没得显示）。
    let local: HistoryProject[];
    try {
      // 〔C4d〕问本机常驻后端（它并注解、判活、合成 Codex 项目）；注解没并上 ⇒ 说一声（星标 / 隐藏数显示成「不知道」）。
      const got = await fetchLocalProjects();
      local = got.projects;
      if (got.notice) showActionFailureToast(copyText("history.refresh.noticeTitle"), got.notice);
    } catch (e) {
      if (seq === this.refreshSeq) this.statusEl.textContent = copyText("history.refresh.failed", { reason: historyReasonOf(e) });
      return;
    }
    if (seq !== this.refreshSeq) return; // 被更新的 refresh 抢占
    // 先用「本地 + 已有远端缓存」渲染一帧（远端缓存命中时这就是最终态）。
    this.projects = [...local, ...(this.remoteCache?.projects ?? [])];
    this.renderList();

    // 远端批：TTL 门控。缓存新鲜且非强刷 → 上面那帧已含缓存，直接返回不发 IPC。
    if (!force && !shouldRefetchRemote(this.remoteCache, Date.now(), HISTORY_REMOTE_TTL_MS)) {
      return;
    }
    // 需 fan-out：独立 try——远端连不上/无配置不影响本地浏览。
    try {
      // 〔C4d〕逐台问本机后端（它沿池里那条 SSH 去问那台、并上本机的注解）；fan-out 住 `../history-reads`。
      const res = await fetchRemoteProjects();
      if (seq !== this.refreshSeq) return; // 抢占：丢弃过期结果
      const remote = res.projects;
      if (res.failedHosts.length === 0) {
        // 全部台成功 → 缓存这份完整快照，TTL 内复用（空结果也缓存，无远端配置时省掉每次 IPC）。
        this.remoteCache = { projects: remote, loadedAt: Date.now() };
        safeSetJson(LS_KEYS.historyRemoteSources, this.remoteCache); // F76b(#46):持久化 → 下次启动首开暖绘
      } else {
        // 部分台失败（后端语义：任一台成功即 Ok，失败台跳过）→ 这份快照**不完整**，不冻结缓存，
        // 下次 open 重试失败台（对齐 F76 前「每次 open 重扫、瞬断下次自愈」），仍渲染已拿到的台。
        this.remoteCache = null;
        safeRemove(LS_KEYS.historyRemoteSources); // F76b(#46):不完整快照不持久(免下次启动暖绘残缺列表)
        showActionFailureToast(
          copyText("history.refresh.partialTitle"),
          copyText("history.refresh.partialHosts", { hosts: res.failedHosts.join(copyText("history.refresh.hostSep")) }),
        );
      }
      this.projects = [...local, ...remote];
      if (this.isOpen) this.renderList(); // await 期间可能已关闭，别渲染进游离 DOM
    } catch (e) {
      if (seq !== this.refreshSeq) return;
      // 全部台失败（Err）→ 保住旧缓存不覆盖（force 也不预清，失败时降级复用更稳）；本地已渲染，仅 toast。
      showActionFailureToast(copyText("history.refresh.remoteFailed"), String(e));
    }
  }

  /**
   * 懒加载某个项目下的会话详情。重复调返回同一个 Promise。
   *
   * 〔C4d · 第四波 4B〕问本机常驻后端 `history-sessions`，一次交全（从前 issue #12 那条逐条流式的 Tauri Channel 退役：
   * 本机那一支原是 monitor 进程内自己边扫边发；join 进了本机后端之后本机与远端同一条路、同一份口径）。
   */
  private loadProjectSessions(proj: HistoryProject): Promise<void> {
    const key = projectKey(proj);
    const inFlight = this.loadingProjects.get(key);
    if (inFlight) return inFlight;
    if (this.sessionCache.has(key)) return Promise.resolve();

    const entries: HistorySessionEntry[] = [];
    this.sessionCache.set(key, entries);

    // 〔C4d · 第四波 4B〕本机与远端同一条路：问本机常驻后端 `history-sessions`（远端那台由它沿 SSH 去问、并上本机的注解）。
    //   从前本机那条是 Tauri Channel **逐条流式**（monitor 自己边扫边发）—— 今天一次交全（后端扫完整个项目才回），
    //   大项目「首条 < 100ms 出现」那一格没了（如实登记在 `C4d.md`），换来本机与远端同一份口径（fork 树 · 标题 · 摘录）。
    const p = (async () => {
      try {
        const { sessions, notice } = await fetchSessions(proj);
        entries.push(...sessions);
        if (notice) console.warn(`[history] ${key}：${notice}`);
        if (this.isOpen) this.renderList();
      } catch (e) {
        console.warn(`sessions in ${key} failed:`, e);
        showActionFailureToast(proj.origin ? copyText("history.sessions.remoteFailed") : copyText("history.sessions.failed"), historyReasonOf(e));
      } finally {
        this.loadingProjects.delete(key);
      }
    })();
    this.loadingProjects.set(key, p);
    return p;
  }

  // === DOM 构建 ===

  private build(): HTMLElement {
    const view = document.createElement("div");
    view.className = "history-view";

    this.listShell = document.createElement("div");
    this.listShell.className = "history-list-shell";
    view.appendChild(this.listShell);

    const bar = document.createElement("div");
    bar.className = "history-bar";

    const backBtn = document.createElement("button");
    backBtn.type = "button";
    backBtn.className = "history-back";
    backBtn.textContent = copyText("history.build.back");
    backBtn.addEventListener("click", () => this.close());
    bar.appendChild(backBtn);

    // issue #6: 模式切换（项目 / 全文）
    const modeToggle = document.createElement("div");
    modeToggle.className = "history-mode-toggle";
    const mkModeBtn = (mode: SearchMode, label: string, title: string) => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "history-mode-btn";
      b.textContent = label;
      b.title = title;
      b.addEventListener("click", () => this.setMode(mode));
      this.modeBtns[mode] = b;
      modeToggle.appendChild(b);
    };
    mkModeBtn("tree", copyText("history.build.modeTree"), copyText("history.build.modeTreeHint"));
    mkModeBtn("fulltext", copyText("history.build.modeFulltext"), copyText("history.build.modeFulltextHint"));
    bar.appendChild(modeToggle);

    this.searchInput = document.createElement("input");
    this.searchInput.type = "search";
    this.searchInput.className = "history-search";
    // placeholder 在 updateSearchPlaceholder 里根据模式 / loadedAll 动态设
    this.searchInput.addEventListener("input", () => {
      // F07：**去抖**。此前每敲一个字符都同步走一遍 `renderList()`，而 `renderList` 自己
      // 还会扇出 P 条懒加载、每条回来再触发一次 renderList —— 三个放大器叠乘。
      if (this.searchDebounce !== null) clearTimeout(this.searchDebounce);
      this.searchDebounce = setTimeout(() => {
        this.searchDebounce = null;
        if (this.searchMode === "tree") {
          this.filter = this.searchInput.value.trim().toLowerCase();
          this.renderList();
        } else if (this.searchInput.value.trim() === "") {
          // 全文模式清空 → 清结果
          this.runFullTextSearch();
        }
      }, HistoryView.SEARCH_DEBOUNCE_MS);
    });
    this.searchInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && this.searchMode === "fulltext") {
        e.preventDefault();
        this.runFullTextSearch();
      }
    });
    bar.appendChild(this.searchInput);

    const sortSel = document.createElement("select");
    sortSel.className = "history-sort";
    sortSel.title = copyText("history.build.sortHint");
    const options: { value: SortMode; label: string }[] = [
      { value: "updated_desc", label: copyText("history.build.sortUpdated") },
      { value: "started_desc", label: copyText("history.build.sortCreated") },
    ];
    for (const o of options) {
      const opt = document.createElement("option");
      opt.value = o.value;
      opt.textContent = o.label;
      sortSel.appendChild(opt);
    }
    sortSel.addEventListener("change", () => {
      this.sort = sortSel.value as SortMode;
      this.renderList();
    });
    bar.appendChild(sortSel);
    this.treeOnlyEls.push(sortSel);

    const expandAllBtn = document.createElement("button");
    expandAllBtn.type = "button";
    expandAllBtn.className = "history-refresh";
    expandAllBtn.textContent = copyText("history.build.expandAll");
    expandAllBtn.title = copyText("history.build.expandAllHint");
    expandAllBtn.addEventListener("click", () => void this.toggleAll());
    bar.appendChild(expandAllBtn);
    this.treeOnlyEls.push(expandAllBtn);

    // 全量加载：把每个项目的 session 详情都拉到缓存，让搜索可命中 session 内容
    this.loadAllBtn = document.createElement("button");
    this.loadAllBtn.type = "button";
    this.loadAllBtn.className = "history-refresh";
    this.loadAllBtn.textContent = copyText("history.loadAll.action");
    this.loadAllBtn.title =
      copyText("history.build.loadAllHint");
    this.loadAllBtn.addEventListener("click", () => void this.loadAllSessions());
    bar.appendChild(this.loadAllBtn);
    this.treeOnlyEls.push(this.loadAllBtn);

    const hiddenLabel = document.createElement("label");
    hiddenLabel.className = "history-toggle";
    const hiddenCheck = document.createElement("input");
    hiddenCheck.type = "checkbox";
    hiddenCheck.addEventListener("change", () => {
      this.showHidden = hiddenCheck.checked;
      this.renderList();
    });
    hiddenLabel.appendChild(hiddenCheck);
    const hiddenText = document.createElement("span");
    hiddenText.textContent = copyText("history.build.showHidden");
    hiddenLabel.appendChild(hiddenText);
    bar.appendChild(hiddenLabel);
    this.treeOnlyEls.push(hiddenLabel);

    const refreshBtn = document.createElement("button");
    refreshBtn.type = "button";
    refreshBtn.className = "history-refresh";
    refreshBtn.textContent = copyText("history.build.refresh");
    // F76（#46）：刷新按钮 = 强制失效，无视 TTL 重新 fan-out 所有远端（保留用户主动强刷语义）。
    refreshBtn.addEventListener("click", () => void this.refresh(true));
    bar.appendChild(refreshBtn);
    this.treeOnlyEls.push(refreshBtn);

    // issue #6: 全文模式专属控件 —— "含工具内容" 复选框 + "重新索引"
    const toolsLabel = document.createElement("label");
    toolsLabel.className = "history-toggle";
    toolsLabel.title =
      copyText("history.build.includeToolsHint");
    const toolsCheck = document.createElement("input");
    toolsCheck.type = "checkbox";
    toolsCheck.addEventListener("change", () => {
      this.includeTools = toolsCheck.checked;
      if (this.searchInput.value.trim() !== "") this.runFullTextSearch();
    });
    toolsLabel.appendChild(toolsCheck);
    const toolsText = document.createElement("span");
    toolsText.textContent = copyText("history.build.includeTools");
    toolsLabel.appendChild(toolsText);
    bar.appendChild(toolsLabel);
    this.fulltextOnlyEls.push(toolsLabel);

    // 搜索范围：全部 / 只我的输入 / 只 Claude
    const scopeSel = document.createElement("select");
    scopeSel.className = "history-sort";
    scopeSel.title = copyText("history.build.scopeHint");
    for (const o of [
      { value: "all", label: copyText("history.build.scopeAll") },
      { value: "user", label: copyText("history.build.scopeUser") },
      { value: "assistant", label: copyText("history.build.scopeClaude") },
    ]) {
      const opt = document.createElement("option");
      opt.value = o.value;
      opt.textContent = o.label;
      scopeSel.appendChild(opt);
    }
    scopeSel.addEventListener("change", () => {
      this.searchScope = scopeSel.value as "all" | "user" | "assistant";
      if (this.searchInput.value.trim() !== "") this.runFullTextSearch();
    });
    bar.appendChild(scopeSel);
    this.fulltextOnlyEls.push(scopeSel);

    // 时间范围：全部 / 近 7 天 / 近 30 天
    const timeSel = document.createElement("select");
    timeSel.className = "history-sort";
    timeSel.title = copyText("history.build.timeHint");
    for (const o of [
      { value: "0", label: copyText("history.build.timeAll") },
      { value: "7", label: copyText("history.build.time7d") },
      { value: "30", label: copyText("history.build.time30d") },
    ]) {
      const opt = document.createElement("option");
      opt.value = o.value;
      opt.textContent = o.label;
      timeSel.appendChild(opt);
    }
    timeSel.addEventListener("change", () => {
      const days = Number(timeSel.value);
      this.searchAfterMs = days > 0 ? Date.now() - days * 86_400_000 : null;
      if (this.searchInput.value.trim() !== "") this.runFullTextSearch();
    });
    bar.appendChild(timeSel);
    this.fulltextOnlyEls.push(timeSel);

    // 〔LOC1b · 4D〕「重新索引」按钮随本机内存索引一起删了：本机也问本机后端，每次现扫、没有可重建的东西。

    this.listShell.appendChild(bar);

    this.statusEl = document.createElement("div");
    this.statusEl.className = "history-status";
    this.listShell.appendChild(this.statusEl);

    // F03 多机 #30：来源筛选条（仅 >1 来源时显示，由 renderOriginFilter 控制）
    this.originFilterBar = document.createElement("div");
    this.originFilterBar.className = "history-origin-filter";
    this.originFilterBar.style.display = "none";
    this.listShell.appendChild(this.originFilterBar);

    this.listEl = document.createElement("div");
    this.listEl.className = "history-list";
    this.listShell.appendChild(this.listEl);

    // issue #6: 全文搜索结果容器（默认隐藏，fulltext 模式显示）
    this.resultsEl = document.createElement("div");
    this.resultsEl.className = "history-search-results";
    this.resultsEl.style.display = "none";
    this.listShell.appendChild(this.resultsEl);

    // 首次构造时给一份合理 placeholder（open() 里会再刷新一次以反映 loadedAll）
    this.updateSearchPlaceholder();
    this.updateModeUI();

    return view;
  }

  // === issue #6: 全文搜索 ===

  /** 切换 项目树 / 全文 模式。 */
  private setMode(mode: SearchMode): void {
    if (this.searchMode === mode) return;
    this.searchMode = mode;
    this.updateModeUI();
    if (mode === "tree") {
      // 回树模式：用当前输入作过滤词重画
      this.filter = this.searchInput.value.trim().toLowerCase();
      this.renderList();
    } else {
      // 进全文模式：有词就立刻搜，否则显示索引状态提示
      if (this.searchInput.value.trim() !== "") {
        this.runFullTextSearch();
      } else {
        this.showIndexIdleHint();
      }
    }
    this.searchInput.focus();
  }

  /** 按当前模式更新工具条控件可见性 + 列表/结果容器显隐 + placeholder。 */
  private updateModeUI(): void {
    const isTree = this.searchMode === "tree";
    for (const [m, b] of Object.entries(this.modeBtns)) {
      b?.classList.toggle("is-active", m === this.searchMode);
    }
    for (const el of this.treeOnlyEls) el.style.display = isTree ? "" : "none";
    for (const el of this.fulltextOnlyEls) el.style.display = isTree ? "none" : "";
    this.listEl.style.display = isTree ? "" : "none";
    this.resultsEl.style.display = isTree ? "none" : "";
    this.updateSearchPlaceholder();
  }

  /** 全文模式但无关键词时给个提示。〔LOC1b · 4D〕本机没有索引了 ⇒ 不再有「已索引 N 个 / 构建中」那两态。 */
  private showIndexIdleHint(): void {
    this.resultsEl.replaceChildren();
    this.statusEl.textContent = copyText("history.indexHint.idle");
  }

  /**
   * 执行全文搜索。竞态防护：每次调用递增 ftSeq，异步结果回来时若 seq 已过期则丢弃。
   * 〔LOC1b · 4D〕「索引未就绪 ⇒ 显示进度并每秒重试」那一支随本机内存索引删了（本机也问本机后端，没有「索引中」）。
   */
  private async runFullTextSearch(): Promise<void> {
    const query = this.searchInput.value.trim();
    const seq = ++this.ftSeq;
    if (query === "") {
      this.resultsEl.replaceChildren();
      this.showIndexIdleHint();
      return;
    }
    this.statusEl.textContent = copyText("history.search.searching");
    try {
      // 本机 ＋ 各台远端，逐台经通道说 `history-search`（〔LOC1b〕本机也是），合并在前端（`history-search.ts`）。
      const resp = await searchAllMachines({
        query,
        includeTools: this.includeTools,
        scope: this.searchScope,
        afterMs: this.searchAfterMs,
        limit: 300,
      });
      if (seq !== this.ftSeq || this.searchMode !== "fulltext") return; // 过期 / 已切模式
      this.renderSearchResults(resp, query);
    } catch (e) {
      if (seq !== this.ftSeq) return;
      this.statusEl.textContent = copyText("history.search.failed", { e: String(e) });
    }
  }

  // 〔LOC1b · 4D〕`INDEX_WAIT_MAX_TICKS` 与 `waitForIndexThenSearch`〔散文墓碑〕删了：它们等的是本机内存索引建好
  //   （audit-0805 F14 那条「只问本地索引状态、不再每秒重跑整条搜索」的 1 秒链），本机也改问本机后端之后没有「索引中」这一态。

  private renderSearchResults(resp: SearchResult, query: string): void {
    this.resultsEl.replaceChildren();
    // K-R100：`truncated` 现在**本地与每一台远端都算**（收口前它只装本地那一半，
    // 于是远端截断在这一行上一个字不说）。措辞也改准：被砍掉的是 **snippet**，
    // 不是命中 —— `totalHits` 一直报的是全量。
    const starved = resp.sessions.filter((x) => x.hitsTruncated).length;
    const { totalHits, sessionCount } = resp;
    this.statusEl.textContent = !resp.truncated
      ? copyText("history.search.summary", { query, totalHits, sessionCount })
      : starved > 0
        ? copyText("history.search.summaryStarved", { query, totalHits, sessionCount, starved })
        : copyText("history.search.summaryTruncated", { query, totalHits, sessionCount });
    if (resp.sessions.length === 0) {
      this.resultsEl.appendChild(makeStatusRow(copyText("history.search.noMatch")));
      return;
    }
    for (const s of resp.sessions) {
      this.resultsEl.appendChild(this.buildSearchSession(s));
    }
  }

  private buildSearchSession(s: SearchSessionHits): HTMLElement {
    const group = document.createElement("div");
    group.className = "search-session";

    const header = document.createElement("div");
    header.className = "search-session-header";
    // issue #28：远端命中带 `[host]` 来源标识（本地无前缀）。
    if (s.origin) {
      const host = document.createElement("span");
      host.className = "search-session-host";
      host.textContent = `[${s.origin}]`;
      host.title = copyText("history.searchSession.remoteHost", { origin: s.origin });
      header.appendChild(host);
    }
    const title = document.createElement("span");
    title.className = "search-session-title";
    title.textContent = s.title || s.sessionId.slice(0, 8);
    header.appendChild(title);
    const proj = document.createElement("span");
    proj.className = "search-session-project";
    proj.textContent = s.projectName || s.projectPath || "";
    proj.title = s.projectPath;
    header.appendChild(proj);
    const count = document.createElement("span");
    count.className = "search-session-count";
    count.textContent = copyText("history.searchSession.count", { hitCount: s.hitCount, time: formatTimestampSmart(s.updatedAt) });
    header.appendChild(count);
    // F85（#44）：搜索卡片直接 resume——复用 F96 的 `runResume`（hasEntry:false 的 ctx，
    // 只用 identity 段）。本地走 resume_history_session、远端走 runRemoteResume，尊重 F34 命令。
    const resume = document.createElement("button");
    resume.type = "button";
    resume.className = "search-session-resume";
    resume.textContent = copyText("history.resume.icon");
    resume.title = s.origin
      ? copyText("history.resume.remoteHint", { origin: s.origin })
      : copyText("history.resume.hint");
    // F85 + A4：搜索卡片 ctx（hasEntry:false，只用 identity 段）——resume 按钮与右键菜单共用。
    const cardCtx: RowActionCtx = {
      sessionId: s.sessionId,
      jsonlPath: s.jsonlPath,
      cwd: s.projectPath,
      origin: s.origin,
      hasEntry: false,
    };
    resume.addEventListener("click", (ev) => {
      ev.stopPropagation(); // 不冒泡触发卡片/命中的「点开 viewer」
      void this.runResume(cardCtx);
    });
    header.appendChild(resume);
    // A4：右键搜索卡片 → 同一套动作菜单（含「用账号 X resume」，远端 + 账号库可用时）。
    header.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      this.showEntryMenu(ev.clientX, ev.clientY, cardCtx);
    });
    group.appendChild(header);

    for (const hit of s.hits) {
      group.appendChild(this.buildSearchHit(s, hit));
    }
    // K-R100：`hitCount > hits.length` **不是一件事，是两件** ——
    //   · `hitsTruncated`  = 整份结果的 snippet 预算用完了（该说「缩小范围」）
    //   · 否则             = 这个会话话太多，只列前 30 条（点进去看就行）
    // 收口前两种同文案，而 `hits: []` 那一档更糟：卡片里**一条可点的行都没有**，
    // 文案却写着「点任意条打开会话查看全部」—— 指向一个不存在的东西。
    if (s.hitCount > s.hits.length) {
      const more = document.createElement("div");
      more.className = "search-hit-more";
      const rest = s.hitCount - s.hits.length;
      if (s.hitsTruncated) {
        more.classList.add("search-hit-more-truncated");
        more.textContent =
          s.hits.length === 0
            ? copyText("history.searchSession.allStarved", { hitCount: s.hitCount })
            : copyText("history.searchSession.moreStarved", { rest });
      } else {
        more.textContent = copyText("history.searchSession.moreCapped", { rest, shown: s.hits.length });
      }
      // 🔴 无论哪一种，这一行自己就能打开会话：`hits: []` 时它是**唯一**的入口。
      more.addEventListener("click", () => {
        this.openViewerWith({
          jsonlPath: s.jsonlPath,
          displayTitle: s.title || s.sessionId.slice(0, 8),
          subtitle: s.projectName
            ? `${s.projectName}  ·  ${s.projectPath}`
            : s.projectPath,
          origin: originFromWire(s.origin),
          cwd: s.projectPath,
        });
      });
      group.appendChild(more);
    }
    return group;
  }

  private buildSearchHit(s: SearchSessionHits, hit: SearchHit): HTMLElement {
    const row = document.createElement("div");
    row.className = "search-hit";

    const kind = document.createElement("span");
    kind.className = `search-hit-kind kind-${hit.kind}`;
    kind.textContent =
      hit.kind === "user" ? copyText("history.searchHit.you") : hit.kind === "assistant" ? "Claude" : copyText("history.searchHit.tool");
    row.appendChild(kind);

    // snippet：before + <mark>matched</mark> + after。全部用 textContent 防 XSS
    // （matched 是用户 / Claude 的原始内容，绝不能 innerHTML）。
    const snip = document.createElement("span");
    snip.className = "search-hit-snippet";
    snip.append(document.createTextNode(hit.before));
    const mark = document.createElement("mark");
    mark.textContent = hit.matched;
    snip.appendChild(mark);
    snip.append(document.createTextNode(hit.after));
    row.appendChild(snip);

    row.addEventListener("click", () => {
      this.openViewerWith({
        jsonlPath: s.jsonlPath,
        displayTitle: s.title || s.sessionId.slice(0, 8),
        subtitle: s.projectName
          ? `${s.projectName}  ·  ${s.projectPath}`
          : s.projectPath,
        scrollToUuid: hit.uuid,
        // issue #28：远端命中点击走那台的只读视图（origin → `stream_read_session_jsonl` 带 origin 那一条）。
        origin: originFromWire(s.origin),
        cwd: s.projectPath, // F62：本地命中建分支后 resume 用
      });
    });
    return row;
  }

  // 〔LOC1b · 4D〕`rebuildIndex`〔散文墓碑〕（「重新索引」按钮的动作）随本机内存索引删了。

  // === 列表渲染 ===

  private renderList(): void {
    this.fanoutStats.renders += 1;
    this.listEl.replaceChildren();
    this.renderOriginFilter(); // F03：同步来源筛选 chip 行
    if (this.projects.length === 0) {
      this.statusEl.textContent =
        copyText("history.list.empty");
      return;
    }

    // 项目过滤：搜索匹配（matchProject）+ F03 来源筛选（hiddenOrigins）正交叠加。
    // F86：隐藏筛选只在 >1 来源时生效——筛选 chip 行本身也只在 >1 来源时可见（renderOriginFilter）。
    // 持久化后，若不门控，「隐藏了唯一来源」会从「重启自愈的暂态」变成「无 chip 可复原的永久死锁」。
    const applyHidden = new Set(this.projects.map((p) => p.origin)).size > 1;
    const filteredProjects = this.projects.filter(
      (p) =>
        this.matchProject(p) &&
        (!applyHidden || !this.hiddenOrigins.has(p.origin ?? "")),
    );

    // 项目排序：live > starred > last_activity desc（与后端默认一致，前端不改）
    // `K-R92`：`Number(b.hasLive)` 在 `hasLive` 是 `null`（不知道）时得 0 —— 与
    // 「查过了，没有活会话」一模一样。改走三态档位：确定有 > 不知道 > 确定没有。
    const sorted = filteredProjects.slice().sort((a, b) => {
      const live = liveRank(b.hasLive) - liveRank(a.hasLive);
      if (live !== 0) return live;
      const star = starRank(b.starredCount) - starRank(a.starredCount);
      if (star !== 0) return star;
      return b.lastActivity - a.lastActivity;
    });

    const searchActive = this.filter.length > 0;
    const total = this.projects.reduce((n, p) => n + p.sessionCount, 0);
    const filteredTotal = sorted.reduce((n, p) => n + p.sessionCount, 0);
    this.statusEl.textContent =
      filteredTotal !== total
        ? copyText("history.list.summaryFiltered", { projects: sorted.length, sessions: filteredTotal, total })
        : copyText("history.list.summary", { projects: sorted.length, sessions: filteredTotal });

    // F02 多机 #30：是否分组取决于**存在**几个来源（this.projects），不随 F03 隐藏 / 搜索
    // 过滤而塌缩——否则隐藏到只剩 1 来源时分组结构会突然变扁平。被隐藏 / 过滤光的来源其
    // section 为空、跳过不渲染。distinct ≤1（通常纯本地）→ 扁平（零回归）。
    const allOrigins = [...new Set(this.projects.map((p) => p.origin))];
    if (allOrigins.length <= 1) {
      for (const proj of sorted) {
        this.appendProjectGroup(this.listEl, proj, searchActive);
      }
    } else {
      for (const origin of this.orderOrigins(allOrigins)) {
        const group = sorted.filter((p) => p.origin === origin);
        if (group.length === 0) continue; // 被 F03 隐藏 / 被搜索过滤光 → 不渲染空区
        this.listEl.appendChild(this.buildOriginGroup(origin, group, searchActive));
      }
    }
    // 全部被过滤 / 隐藏 → 列表空白，给一行提示（this.projects 非空但 sorted 空）。
    if (sorted.length === 0) {
      const hint = document.createElement("div");
      hint.className = "history-empty-hint";
      hint.textContent = copyText("history.list.noMatch");
      this.listEl.appendChild(hint);
    }
  }

  /** F02：把一个项目组（buildProjectGroup）挂到 parent，并在搜索激活时触发懒加载。 */
  private appendProjectGroup(
    parent: HTMLElement,
    proj: HistoryProject,
    searchActive: boolean,
  ): void {
    const expanded = searchActive || this.expandedProjects.has(projectKey(proj));
    parent.appendChild(this.buildProjectGroup(proj, expanded));
    if (searchActive && expanded && !this.sessionCache.has(projectKey(proj))) {
      // F07：原来是 `.then(() => this.renderList())` —— **P 个项目各触发一次全树重建**，
      // 而每次重建又会再走一遍本循环。改成入队：去重 + 有上限 + 批末合并重画一次。
      this.enqueueLazyLoad(proj);
    }
  }

  /**
   * F07：搜索触发的懒加载 —— **去重 + 有上限**。
   *
   * 同一个项目重复入队会被挡掉（`renderList` 会被反复调用，每次都想加载同一批未缓存项目）。
   */
  private enqueueLazyLoad(proj: HistoryProject, onDone?: () => void): void {
    const k = projectKey(proj);
    if (this.lazyQueued.has(k) || this.sessionCache.has(k)) return;
    this.lazyQueued.add(k);
    this.lazyQueue.push(proj);
    if (onDone) this.lazyDone.set(k, onDone);
    this.pumpLazy();
  }

  /** F07：把队列抽干，同时在飞不超过 `LOAD_ALL_CONCURRENCY`。 */
  private pumpLazy(): void {
    while (
      this.lazyActive < HistoryView.LOAD_ALL_CONCURRENCY &&
      this.lazyQueue.length > 0
    ) {
      const proj = this.lazyQueue.shift();
      if (!proj) break;
      this.lazyActive += 1;
      this.fanoutStats.loads += 1;
      this.fanoutStats.peakConcurrent = Math.max(
        this.fanoutStats.peakConcurrent,
        this.lazyActive,
      );
      void this.loadProjectSessions(proj).finally(() => {
        this.lazyActive -= 1;
        const k = projectKey(proj);
        this.lazyQueued.delete(k);
        const done = this.lazyDone.get(k);
        this.lazyDone.delete(k);
        done?.();
        this.scheduleRender();
        this.pumpLazy();
        if (this.lazyActive === 0 && this.lazyQueue.length === 0) {
          const waiters = this.lazyIdle;
          this.lazyIdle = [];
          for (const w of waiters) w();
        }
      });
    }
  }

  /** F07：等队列抽干。队列本来就空 ⇒ 立即 resolve。 */
  private lazyDrained(): Promise<void> {
    if (this.lazyActive === 0 && this.lazyQueue.length === 0) return Promise.resolve();
    return new Promise<void>((r) => this.lazyIdle.push(r));
  }

  /**
   * F07：**批末合并重画** —— 已排程就不重排（schedule-once）。
   *
   * ⚠ 范式取自 `tabs.ts:713-737`（`materializeScheduled` 布尔 + rIC），**不是** `:2714` 那个
   * 裸 rAF —— 后者没有去重位，连排多个 rAF（核实台账 I6′ 逐字记过这一格）。
   */
  private scheduleRender(): void {
    if (this.renderScheduled) return;
    this.renderScheduled = true;
    requestAnimationFrame(() => {
      this.renderScheduled = false;
      if (this.isOpen) this.renderList();
    });
  }

  /** F02：来源排序——本地（undefined）优先，远端按 label 字母序。 */
  private orderOrigins(origins: (string | undefined)[]): (string | undefined)[] {
    const remotes = origins
      .filter((o): o is string => o !== undefined)
      .sort((a, b) => a.localeCompare(b));
    return origins.some((o) => o === undefined) ? [undefined, ...remotes] : remotes;
  }

  /** F02 多机 #30：一个来源（本地 / 某远端 host）的可折叠大区，内含其项目组。 */
  private buildOriginGroup(
    origin: string | undefined,
    projects: HistoryProject[],
    searchActive: boolean,
  ): HTMLElement {
    const key = origin ?? "";
    const details = document.createElement("details");
    details.className = "history-origin-group";
    // F86：搜索激活强制展开 > 用户显式偏好 > 首见默认（本地展开 / 远端折叠）。
    details.open = resolveOriginOpen(this.originOpenOverrides[key], origin, searchActive);

    const header = document.createElement("summary");
    header.className = "history-origin-header";
    const indicator = document.createElement("span");
    indicator.className = "history-group-indicator";
    indicator.textContent = copyText("history.group.collapsedMark");
    header.appendChild(indicator);
    const name = document.createElement("span");
    name.className = "history-origin-name";
    name.textContent = origin ? `[${origin}]` : copyText("history.originGroup.local");
    header.appendChild(name);
    const stats = document.createElement("span");
    stats.className = "history-group-stats";
    const sessionTotal = projects.reduce((n, p) => n + p.sessionCount, 0);
    stats.textContent = copyText("history.originGroup.stats", { projects: projects.length, sessions: sessionTotal });
    header.appendChild(stats);
    details.appendChild(header);

    const body = document.createElement("div");
    body.className = "history-origin-body";
    for (const proj of projects) {
      this.appendProjectGroup(body, proj, searchActive);
    }
    details.appendChild(body);

    // F86：折叠偏好持久化（搜索激活时不写，避免污染用户偏好）。nextOverrides 只存偏离默认的项、
    // 回到默认就删键——故首见默认折叠若触发了这次程序化 toggle（宿主行为不定）也不会污染成偏好。
    details.addEventListener("toggle", () => {
      if (searchActive) return;
      const next = nextOverrides(this.originOpenOverrides, key, origin, details.open);
      // 仅当内容变化才存盘——挡掉宿主对默认展开大区程序化 open 变更的冗余 toggle 写放大。
      if (sameOverrides(next, this.originOpenOverrides)) return;
      this.originOpenOverrides = next;
      saveOriginOpenOverrides(next);
    });
    return details;
  }

  /** F03 多机 #30：来源筛选 chip 行。distinct origin ≤1 → 隐藏；否则每来源一个 chip。 */
  private renderOriginFilter(): void {
    const origins = [...new Set(this.projects.map((p) => p.origin))];
    if (origins.length <= 1) {
      this.originFilterBar.style.display = "none";
      this.originFilterBar.replaceChildren();
      return;
    }
    this.originFilterBar.style.display = "flex";
    this.originFilterBar.replaceChildren();
    const label = document.createElement("span");
    label.className = "history-origin-filter-label";
    label.textContent = copyText("history.originFilter.label");
    this.originFilterBar.appendChild(label);
    for (const origin of this.orderOrigins(origins)) {
      const key = origin ?? "";
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "history-origin-chip";
      chip.classList.toggle("active", !this.hiddenOrigins.has(key));
      chip.textContent = origin ? `[${origin}]` : copyText("history.originFilter.local");
      chip.title = origin ? copyText("history.originFilter.remoteHint", { origin }) : copyText("history.originFilter.localHint");
      chip.addEventListener("click", () => {
        if (this.hiddenOrigins.has(key)) this.hiddenOrigins.delete(key);
        else this.hiddenOrigins.add(key);
        saveHiddenOrigins(this.hiddenOrigins); // F86：来源筛选跨重启保持
        this.renderList();
      });
      this.originFilterBar.appendChild(chip);
    }
  }

  /** 单个项目组：collapsible header + 内嵌 session 列表（lazy 加载） */
  private buildProjectGroup(
    proj: HistoryProject,
    expanded: boolean,
  ): HTMLElement {
    const details = document.createElement("details");
    details.className = "history-group";
    details.open = expanded;

    const header = document.createElement("summary");
    header.className = "history-group-header";

    const indicator = document.createElement("span");
    indicator.className = "history-group-indicator";
    indicator.textContent = copyText("history.group.collapsedMark"); // ▸ 折叠指示符（[open] 时 CSS 旋转 90deg）
    header.appendChild(indicator);

    // 项目名前不再加 📁 emoji —— 折叠指示器 + 项目名已经够清，多余的图标视觉噪声

    const name = document.createElement("span");
    name.className = "history-group-name";
    name.textContent = proj.projectName || copyText("history.project.unknown");
    header.appendChild(name);

    // issue #16：远端项目组头加 [host] 徽标区分来源
    if (proj.origin) {
      const originBadge = document.createElement("span");
      originBadge.className = "history-origin-badge";
      originBadge.textContent = `[${proj.origin}]`;
      originBadge.title = copyText("history.projectGroup.remoteHint", { origin: proj.origin });
      header.appendChild(originBadge);
    }

    const pathLbl = document.createElement("span");
    pathLbl.className = "history-group-path";
    pathLbl.textContent = proj.projectPath;
    pathLbl.title = proj.projectPath;
    header.appendChild(pathLbl);

    const stats = document.createElement("span");
    stats.className = "history-group-stats";
    const chips: string[] = [copyText("history.projectGroup.sessions", { sessionCount: proj.sessionCount })];
    // `K-R92`：只在**算过了**的时候才说话。「不知道」这一档不出 chip ——
    // ⚠ 界面怎么把「不知道」显示出来（例如一个 `?` 徽标）是 `K-R66` 的面，本件不做；
    // 本件只保证这里不会拿一个没人查过的值去说「没有星标」「没有活会话」。
    if (isKnown(proj.hasLive) && proj.hasLive) chips.push(`● ${copyText("sessionState.live.name")}`);
    if (isKnown(proj.starredCount) && proj.starredCount > 0)
      chips.push(`★ ${proj.starredCount}`);
    if (this.showHidden && isKnown(proj.hiddenCount) && proj.hiddenCount > 0)
      chips.push(copyText("history.projectGroup.hidden", { hiddenCount: proj.hiddenCount }));
    chips.push(formatTimestampSmart(proj.lastActivity));
    stats.textContent = chips.join(copyText("history.projectGroup.chipSep"));
    header.appendChild(stats);

    details.appendChild(header);

    const body = document.createElement("div");
    body.className = "history-group-body";
    details.appendChild(body);

    // 渲染 body：根据缓存命中情况
    const renderBody = () => {
      body.replaceChildren();
      const cached = this.sessionCache.get(projectKey(proj));
      const isLoading = this.loadingProjects.has(projectKey(proj));
      if (cached === undefined) {
        // 还没开始加载（用户没展开过）
        body.appendChild(makeStatusRow(isLoading ? copyText("history.body.loading") : copyText("history.body.clickToLoad")));
        return;
      }
      const visible = cached
        .filter((e) => (this.showHidden ? true : !e.hidden))
        .filter((e) => this.matchSession(e));
      if (visible.length === 0) {
        // 流式加载初期 cache 可能是 [] —— 此时显示 "加载中" 而非 "无会话"
        if (isLoading) {
          body.appendChild(makeStatusRow(copyText("history.body.loading")));
        } else {
          body.appendChild(
            makeStatusRow(
              cached.length === 0
                ? copyText("history.body.empty")
                : copyText("history.body.noMatch"),
            ),
          );
        }
        return;
      }
      // issue #12: 项目内建 fork 树（child 缩进显示在 parent 下，可折叠）
      const roots = buildSessionTree(visible);
      this.sortTree(roots);
      // 迭代 DFS pre-order 输出（INVARIANT § 17: 不递归遍历用户数据）
      const stack: Array<{ node: SessionTreeNode; depth: number }> = [];
      for (let i = roots.length - 1; i >= 0; i--) {
        stack.push({ node: roots[i], depth: 0 });
      }
      while (stack.length > 0) {
        const { node, depth } = stack.pop()!;
        body.appendChild(
          this.buildEntryRow(node.entry, proj, depth, node.children.length, node.orphan),
        );
        if (node.children.length > 0 && this.expandedForks.has(node.entry.sessionId)) {
          for (let i = node.children.length - 1; i >= 0; i--) {
            stack.push({ node: node.children[i], depth: depth + 1 });
          }
        }
      }
      // 流式加载未完成时，在已渲染条目下方加 "继续加载中…" 提示
      if (isLoading) {
        body.appendChild(makeStatusRow(copyText("history.body.loadingMore")));
      }
    };

    // 跟踪展开状态 + 触发懒加载
    details.addEventListener("toggle", () => {
      if (details.open) {
        this.expandedProjects.add(projectKey(proj));
        if (!this.sessionCache.has(projectKey(proj))) {
          renderBody(); // 显示 "加载中…"
          // F07（★ 判据实测抓到的第五个放大器，核实台账没点到）：这里原来是裸
          // `loadProjectSessions(...)`。平时用户一次只展开一个，看不出问题；**「全展开」会把
          // P 个 `details` 一起置 open ⇒ P 个 toggle 事件齐发 ⇒ P 条 IPC 同时出去**，
          // 把「加载全部」那条路的上限整个绕过去。现在三条路共用同一条有上限的队列。
          this.enqueueLazyLoad(proj, () => {
            if (details.isConnected) renderBody();
          });
        } else {
          renderBody();
        }
      } else {
        this.expandedProjects.delete(projectKey(proj));
        body.replaceChildren(); // 折叠时清空，下次展开重画
      }
    });

    // 初次构造时如果已经标记为展开，提前 render body
    if (expanded) renderBody();

    return details;
  }

  /**
   * 全量加载：把所有项目的 session 详情拉到缓存。
   *
   * 完成后：
   *   - 搜索的 matchProject 路径会命中已缓存的 session 字段（ai-title / customTitle /
   *     first_user_excerpt / session_id）
   *   - searchInput placeholder 更新提示
   *   - loadedAll = true，再次打开历史视图前不会重复跑
   *
   * 节流：并发上限 LOAD_ALL_CONCURRENCY（默认 4），避免一次性向后端 fire 500 个 IPC。
   */
  private async loadAllSessions(): Promise<void> {
    if (this.loadedAll) return;
    const pending = this.projects.filter(
      (p) => !this.sessionCache.has(projectKey(p)),
    );
    if (pending.length === 0) {
      this.loadedAll = true;
      this.updateSearchPlaceholder();
      this.statusEl.textContent = copyText("history.loadAll.done", { projects: this.projects.length });
      return;
    }

    this.loadAllBtn.disabled = true;
    const baseLabel = this.loadAllBtn.textContent;
    const total = pending.length;
    let done = 0;
    try {
      // F07：与搜索懒加载、全展开**共用同一条队列、同一个上限**。
      // 此前这里有自己的工作池、全展开是裸 `Promise.all(map)`、`details.toggle` 又是第三条路
      // ⇒ 三条路各自为政，其中两条无上限，而「全展开」会同时踩到两条 ⇒ 上限被绕过去。
      for (const proj of pending) {
        this.enqueueLazyLoad(proj, () => {
          done += 1;
          this.statusEl.textContent = copyText("history.loadAll.progress", { done, total });
          this.loadAllBtn.textContent = copyText("history.loadAll.progressButton", { done, total });
        });
      }
      await this.lazyDrained();
      this.loadedAll = true;
      this.updateSearchPlaceholder();
      // 重画一次以应用搜索匹配（如果用户已经在搜索框输入）
      this.renderList();
    } finally {
      this.loadAllBtn.disabled = false;
      this.loadAllBtn.textContent = baseLabel ?? copyText("history.loadAll.action");
    }
  }

  /** "展开/收起全部" 按钮：当前若全收起 → 全展开；否则 → 全收起 */
  private async toggleAll(): Promise<void> {
    if (this.projects.length === 0) return;
    const keys = this.projects.map(projectKey);
    const allExpanded = keys.every((k) => this.expandedProjects.has(k));
    if (allExpanded) {
      this.expandedProjects.clear();
      this.renderList();
      return;
    }
    // 全展开：触发未缓存项目的并发加载
    for (const k of keys) this.expandedProjects.add(k);
    this.renderList();
    const toLoad = this.projects.filter(
      (p) => !this.sessionCache.has(projectKey(p)),
    );
    if (toLoad.length > 0) {
      this.statusEl.textContent = copyText("history.toggleAll.loading", { count: toLoad.length });
      // F07：原来是 `Promise.all(toLoad.map(…))` —— **无上限**。改走同一条队列。
      for (const proj of toLoad) this.enqueueLazyLoad(proj);
      await this.lazyDrained();
      this.renderList();
    }
  }

  // === 过滤 / 排序 ===

  /** 项目级匹配：name / path / project_dir。命中后可能再叠 session 级匹配 */
  private matchProject(p: HistoryProject): boolean {
    if (!this.filter) return true;
    const hay = `${p.projectName}\n${p.projectPath}\n${p.projectDir}`.toLowerCase();
    if (hay.includes(this.filter)) return true;
    // project 元数据不命中时，看看缓存里的 sessions 是否有命中（仅对已加载项目）
    const cached = this.sessionCache.get(projectKey(p));
    if (!cached) return false;
    return cached.some((e) => this.matchSession(e));
  }

  /** 会话级匹配：ai_title / customTitle / first_user / sessionId */
  private matchSession(e: HistorySessionEntry): boolean {
    if (!this.filter) return true;
    const hay = [
      e.aiTitle ?? "",
      e.customTitle ?? "",
      e.firstUserExcerpt,
      e.sessionId,
    ]
      .join("\n")
      .toLowerCase();
    return hay.includes(this.filter);
  }

  /**
   * issue #12: 按当前 sort 模式排序整棵 fork 树。先排 roots，再迭代排每个 node 的 children。
   */
  private sortTree(roots: SessionTreeNode[]): void {
    const cmp = this.entryComparator();
    roots.sort(cmp);
    // 迭代遍历所有节点排序它们的 children（INVARIANT § 17: 不递归）
    const stack: SessionTreeNode[] = roots.slice();
    while (stack.length > 0) {
      const n = stack.pop()!;
      if (n.children.length > 0) {
        n.children.sort(cmp);
        for (const c of n.children) stack.push(c);
      }
    }
  }

  private entryComparator(): (a: SessionTreeNode, b: SessionTreeNode) => number {
    switch (this.sort) {
      case "started_desc":
        return (a, b) =>
          Number(b.entry.starred) - Number(a.entry.starred) ||
          b.entry.startedAt - a.entry.startedAt;
      case "updated_desc":
      default:
        return (a, b) =>
          Number(b.entry.starred) - Number(a.entry.starred) ||
          b.entry.updatedAt - a.entry.updatedAt;
    }
  }

  // === 会话行 ===

  // === F96 SS-4 ③块：共享动作表的 run 副作用体（判定在 history-actions.ts） ===
  // inline 行尾按钮与右键菜单走同一 run 分发 → 天然不漂移。star/rename/hide/delete 需活的
  // entry+project 引用（缓存/计数同步）；resume/new-session 只用 identity 段（搜索卡片 F85 也能用）。

  /** id → run 方法分发（对齐 actionsFor）。 */
  private runOf(id: HistoryActionId): (ctx: RowActionCtx) => void | Promise<void> {
    switch (id) {
      case "resume":
        return (c) => this.runResume(c);
      case "new-session":
        return (c) => this.runNewSession(c);
      case "star":
        return (c) => this.runStar(c);
      case "rename":
        return (c) => this.runRename(c);
      case "hide":
        return (c) => this.runHide(c);
      case "delete":
        return (c) => this.runDelete(c);
    }
  }

  private async runStar(ctx: RowActionCtx): Promise<void> {
    const e = ctx.entry,
      proj = ctx.project;
    if (!e || !proj) return;
    try {
      const next = await annotate(e.sessionId, { starred: !e.starred });
      const wasStarred = e.starred;
      e.starred = next.starred;
      // 同步 project 的 starred_count
      // `K-R92`：`null + 1 === 1` —— 一次 star 操作能把「不知道」变成一个看起来是真值的数，
      // 而且从此回不去。`bumpCounted` 让「不知道」加减之后**还是不知道**。
      if (!wasStarred && next.starred)
        proj.starredCount = bumpCounted(proj.starredCount, +1);
      else if (wasStarred && !next.starred)
        proj.starredCount = bumpCounted(proj.starredCount, -1);
      this.renderList();
    } catch (err) {
      console.warn("star update failed:", err);
      // 〔CFG1 · 4D〕从前只记日志：点了星标、什么都没变、也不说（E §3.3）。改名 / 隐藏同。
      showActionFailureToast(copyText("history.star.failed"), String(err));
    }
  }

  private async runRename(ctx: RowActionCtx): Promise<void> {
    const e = ctx.entry;
    if (!e) return;
    const cur = e.customTitle ?? e.aiTitle ?? "";
    const next = await askText(copyText("history.rename.prompt"), { initial: cur });
    if (next === null) return;
    try {
      // 〔C4d〕清空传**空串**（缺格 / `null` = 不改 —— 从前这里传 `null`，而 monitor 那份 patch 同样把 `null` 读成「不改」，
      //   「留空恢复默认」其实一直没生效；本机后端照搬了那条语义，这里改传空串，清空才真的清空）。
      const updated = await annotate(e.sessionId, { customTitle: next.trim() });
      e.customTitle = updated.customTitle;
      this.renderList();
    } catch (err) {
      console.warn("rename failed:", err);
      showActionFailureToast(copyText("history.rename.failed"), String(err));
    }
  }

  private async runHide(ctx: RowActionCtx): Promise<void> {
    const e = ctx.entry,
      proj = ctx.project;
    if (!e || !proj) return;
    try {
      const updated = await annotate(e.sessionId, { hidden: !e.hidden });
      const wasHidden = e.hidden;
      e.hidden = updated.hidden;
      if (!wasHidden && updated.hidden)
        proj.hiddenCount = bumpCounted(proj.hiddenCount, +1);
      else if (wasHidden && !updated.hidden)
        proj.hiddenCount = bumpCounted(proj.hiddenCount, -1);
      this.renderList();
    } catch (err) {
      console.warn("hide toggle failed:", err);
      showActionFailureToast(copyText("history.hide.failed"), String(err));
    }
  }

  private async runResume(ctx: RowActionCtx): Promise<void> {
    if (ctx.origin) {
      // F41：远端 resume 一键拉起（wt.exe → `ssh -t …`），失败回退 F09 复制命令。
      // F34：用户自定义远端 resume 命令（如 cct）；空 = 后端默认
      const origin = ctx.origin;
      const behavior = await getBehavior();
      // account-ux U3:无显式选号 → 跟随。先读该会话的 pin(源②,本机后端 `history-last-accounts` 只读本地那份注解,
      // 非远端 SSH)传给 follow,使「粘性优先」在 history 入口也成立——有 pin 走 pin、无 pin 走当前账号;
      // 配合 withAccount 的不-clobber 记账,绝不把既有 pin 翻成当前账号(U3 审计 重要-1)。显式选号维持 A4。
      let rowLastAccount: string | undefined;
      if (!ctx.account) {
        try {
          const lastMap = await lastAccounts();
          rowLastAccount = lastMap?.[ctx.sessionId];
        } catch {
          rowLastAccount = undefined;
        }
      }
      // A4：带账号 resume 统一走 withAccount（resolve configDir → 不可选则不起、说清〔FE1 · D-h〕→ record 源②）。
      await withAccount(
        origin,
        ctx.account ?? null,
        // 同 tabs.ts：`runRemoteResume` 已改返回 boolean，这条路显式丢弃（反馈走它自己的 toast）。
        async (mods) => {
          await runRemoteResume(
            origin,
            ctx.sessionId,
            ctx.cwd,
            await resolveResumeCommand(origin, behavior.resumeCommandRemote),
            mods,
          );
        },
        {
          sessionId: ctx.sessionId,
          // 〔FE1 · D-h〕要的号选不了 ⇒ `withAccount` 自己不起、说清、给显式选择（先前这里的提示完按基座起）。
          follow: ctx.account ? undefined : { lastAccount: rowLastAccount },
        },
      );
    } else {
      // 〔FE1〕本机 resume 的编排只有一份（`local-resume.ts`）：校验 sid → 铸名 → 起 → 记 pin。
      //   这里先前逐字抄着一份（`K-R46` 补铸名 · `K-H2b` 补账号 · `D3 阻-2` 补记 pin，三次都是
      //   「tab 栏那条早有了、这条没有」）。账号跟随这条会话上次的号 —— 与上面远端那条 `follow` **同形**。
      await resumeLocalSession({ sid: ctx.sessionId, cwd: ctx.cwd, account: { kind: "follow" } });
    }
  }

  private async runNewSession(ctx: RowActionCtx): Promise<void> {
    // `D1 阻-1`：**不等待**地把账号快照踢一脚（等它就多一拍，撞两条只放行一个微任务的判据）。
    primeLocalLaunchAccounts();
    const behavior = await getBehavior();
    if (ctx.origin) {
      // 远端：薄封装 F53 拉起（tmux 名派生 + 默认拉起命令兜底都在 runNewSessionRemote 里，
      // 本处既不知 tmux、也不知默认 agent；只传 F34 配置命令，空则传输层兜默认）。
      // account-ux U3:远端新会话跟随当前账号（新会话无 sid → 不记账）。
      const origin = ctx.origin;
      await withAccount(
        origin,
        null,
        async (mods) =>
          runNewSessionRemote(
            origin,
            ctx.cwd,
            await resolveResumeCommand(origin, behavior.resumeCommandRemote),
            mods,
          ),
        { follow: {} },
      );
    } else {
      try {
        // 本地：后端 new_local_session（cc 优先 + F34 自定义，无 sid/resume flag）。
        // 〔DUP1〕这里原来调一次 `validateLocalLaunch`〔散文墓碑〕（new 动作恒不 throw，只为「让本地那条路活过」）——
        // 那个函数随它唯一的一格（sid 字符集，交 Rust 判）删了。
        // ★★ `K-H2b` `D1 阻-1`：起新会话这条主路同样一个账号都不传。
        //    ⚠ 它取的是**当前账号**（不是从别的会话继承 —— 那是 fork 的语义），
        //    与远端那条 `runNewSessionRemote` 的 `withAccount(origin, null, …, {follow:{}})`
        //    **同形**：新会话跟随当前账号。
        // ★★ `K-P5h` `KP5HD2`：**这条命令现在把这次拉起的身份 token 交回来。**
        //    `K-P5 §3 三` 现打的那条结构性事实（「没有一处在起新会话时知道 sid」）
        //    在这一行上是活的：这一刻我们手上有 cwd、有账号，**就是没有 sid** ——
        //    于是那条 `recordLocalLaunchAccount` 的 pin 今天写不出来
        //    （`tabs.ts` 那条远端同形注释逐字写着「新会话无 sid → 不记账」）。
        //    ⇒ 把 token 挂进待回填表，等这条会话真的跑起来之后拿它反查 sid 再补写 pin。
        //    ⚠ **不 `await` 回填**（它要等进程起来，见 `resolvePendingLocalLaunches` 头注）；
        //      这里只是登记，一拍都不多花 —— 那两条只放行一个微任务的 DOM 判据在盯着。
        const launchId = await commands.new_local_session({
          cwd: ctx.cwd,
          launcher: behavior.resumeCommandLocal || null,
          account: localLaunchAccountSync(null),
        });
        rememberLocalLaunch(launchId, localLaunchAccountNameSync(null));
        showActionFailureToast(
          copyText("history.newSession.started"),
          copyText("history.newSession.startedDetail", { cwd: ctx.cwd }),
          { level: "info", durationMs: 6000 },
        );
      } catch (err) {
        showActionFailureToast(copyText("history.newSession.failed"), String(err));
      }
    }
  }

  private async runDelete(ctx: RowActionCtx): Promise<void> {
    const e = ctx.entry,
      proj = ctx.project;
    if (!e || !proj) return;
    const label = e.customTitle ?? e.aiTitle ?? e.sessionId.slice(0, 8);
    // 〔FW1 · 第四波 4D · 主会话裁 D-e〕删之前看活不活：活着 ⇒ 多问一句（Claude 还往旧文件里写，之后 resume 不到）；
    //   说不清（这条路答不出，`isLive === null`）⇒ 也多问一句（09-25 裁）。确定不活 ⇒ 照原来那一问 / 两问。
    const liveness = deleteLiveness(e.isLive, this.liveInTabs(e.sessionId));
    //   〔W5-UI 之后〕问一律走应用内对话框（`askConfirm`；原生 `confirm` 在真 app 里恒真、从来不拦）。
    if (liveness === "live" && !(await askConfirm(copyText("sessionState.deleteLive.confirm", { label })))) return;
    if (liveness === "unknown" && !(await askConfirm(copyText("sessionState.deleteUnknown.confirm", { label })))) return;
    if (e.origin) {
      // 远端删除更危险（删的是别人机器上的文件）→ 二次确认。〔RW1〕删那一下由那台机器的后端做。
      const ok1 = await askConfirm(
        copyText("history.delete.confirmRemote", { label, origin: e.origin }),
      );
      if (!ok1) return;
      const ok2 = await askConfirm(
        copyText("history.delete.confirmRemoteAgain", { origin: e.origin, label }),
      );
      if (!ok2) return;
      try {
        // 〔步 12·C〕与本机那条是**同一条命令**了，只是 origin 不同。
        await commands.delete_history_session({
          origin: e.origin,
          sessionId: e.sessionId,
          jsonlPath: e.jsonlPath,
        });
      } catch (err) {
        showActionFailureToast(copyText("history.delete.remoteFailed"), String(err));
        return;
      }
    } else {
      const ok = await askConfirm(
        copyText("history.delete.confirmLocal", { label }),
      );
      if (!ok) return;
      try {
        await commands.delete_history_session({
          origin: LOCAL_ORIGIN,
          sessionId: e.sessionId,
          jsonlPath: e.jsonlPath,
        });
      } catch (err) {
        showActionFailureToast(copyText("history.delete.failed"), String(err));
        return;
      }
    }
    // 〔C4d〕会话删了 ⇒ 连带删本机那条注解（从前 monitor 删完顺手清；今天注解归本机后端，由这里交 `history-forget`）。
    void forgetAnnotation(e.sessionId);
    // 成功后：从缓存移除 + 同步 project counts（本地 / 远端一致）。
    const arr = this.sessionCache.get(projectKey(proj));
    if (arr) {
      const idx = arr.findIndex((x) => x.sessionId === e.sessionId);
      if (idx >= 0) arr.splice(idx, 1);
    }
    proj.sessionCount = Math.max(0, proj.sessionCount - 1);
    if (e.starred) proj.starredCount = bumpCounted(proj.starredCount, -1);
    if (e.hidden) proj.hiddenCount = bumpCounted(proj.hiddenCount, -1);
    // 项目内全部删完了 → 也从 projects 列表移除
    if (proj.sessionCount === 0) {
      this.projects = this.projects.filter(
        (p) => projectKey(p) !== projectKey(proj),
      );
      // F76：远端项目还要从 remoteCache 同步移除，否则 TTL 内重开会把这个 0 会话的幽灵
      // 项目从陈旧缓存拼回来（`this.projects` 与 `remoteCache.projects` 共享对象引用，
      // 结构性移除不会自动传导——见 refresh() 顶部承重不变式）。本地项目不在缓存里，no-op。
      if (this.remoteCache) {
        this.remoteCache.projects = this.remoteCache.projects.filter(
          (p) => projectKey(p) !== projectKey(proj),
        );
      }
      this.sessionCache.delete(projectKey(proj));
      this.expandedProjects.delete(projectKey(proj));
    }
    this.renderList();
  }

  /** F96：条目/搜索卡片右键 → 极简上下文菜单（守 SS-1，不抽共享组件、不复用 tabs 私有函数）。 */
  private showEntryMenu(x: number, y: number, ctx: RowActionCtx): void {
    this.closeEntryMenu();
    const menu = document.createElement("div");
    menu.className = "history-context-menu";
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    for (const def of actionsFor(ctx)) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "history-context-item";
      if (def.danger) item.classList.add("is-danger");
      item.textContent = def.label(ctx);
      item.addEventListener("click", () => {
        this.closeEntryMenu();
        void this.runOf(def.id)(ctx);
      });
      menu.appendChild(item);
    }
    document.body.appendChild(menu);
    this.openEntryMenu = menu;
    // A4：远端会话——异步追加「用账号 X resume」项（先显标准项，账号项 fetch 完再挂，缓存暖则几乎无感）。
    if (ctx.origin) void this.appendAccountResumeItems(menu, ctx);
    // 关闭监听：Esc 键 / 菜单外 pointerdown 才关（点菜单内边距/非 Esc 键 → 早退不关）。
    // ★ 不用 `{once:true}`——它会在早退那次就摘掉监听，导致「按过任意非 Esc 键后 Esc 再关不掉」
    // 「点 padding 后外部点击再关不掉」。改为常驻监听、由 closeEntryMenu 显式反注册。
    const close = (ev: Event): void => {
      if (ev instanceof KeyboardEvent && ev.key !== "Escape") return;
      if (ev.type === "pointerdown" && menu.contains(ev.target as Node)) return;
      this.closeEntryMenu();
    };
    this.entryMenuClose = close;
    // 下一拍才挂（避免开菜单这次 contextmenu 自身派发的 pointerdown 立即关掉）。
    setTimeout(() => {
      if (this.openEntryMenu !== menu) return; // 期间已被新菜单/关闭取代 → 别挂陈旧监听
      document.addEventListener("pointerdown", close);
      document.addEventListener("keydown", close);
    }, 0);
  }

  /**
   * A4：给远端会话菜单追加「用账号 X resume」项（每个可选账号一条）。**异步**——不阻塞菜单弹出。
   * 只在 ≥2 个可选账号时出（<2 无可切换意义）；账号库不可用（旧/未启用）安静不加（§7 降级）。
   * 追加前校验菜单仍是当前打开的那个（防 fetch 期间已换/已关，避免挂到陈旧 DOM）。
   */
  private async appendAccountResumeItems(menu: HTMLElement, ctx: RowActionCtx): Promise<void> {
    if (!ctx.origin) return;
    let state;
    try {
      state = await fetchAccounts(ctx.origin);
    } catch {
      return; // fetch 失败 → 就不加账号项，默认 resume 仍可用
    }
    if (!state.available) return; // 旧 backend / 未启用 → 安静降级
    const selectable = state.accounts.filter(isSelectable);
    if (selectable.length < 2) return; // 无可切换选择就不加噪
    if (this.openEntryMenu !== menu) return; // fetch 期间菜单已变/已关
    const sep = document.createElement("div");
    sep.className = "history-context-sep";
    menu.appendChild(sep);
    for (const a of selectable) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "history-context-item";
      item.textContent = copyText("history.accountResume.item", { name: a.name });
      item.title = copyText("history.accountResume.hint", { name: a.name, email: a.email ? ` · ${a.email}` : "" });
      item.addEventListener("click", () => {
        this.closeEntryMenu();
        void this.runResume({ ...ctx, account: a.name });
      });
      menu.appendChild(item);
    }
  }

  private closeEntryMenu(): void {
    if (this.entryMenuClose) {
      document.removeEventListener("pointerdown", this.entryMenuClose);
      document.removeEventListener("keydown", this.entryMenuClose);
      this.entryMenuClose = null;
    }
    if (this.openEntryMenu) {
      this.openEntryMenu.remove();
      this.openEntryMenu = null;
    }
  }

  private buildEntryRow(
    e: HistorySessionEntry,
    proj: HistoryProject,
    depth: number = 0,
    childCount: number = 0,
    orphan: boolean = false,
  ): HTMLElement {
    const row = document.createElement("div");
    row.className = "history-entry";
    if (e.hidden) row.classList.add("is-hidden-entry");
    if (e.isLive) row.classList.add("is-live-entry");
    if (depth > 0) {
      row.classList.add("is-fork-child");
      row.style.setProperty("--fork-depth", String(depth));
    }

    row.addEventListener("click", (ev) => {
      const target = ev.target as HTMLElement | null;
      if (target?.closest(".history-star, .history-action, .history-fork-toggle"))
        return;
      this.openViewer(e);
    });

    // F96：条目行动作上下文（inline 按钮与右键菜单共用）。带活的 entry/project 引用，
    // star/hide/delete 的 run 直接 mutate 它们并同步缓存（与旧 inline 闭包同一对象）。
    const rowCtx: RowActionCtx = {
      sessionId: e.sessionId,
      jsonlPath: e.jsonlPath,
      cwd: e.projectPath,
      origin: e.origin,
      isLive: e.isLive,
      starred: e.starred,
      hidden: e.hidden,
      hasEntry: true,
      entry: e,
      project: proj,
    };
    row.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      this.showEntryMenu(ev.clientX, ev.clientY, rowCtx);
    });

    // issue #12: fork 树展开 / 折叠按钮（只在有 children 时出现）
    if (childCount > 0) {
      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "history-fork-toggle";
      const expanded = this.expandedForks.has(e.sessionId);
      toggle.textContent = expanded ? copyText("history.fork.expandedMark") : copyText("history.fork.collapsedMark");
      toggle.title = expanded
        ? copyText("history.fork.collapseHint", { childCount })
        : copyText("history.fork.expandHint", { childCount });
      toggle.addEventListener("click", (ev) => {
        ev.stopPropagation();
        if (this.expandedForks.has(e.sessionId)) {
          this.expandedForks.delete(e.sessionId);
        } else {
          this.expandedForks.add(e.sessionId);
        }
        saveExpandedForks(this.expandedForks);
        this.renderList();
      });
      row.appendChild(toggle);
    } else if (depth > 0) {
      // child 行没有 toggle 但需要占位保持对齐
      const spacer = document.createElement("span");
      spacer.className = "history-fork-toggle history-fork-spacer";
      row.appendChild(spacer);
    }

    // issue #12: orphan 标记（fork 自不存在的 parent → "↳ 原 session 不见了"）
    if (orphan && e.forkedFromSessionId) {
      const orphanMark = document.createElement("span");
      orphanMark.className = "history-fork-orphan";
      orphanMark.textContent = copyText("history.fork.orphanMark");
      orphanMark.title = copyText("history.fork.orphanHint", { parent: e.forkedFromSessionId.slice(0, 8) });
      row.appendChild(orphanMark);
    }

    // Batch11-F32：CC 后台分身会话徽标——resume 请选主会话（克隆与主会话同标题，
    // 不标必踩；CC 官方 resume 选择器也标 "bg"）
    if (e.isBg) {
      const bgMark = document.createElement("span");
      bgMark.className = "history-bg-badge";
      bgMark.textContent = copyText("history.entry.backgroundMark");
      bgMark.title =
        copyText("history.entry.backgroundHint");
      row.appendChild(bgMark);
    }
    const starBtn = document.createElement("button");
    starBtn.type = "button";
    starBtn.className = "history-star";
    starBtn.textContent = e.starred ? copyText("history.entry.starred") : copyText("history.entry.unstarred");
    starBtn.title = e.starred ? copyText("history.entry.unstarHint") : copyText("history.entry.starHint");
    if (e.starred) starBtn.classList.add("is-starred");
    starBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      void this.runOf("star")(rowCtx);
    });
    row.appendChild(starBtn);

    const main = document.createElement("div");
    main.className = "history-main";

    const title = document.createElement("div");
    title.className = "history-title";
    const displayTitle =
      e.customTitle ??
      e.aiTitle ??
      e.firstUserExcerpt ??
      e.sessionId.slice(0, 8);
    title.textContent = displayTitle;
    main.appendChild(title);

    if (e.customTitle && e.aiTitle && e.customTitle !== e.aiTitle) {
      const subtitle = document.createElement("div");
      subtitle.className = "history-subtitle";
      subtitle.textContent = e.aiTitle;
      main.appendChild(subtitle);
    }

    if (e.firstUserExcerpt && e.firstUserExcerpt !== displayTitle) {
      const excerpt = document.createElement("div");
      excerpt.className = "history-excerpt";
      excerpt.textContent = e.firstUserExcerpt;
      main.appendChild(excerpt);
    }

    const meta = document.createElement("div");
    meta.className = "history-meta";
    meta.append(
      // `isLive` 是三态（`null` = 这条路答不出，`K-R92`）。三态各一个词，全从 `sessionState.*` 取
      // （`设计/30 §3.5.2`：说到会话状态的字只住那里；`§3.5.7a`：说不清不许说成已结束）。
      // 〔AR1〕此前这一格显示英文 `live` / `archived`，而且把「不知道」也显示成 `archived`。
      makeChip(livenessWord(e.isLive), e.isLive === true ? "history-live" : ""),
      makeChip(copyText("history.entry.messages", { count: e.messageCountApprox })),
      makeChip(formatTimestampSmart(e.updatedAt)),
    );
    main.appendChild(meta);

    row.appendChild(main);

    const actions = document.createElement("div");
    actions.className = "history-actions";

    const renameBtn = document.createElement("button");
    renameBtn.type = "button";
    renameBtn.className = "history-action";
    renameBtn.textContent = copyText("history.entry.renameIcon"); // ✎ pencil（BMP，非 emoji）
    renameBtn.title = copyText("history.entry.renameHint");
    renameBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      void this.runOf("rename")(rowCtx);
    });
    actions.appendChild(renameBtn);

    const hideBtn = document.createElement("button");
    hideBtn.type = "button";
    hideBtn.className = "history-action";
    // hidden 时按钮指示"恢复显示"用 +；显示时按钮指示"隐藏"用 –（en-dash U+2013）
    hideBtn.textContent = e.hidden ? "+" : copyText("history.entry.hideIcon");
    hideBtn.title = e.hidden ? copyText("history.entry.unhideHint") : copyText("history.entry.hideHint");
    hideBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      void this.runOf("hide")(rowCtx);
    });
    actions.appendChild(hideBtn);

    const resumeBtn = document.createElement("button");
    resumeBtn.type = "button";
    resumeBtn.className = "history-action";
    resumeBtn.textContent = copyText("history.resume.icon"); // ↺ anticlockwise circle arrow ("replay")
    // F41：远端一键拉起（wt.exe → `ssh -t …`），失败回退 F09 复制命令；本地 wt.exe/PowerShell。
    resumeBtn.title = e.origin
      ? copyText("history.resume.remoteHint", { origin: e.origin })
      : copyText("history.resume.hint"); // F96：去硬编码启动命令（守「不许知道是哪个 agent」）
    resumeBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      void this.runOf("resume")(rowCtx);
    });
    actions.appendChild(resumeBtn);

    const deleteBtn = document.createElement("button");
    deleteBtn.type = "button";
    deleteBtn.className = "history-action history-action-danger";
    deleteBtn.textContent = copyText("history.entry.deleteIcon"); // ✕ multiplication X
    // F11：远端会话删除经 SFTP（SS-G 用户数据写豁免，二次确认）；本地走既有物理删除。
    deleteBtn.title = e.origin
      ? copyText("history.entry.deleteRemoteHint", { origin: e.origin })
      : copyText("history.entry.deleteHint");
    deleteBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      void this.runOf("delete")(rowCtx);
    });
    actions.appendChild(deleteBtn);

    row.appendChild(actions);

    return row;
  }
}

function makeChip(text: string, extraClass = ""): HTMLElement {
  const el = document.createElement("span");
  el.className = `history-chip ${extraClass}`.trim();
  el.textContent = text;
  return el;
}

function makeStatusRow(text: string): HTMLElement {
  const el = document.createElement("div");
  el.className = "history-group-status";
  el.textContent = text;
  return el;
}

// === issue #12: fork 树构建 + 持久化 ===

/**
 * 按 forkedFromSessionId 在项目内建 tree。
 *
 * # 为什么**不加 memo**〔audit-0805 §5 1u，08-06 结案〕
 *
 * `ROADMAP §5` 的 1u 写着「合并重画把**次数**降了一个量级，**单次代价没动**」，
 * 并把它挂成一条待办。08-06 按第四问（**这个数你量过吗**）去量，结论是**不该做**：
 *
 * | 单个项目的会话数 | 一次 `buildSessionTree` + `sortTree` |
 * |---|---|
 * | 200 | 0.18 ms |
 * | 1 000 | 0.35 ms |
 * | 5 000 | 1.39 ms |
 * | 20 000 | 5.85 ms |
 *
 * ⚠ N 是**单个项目**的可见会话数（本函数按项目调），现实量级是几十到几百 ⇒ **亚毫秒**。
 * 而 F15 之后每帧只重画一次。加 memo 要引入失效键（过滤结果 / 星标 / 排序 / 折叠），
 * **换来的是亚毫秒，代价是一块新的状态与它的失效 bug**。
 *
 * ★ 这个结论**依赖「按项目调」**：若将来改成对全部项目建一棵大树，N 就变成总会话数，
 * 上表要重量。`history-fanout.vitest.ts` 里有一条判据钉住**调用点恰好一处**，
 * 挪动它的人会被迫回来看这段。
 *
 * 算法（O(N) 迭代，遵 INVARIANT § 17）：
 *  1. 一遍 byId 索引
 *  2. 二遍把每个 entry 挂到 parent.children（parent 存在）或 roots（parent 不存在）
 *  3. parent 存在但不在本项目集（跨项目 fork / parent 已物理删除）→ 当 root + orphan=true
 */
function buildSessionTree(entries: HistorySessionEntry[]): SessionTreeNode[] {
  const byId = new Map<string, SessionTreeNode>();
  for (const e of entries) {
    byId.set(e.sessionId, { entry: e, children: [], orphan: false });
  }
  const roots: SessionTreeNode[] = [];
  for (const node of byId.values()) {
    const parentId = node.entry.forkedFromSessionId;
    if (parentId) {
      const parent = byId.get(parentId);
      if (parent) {
        parent.children.push(node);
        continue;
      }
      // parent 不在本项目集 → 孤儿，挂顶层加 marker
      node.orphan = true;
    }
    roots.push(node);
  }
  return roots;
}

function loadExpandedForks(): Set<string> {
  const arr = safeGetJson<string[]>(LS_KEYS.historyExpandedForks);
  if (Array.isArray(arr)) return new Set(arr.filter((x) => typeof x === "string"));
  return new Set();
}

function saveExpandedForks(s: Set<string>): void {
  safeSetJson(LS_KEYS.historyExpandedForks, Array.from(s));
}

// F86(#45)：来源筛选/折叠偏好的 localStorage 持久化（照 expandedForks 先例；判定逻辑在 history-prefs.ts）。
function loadHiddenOrigins(): Set<string> {
  return new Set(normalizeOriginKeys(safeGetJson(LS_KEYS.historyHiddenOrigins)));
}

function saveHiddenOrigins(s: Set<string>): void {
  safeSetJson(LS_KEYS.historyHiddenOrigins, Array.from(s));
}

function loadOriginOpenOverrides(): OriginOpenOverrides {
  return normalizeOverrides(safeGetJson(LS_KEYS.historyOriginOpen));
}

function saveOriginOpenOverrides(o: OriginOpenOverrides): void {
  safeSetJson(LS_KEYS.historyOriginOpen, o);
}

/**
 * F76b(#46)：从 localStorage 读远端来源快照，**逐元素防脏**(对齐 loadExpandedForks/normalize* 惯例)。
 * ★审计:仅校验数组**形状**不够——被篡改/旧 schema 若混入 `null`/基元元素,后续 `renderList` 对它 deref
 * `p.origin`(`:968`/`renderOriginFilter`)会抛 TypeError,而 `renderList` 在 `refresh`(`:410`)里**未 try 包**
 * → 冒泡出 `open()`、历史视图打不开直到清 localStorage。故过滤到「非空对象 + 关键 `projectPath` 为 string」。
 * `loadedAt` 恒归 **0**:持久快照只作首帧暖绘,首开必刷一次(见构造注释),不冒充新鲜、不吃跨启动陈旧。
 */
function loadPersistedRemoteCache(): RemoteSourceCache<HistoryProject> | null {
  const raw = safeGetJson<{ projects?: unknown }>(LS_KEYS.historyRemoteSources);
  if (!raw || !Array.isArray(raw.projects)) return null;
  const projects = raw.projects.filter(
    (p): p is HistoryProject =>
      p !== null &&
      typeof p === "object" &&
      typeof (p as { projectPath?: unknown }).projectPath === "string",
  );
  return { projects, loadedAt: 0 };
}

/**
 * 〔AR1〕历史条目的活性三态 → 说给用户的那个词（`设计/30 §3.5.2` · `§3.5.7a`）。
 * `null` = 这条路答不出 ⇒「说不清」，不许落成「已结束」。
 */
/**
 * 〔FW1 · 第四波 4D · D-e〕删会话前的活性判定（纯函数）：tab 栏里活着 ∨ 条目说活着 ⇒ `live`；
 * 否则条目答不出（`null`）⇒ `unknown`；否则 `dead`。tab 栏那一格是此刻的事实，所以它说活就算活。
 */
export function deleteLiveness(entryIsLive: boolean | null, liveInTabs: boolean): "live" | "unknown" | "dead" {
  if (liveInTabs || entryIsLive === true) return "live";
  return entryIsLive === null ? "unknown" : "dead";
}

function livenessWord(isLive: boolean | null): string {
  if (isLive === null) return copyText("sessionState.unseen.name");
  return isLive ? copyText("sessionState.live.name") : copyText("sessionState.ended.name");
}
