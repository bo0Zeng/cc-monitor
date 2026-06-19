/**
 * 历史会话浏览视图。
 *
 * 设计：全屏接管 `#message-stream` 区域（不破坏 tab-bar / status-bar）。
 *  - 显示时把原 message-stream 的 children 临时挪走 → 视图 mount 进去
 *  - 关闭时反向还原
 *
 * 两级懒加载（性能优化）：
 *   1. open / refresh：调 `list_history_projects` 只拿项目级元数据（不读 jsonl 内容）。
 *      500+ 项目 < 100ms。所有组**默认折叠**。
 *   2. 用户展开某个项目组：调 `list_history_sessions_in_project(projectDir)` 拿该项目
 *      下所有会话详情，缓存到 `sessionCache`。下次展开同项目直接读缓存。
 *
 * 搜索两种模式（issue #6）：
 *   - "项目"（默认）：本地即时过滤项目级字段（name / path）+ 已展开项目内的会话标题。
 *   - "全文"：回车触发后端 `search_history` 全文搜索所有会话**内容**（user 输入 +
 *     Claude 回复；可勾选"含工具内容"附加 tool_use/result/thinking）。结果按 session
 *     分组 + snippet <mark> 高亮，点击进 viewer 滚动定位到命中消息。
 *
 * 操作：star / 重命名 / 删除 / 恢复 全部走单条 IPC，本地状态在响应回来后同步。
 *  - star/hide 改变会更新缓存中那条 entry，并同步对应 project 的 starred/hidden_count
 *  - delete 从缓存移除条目，并 -1 project.sessionCount
 */

import { invoke, Channel } from "@tauri-apps/api/core";
import { SessionViewer, type ViewerOptions } from "./session-viewer";
import { dispatcher } from "../keybindings/registry";
import { showActionFailureToast } from "../error-toast";
import { LS_KEYS, safeGetJson, safeSetJson } from "../local-storage";
import { formatTimestampSmart } from "../format";

/** 项目级元数据，从 `list_history_projects` 拿。不含 session 内容。
 *  P1.2：后端 wire 全 camelCase（`#[serde(rename_all = "camelCase")]`）。 */
interface HistoryProject {
  projectPath: string;
  projectName: string;
  /** `<claude_dir>/projects/<encoded-dir>` 的完整路径，作 lazy load 的 key */
  projectDir: string;
  sessionCount: number;
  starredCount: number;
  hiddenCount: number;
  lastActivity: number;
  hasLive: boolean;
  /** issue #16：数据来源。undefined=本地；host 字符串=远端（组头显示 [host] 徽标，
   *  展开走 stream_remote_history_sessions）。 */
  origin?: string;
}

/** issue #16：项目缓存/展开态的 key。本地 = projectDir；远端用 origin 命名空间隔离
 *  （本地与远端可能有相同的编码目录名，裸 projectDir 会撞 key）。 */
function projectKey(p: { origin?: string; projectDir: string }): string {
  return p.origin ? `${p.origin}\u0000${p.projectDir}` : p.projectDir;
}

/** 会话级详情，从 `stream_history_sessions_in_project` 流式拿。 */
interface HistorySessionEntry {
  sessionId: string;
  projectPath: string;
  projectName: string;
  aiTitle: string | null;
  firstUserExcerpt: string;
  startedAt: number;
  updatedAt: number;
  jsonlPath: string;
  isLive: boolean;
  messageCountApprox: number;
  starred: boolean;
  customTitle: string | null;
  hidden: boolean;
  /** issue #12: 若是 /branch fork 出来的，记 parent session id（在本项目内才能建树） */
  forkedFromSessionId?: string;
  /** issue #12: fork 处的 parent 消息 uuid（v1 树渲染不显示，将来可点击跳到父消息） */
  forkedFromMessageUuid?: string;
  /** issue #16：数据来源。undefined=本地；host=远端（禁用 resume/delete，查看走
   *  stream_read_remote_session）。 */
  origin?: string;
}

/**
 * issue #12: session tree node。child 关系由 forkedFromSessionId 建。
 * 项目内独立树（跨项目 fork 不连接）。parent 不在本项目时 child 当 root + marker。
 */
interface SessionTreeNode {
  entry: HistorySessionEntry;
  children: SessionTreeNode[];
  /** 1 = 本项目里找不到 parent（跨项目 fork / parent 已物理删除）→ root 上加 marker */
  orphan: boolean;
}

/** 后端 `update_history_metadata` 返回值；wire 是 camelCase（带 snake_case alias）。 */
interface EntryMetadata {
  starred: boolean;
  customTitle: string | null;
  hidden: boolean;
  updatedAt: number;
}

/** 组内会话排序模式（顶层布局固定按工作目录分组，不是 sort 选项）。 */
type SortMode = "updated_desc" | "started_desc";

/** issue #6: 全文搜索 —— 单条命中的前/中/后三段（matched 前端包 <mark>）。 */
interface SearchHit {
  uuid: string;
  tsMs: number;
  /** "user" | "assistant" | "tool" */
  kind: string;
  before: string;
  matched: string;
  after: string;
}

/** issue #6: 全文搜索 —— 一个会话的命中组。 */
interface SearchSessionHits {
  sessionId: string;
  projectPath: string;
  projectName: string;
  jsonlPath: string;
  title: string;
  updatedAt: number;
  hitCount: number;
  hits: SearchHit[];
  /** issue #28: 数据来源。undefined=本地；远端=该台 label，渲染 `[host]` 前缀 +
   *  点击走远端只读视图（origin 透传 openViewer → stream_read_remote_session）。 */
  origin?: string;
}

/** issue #6: `search_history` IPC 返回（后端 wire 全 camelCase）。 */
interface SearchResponse {
  /** "ready" | "indexing" */
  status: string;
  totalHits: number;
  sessionCount: number;
  truncated: boolean;
  indexedSessions: number;
  indexedMessages: number;
  sessions: SearchSessionHits[];
}

/** issue #6: 历史浏览器的两种模式 —— 项目树过滤 vs 内容全文搜索。 */
type SearchMode = "tree" | "fulltext";

export class HistoryView {
  /** fixed overlay 根；open 时挂 document.body，close 时 remove。 */
  private root: HTMLElement;

  /** 项目级数据，初次 open 拉一次 */
  private projects: HistoryProject[] = [];
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
   * F02 多机 #30：被折叠的来源大区 key（origin ?? ""，本地为 ""）。**默认全展开**——
   * 只记用户主动折叠的来源。仅在 >1 个 origin（本地 + 远端 / 多远端）时才有分组大区。
   */
  private collapsedOrigins = new Set<string>();
  /**
   * F03 多机 #30：被**隐藏**的来源 key（origin ?? ""）。默认空 = 全显示。机器选择器
   * chip 点掉某来源 → 加入此集 → 该来源项目不渲染。仅在 >1 个 origin 时显示筛选条。
   */
  private hiddenOrigins = new Set<string>();
  /** F03：来源筛选 chip 行（插在 statusEl 与 listEl 间；≤1 来源时 display:none）。 */
  private originFilterBar!: HTMLElement;
  /**
   * issue #12: fork 树展开状态。session_id ∈ 集合 = 该 session 的 children 展开。
   * **默认折叠** —— 第一次见到 fork 父节点时它的 children 不显示。从 localStorage 恢复。
   */
  private expandedForks: Set<string> = loadExpandedForks();

  constructor() {
    this.root = this.build();
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
    // 重新打开时清掉旧缓存（避免文件已被外部改动后展示陈旧数据）
    this.sessionCache.clear();
    this.loadingProjects.clear();
    this.loadedAll = false;
    this.updateSearchPlaceholder();
    // issue #5: 注册到 dispatcher 弹层栈，Esc 由 dispatcher 派给我
    dispatcher.pushOverlay(this);
    await this.refresh();
    this.searchInput.focus();
  }

  /** 根据当前模式 / 是否已全量加载更新搜索框 placeholder，告知用户搜索覆盖范围 */
  private updateSearchPlaceholder(): void {
    if (this.searchMode === "fulltext") {
      this.searchInput.placeholder =
        "全文搜索会话内容（user 输入 + Claude 回复）· 回车搜索";
      return;
    }
    if (this.loadedAll) {
      this.searchInput.placeholder = "搜索：项目 + 所有会话内容（ai-title / 首条消息 / sid）";
    } else {
      this.searchInput.placeholder =
        "搜索：项目名 / 路径（已展开项目还匹配会话内容；或点「全量加载」全文搜）";
    }
  }

  close(): void {
    if (!this.isOpen) return;
    this.closeViewer();
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
    const proj = entry.projectName || entry.projectPath || "(未知项目)";
    const subtitle =
      entry.projectPath && entry.projectPath !== proj
        ? `${proj}  ·  ${entry.projectPath}`
        : proj;
    this.openViewerWith({
      jsonlPath: entry.jsonlPath,
      displayTitle,
      subtitle,
      origin: entry.origin,
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

  /** Esc 优先关查看器，否则关整个历史视图。main.ts 监听后调本方法。 */
  handleEscape(): void {
    if (this.viewer) {
      this.closeViewer();
    } else {
      this.close();
    }
  }

  /** issue #5 OverlayHandle 接口：跟 handleEscape 同一行为 */
  handleEsc(): void {
    this.handleEscape();
  }

  private async refresh(): Promise<void> {
    this.statusEl.textContent = "加载项目列表…";
    this.listEl.replaceChildren();
    this.sessionCache.clear();
    this.loadingProjects.clear();
    try {
      const local = await invoke<HistoryProject[]>("list_history_projects");
      this.projects = local;
      this.renderList();
    } catch (e) {
      this.statusEl.textContent = `加载失败：${String(e)}`;
      return;
    }
    // issue #16：远端历史叠加（独立 try——远端连不上/无配置不影响本地浏览）。
    // 远端未启用时后端返回空数组，无感 no-op。
    try {
      const remote = await invoke<HistoryProject[]>(
        "list_remote_history_projects",
      );
      if (remote.length > 0) {
        this.projects = [...this.projects, ...remote];
        this.renderList();
      }
    } catch (e) {
      // 远端查询失败只提示，不阻断本地历史
      showActionFailureToast("远端历史加载失败", String(e));
    }
  }

  /**
   * 懒加载某个项目下的会话详情。重复调返回同一个 Promise。
   *
   * issue #12: 改用流式 IPC `stream_history_sessions_in_project` + Channel —— 后端
   * 边解析边发，前端 rAF 节流 re-render。大项目（几十个 session）首条 < 100ms 出现，
   * 不再"等齐"。完成（invoke resolve）即所有 entry 都在 cache 里。
   *
   * 取消：当 HistoryView 关闭 / 缓存 clear 时 channel 失去引用 → 后端 send 返 Err
   * → 后端 break loop。无需显式 cancel IPC。
   */
  private loadProjectSessions(proj: HistoryProject): Promise<void> {
    const key = projectKey(proj);
    const inFlight = this.loadingProjects.get(key);
    if (inFlight) return inFlight;
    if (this.sessionCache.has(key)) return Promise.resolve();

    const entries: HistorySessionEntry[] = [];
    this.sessionCache.set(key, entries);

    const channel = new Channel<HistorySessionEntry>();
    let rafPending = false;
    channel.onmessage = (entry) => {
      entries.push(entry);
      if (rafPending) return;
      rafPending = true;
      requestAnimationFrame(() => {
        rafPending = false;
        // 重画一次列表 —— 当前项目已在 expandedProjects 时其 body 会跟着重画
        if (this.isOpen) this.renderList();
      });
    };

    // issue #16：远端项目走 stream_remote_history_sessions（独立 SSH 连接一次性
    // exec daemon --list-sessions），本地走原 IPC。entry 结构两端一致。
    const ipc = proj.origin
      ? "stream_remote_history_sessions"
      : "stream_history_sessions_in_project";

    const p = (async () => {
      try {
        // 多机 #30：远端项目带 origin（= 该台 label）让后端按 label 选连哪台；
        // 本地 proj.origin=undefined → JSON 省略，本地命令无感。
        await invoke(ipc, {
          projectDir: proj.projectDir,
          origin: proj.origin,
          onEntry: channel,
        });
        // 完成后再画一次（兜底最后一帧没触发 rAF 的边界）
        if (this.isOpen) this.renderList();
      } catch (e) {
        console.warn(`stream sessions in ${key} failed:`, e);
        if (proj.origin) showActionFailureToast("远端会话列表加载失败", String(e));
        // 失败时保留已收集的 entries（部分流完的也算）
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
    backBtn.textContent = "← 返回";
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
    mkModeBtn("tree", "项目", "按项目名 / 标题过滤（本地，即时）");
    mkModeBtn("fulltext", "全文", "搜索所有会话的消息内容（回车触发）");
    bar.appendChild(modeToggle);

    this.searchInput = document.createElement("input");
    this.searchInput.type = "search";
    this.searchInput.className = "history-search";
    // placeholder 在 updateSearchPlaceholder 里根据模式 / loadedAll 动态设
    this.searchInput.addEventListener("input", () => {
      if (this.searchMode === "tree") {
        this.filter = this.searchInput.value.trim().toLowerCase();
        this.renderList();
      } else if (this.searchInput.value.trim() === "") {
        // 全文模式清空 → 清结果
        this.runFullTextSearch();
      }
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
    sortSel.title = "组内会话的排序方式";
    const options: { value: SortMode; label: string }[] = [
      { value: "updated_desc", label: "组内：最近更新" },
      { value: "started_desc", label: "组内：最近创建" },
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
    expandAllBtn.textContent = "展开/收起";
    expandAllBtn.title = "切换所有项目组的展开状态";
    expandAllBtn.addEventListener("click", () => void this.toggleAll());
    bar.appendChild(expandAllBtn);
    this.treeOnlyEls.push(expandAllBtn);

    // 全量加载：把每个项目的 session 详情都拉到缓存，让搜索可命中 session 内容
    this.loadAllBtn = document.createElement("button");
    this.loadAllBtn.type = "button";
    this.loadAllBtn.className = "history-refresh";
    this.loadAllBtn.textContent = "全量加载";
    this.loadAllBtn.title =
      "拉取所有项目的会话详情，加载后搜索可匹配 session 内容（ai-title / 首条消息 / sid）";
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
    hiddenText.textContent = "显示已隐藏";
    hiddenLabel.appendChild(hiddenText);
    bar.appendChild(hiddenLabel);
    this.treeOnlyEls.push(hiddenLabel);

    const refreshBtn = document.createElement("button");
    refreshBtn.type = "button";
    refreshBtn.className = "history-refresh";
    refreshBtn.textContent = "刷新";
    refreshBtn.addEventListener("click", () => void this.refresh());
    bar.appendChild(refreshBtn);
    this.treeOnlyEls.push(refreshBtn);

    // issue #6: 全文模式专属控件 —— "含工具内容" 复选框 + "重新索引"
    const toolsLabel = document.createElement("label");
    toolsLabel.className = "history-toggle";
    toolsLabel.title =
      "默认只搜 user 输入 + Claude 回复文本；勾选后附加搜索工具调用 / 结果 / thinking";
    const toolsCheck = document.createElement("input");
    toolsCheck.type = "checkbox";
    toolsCheck.addEventListener("change", () => {
      this.includeTools = toolsCheck.checked;
      if (this.searchInput.value.trim() !== "") this.runFullTextSearch();
    });
    toolsLabel.appendChild(toolsCheck);
    const toolsText = document.createElement("span");
    toolsText.textContent = "含工具内容";
    toolsLabel.appendChild(toolsText);
    bar.appendChild(toolsLabel);
    this.fulltextOnlyEls.push(toolsLabel);

    // 搜索范围：全部 / 只我的输入 / 只 Claude
    const scopeSel = document.createElement("select");
    scopeSel.className = "history-sort";
    scopeSel.title = "搜索范围";
    for (const o of [
      { value: "all", label: "范围：全部" },
      { value: "user", label: "范围：只我的输入" },
      { value: "assistant", label: "范围：只 Claude" },
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
    timeSel.title = "时间范围";
    for (const o of [
      { value: "0", label: "时间：全部" },
      { value: "7", label: "时间：近 7 天" },
      { value: "30", label: "时间：近 30 天" },
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

    const reindexBtn = document.createElement("button");
    reindexBtn.type = "button";
    reindexBtn.className = "history-refresh";
    reindexBtn.textContent = "重新索引";
    reindexBtn.title = "重新扫描所有会话内容建立搜索索引（有大量新会话时用）";
    reindexBtn.addEventListener("click", () => void this.rebuildIndex(reindexBtn));
    bar.appendChild(reindexBtn);
    this.fulltextOnlyEls.push(reindexBtn);

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
        void this.showIndexIdleHint();
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

  /** 全文模式但无关键词时，拉索引状态给个提示。 */
  private async showIndexIdleHint(): Promise<void> {
    this.resultsEl.replaceChildren();
    this.statusEl.textContent = "查询索引状态…";
    try {
      const st = await invoke<{
        ready: boolean;
        indexedSessions: number;
        indexedMessages: number;
      }>("get_search_index_status");
      if (this.searchMode !== "fulltext") return;
      this.statusEl.textContent = st.ready
        ? `输入关键词搜索会话内容（已索引 ${st.indexedSessions} 个会话 / ${st.indexedMessages} 条消息）`
        : `索引构建中…（已 ${st.indexedSessions} 个会话），稍候再搜`;
    } catch (e) {
      this.statusEl.textContent = `索引状态获取失败：${String(e)}`;
    }
  }

  /**
   * 执行全文搜索。竞态防护：每次调用递增 ftSeq，异步结果回来时若 seq 已过期则丢弃。
   * 索引未就绪（status=indexing）时显示进度并自动重试。
   */
  private async runFullTextSearch(): Promise<void> {
    const query = this.searchInput.value.trim();
    const seq = ++this.ftSeq;
    if (query === "") {
      this.resultsEl.replaceChildren();
      void this.showIndexIdleHint();
      return;
    }
    this.statusEl.textContent = "搜索中…";
    try {
      const resp = await invoke<SearchResponse>("search_history", {
        query,
        includeTools: this.includeTools,
        scope: this.searchScope,
        afterMs: this.searchAfterMs,
        limit: 300,
      });
      if (seq !== this.ftSeq || this.searchMode !== "fulltext") return; // 过期 / 已切模式
      if (resp.status === "indexing") {
        this.resultsEl.replaceChildren();
        this.statusEl.textContent = `索引构建中…（已 ${resp.indexedSessions} 个会话），1 秒后自动重试`;
        window.setTimeout(() => {
          if (seq === this.ftSeq && this.searchMode === "fulltext") {
            void this.runFullTextSearch();
          }
        }, 1000);
        return;
      }
      this.renderSearchResults(resp, query);
    } catch (e) {
      if (seq !== this.ftSeq) return;
      this.statusEl.textContent = `搜索失败：${String(e)}`;
    }
  }

  private renderSearchResults(resp: SearchResponse, query: string): void {
    this.resultsEl.replaceChildren();
    this.statusEl.textContent =
      `「${query}」匹配 ${resp.totalHits} 条 · ${resp.sessionCount} 个会话` +
      (resp.truncated ? "（结果较多，仅显示前若干条）" : "");
    if (resp.sessions.length === 0) {
      this.resultsEl.appendChild(makeStatusRow("无匹配。试试别的关键词，或勾选「含工具内容」扩大范围。"));
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
      host.title = `远端机器：${s.origin}`;
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
    count.textContent = `${s.hitCount} 条命中 · ${formatTimestampSmart(s.updatedAt)}`;
    header.appendChild(count);
    group.appendChild(header);

    for (const hit of s.hits) {
      group.appendChild(this.buildSearchHit(s, hit));
    }
    if (s.hitCount > s.hits.length) {
      const more = document.createElement("div");
      more.className = "search-hit-more";
      more.textContent = `…还有 ${s.hitCount - s.hits.length} 条命中（点任意条打开会话查看全部）`;
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
      hit.kind === "user" ? "你" : hit.kind === "assistant" ? "Claude" : "工具";
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
        // issue #28：远端命中点击走远端只读视图（origin → stream_read_remote_session）。
        origin: s.origin,
      });
    });
    return row;
  }

  private async rebuildIndex(btn: HTMLButtonElement): Promise<void> {
    const prev = btn.textContent;
    btn.disabled = true;
    btn.textContent = "索引中…";
    this.statusEl.textContent = "重新索引中…";
    try {
      await invoke("rebuild_search_index");
      // 重建完后若有关键词则重搜，否则刷新空闲提示
      if (this.searchInput.value.trim() !== "") {
        await this.runFullTextSearch();
      } else {
        await this.showIndexIdleHint();
      }
    } catch (e) {
      this.statusEl.textContent = `重新索引失败：${String(e)}`;
    } finally {
      btn.disabled = false;
      btn.textContent = prev ?? "重新索引";
    }
  }

  // === 列表渲染 ===

  private renderList(): void {
    this.listEl.replaceChildren();
    this.renderOriginFilter(); // F03：同步来源筛选 chip 行
    if (this.projects.length === 0) {
      this.statusEl.textContent =
        "尚无历史会话。新会话写入 <claude_dir>/projects/ 后会出现在这里。";
      return;
    }

    // 项目过滤：搜索匹配（matchProject）+ F03 来源筛选（hiddenOrigins）正交叠加。
    const filteredProjects = this.projects.filter(
      (p) => this.matchProject(p) && !this.hiddenOrigins.has(p.origin ?? ""),
    );

    // 项目排序：live > starred > last_activity desc（与后端默认一致，前端不改）
    const sorted = filteredProjects.slice().sort((a, b) => {
      if (a.hasLive !== b.hasLive)
        return Number(b.hasLive) - Number(a.hasLive);
      const aStar = a.starredCount > 0;
      const bStar = b.starredCount > 0;
      if (aStar !== bStar) return Number(bStar) - Number(aStar);
      return b.lastActivity - a.lastActivity;
    });

    const searchActive = this.filter.length > 0;
    const total = this.projects.reduce((n, p) => n + p.sessionCount, 0);
    const filteredTotal = sorted.reduce((n, p) => n + p.sessionCount, 0);
    this.statusEl.textContent =
      `${sorted.length} 个项目 · ${filteredTotal} 个会话` +
      (filteredTotal !== total ? ` / 共 ${total}` : "");

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
      hint.textContent = "无匹配项目 —— 检查上方搜索或来源筛选。";
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
      void this.loadProjectSessions(proj).then(() => this.renderList());
    }
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
    // 搜索激活时强制展开；否则默认展开（除非用户折叠过）。
    details.open = searchActive || !this.collapsedOrigins.has(key);

    const header = document.createElement("summary");
    header.className = "history-origin-header";
    const indicator = document.createElement("span");
    indicator.className = "history-group-indicator";
    indicator.textContent = "▸";
    header.appendChild(indicator);
    const name = document.createElement("span");
    name.className = "history-origin-name";
    name.textContent = origin ? `[${origin}]` : "本地";
    header.appendChild(name);
    const stats = document.createElement("span");
    stats.className = "history-group-stats";
    const sessionTotal = projects.reduce((n, p) => n + p.sessionCount, 0);
    stats.textContent = `${projects.length} 个项目 · ${sessionTotal} 个会话`;
    header.appendChild(stats);
    details.appendChild(header);

    const body = document.createElement("div");
    body.className = "history-origin-body";
    for (const proj of projects) {
      this.appendProjectGroup(body, proj, searchActive);
    }
    details.appendChild(body);

    // 折叠状态持久化（搜索激活时不写，避免污染用户偏好）。
    details.addEventListener("toggle", () => {
      if (searchActive) return;
      if (details.open) this.collapsedOrigins.delete(key);
      else this.collapsedOrigins.add(key);
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
    label.textContent = "来源：";
    this.originFilterBar.appendChild(label);
    for (const origin of this.orderOrigins(origins)) {
      const key = origin ?? "";
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "history-origin-chip";
      chip.classList.toggle("active", !this.hiddenOrigins.has(key));
      chip.textContent = origin ? `[${origin}]` : "本地";
      chip.title = origin ? `远端 ${origin} 的历史` : "本地历史";
      chip.addEventListener("click", () => {
        if (this.hiddenOrigins.has(key)) this.hiddenOrigins.delete(key);
        else this.hiddenOrigins.add(key);
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
    indicator.textContent = "▸"; // ▸ 折叠指示符（[open] 时 CSS 旋转 90deg）
    header.appendChild(indicator);

    // 项目名前不再加 📁 emoji —— 折叠指示器 + 项目名已经够清，多余的图标视觉噪声

    const name = document.createElement("span");
    name.className = "history-group-name";
    name.textContent = proj.projectName || "(未知项目)";
    header.appendChild(name);

    // issue #16：远端项目组头加 [host] 徽标区分来源
    if (proj.origin) {
      const originBadge = document.createElement("span");
      originBadge.className = "history-origin-badge";
      originBadge.textContent = `[${proj.origin}]`;
      originBadge.title = `远端数据来源：${proj.origin}（只读）`;
      header.appendChild(originBadge);
    }

    const pathLbl = document.createElement("span");
    pathLbl.className = "history-group-path";
    pathLbl.textContent = proj.projectPath;
    pathLbl.title = proj.projectPath;
    header.appendChild(pathLbl);

    const stats = document.createElement("span");
    stats.className = "history-group-stats";
    const chips: string[] = [`${proj.sessionCount} 个会话`];
    if (proj.hasLive) chips.push("● live");
    if (proj.starredCount > 0) chips.push(`★ ${proj.starredCount}`);
    if (this.showHidden && proj.hiddenCount > 0)
      chips.push(`隐藏 ${proj.hiddenCount}`);
    chips.push(formatTimestampSmart(proj.lastActivity));
    stats.textContent = chips.join(" · ");
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
        body.appendChild(makeStatusRow(isLoading ? "加载中…" : "点击加载…"));
        return;
      }
      const visible = cached
        .filter((e) => (this.showHidden ? true : !e.hidden))
        .filter((e) => this.matchSession(e));
      if (visible.length === 0) {
        // 流式加载初期 cache 可能是 [] —— 此时显示 "加载中" 而非 "无会话"
        if (isLoading) {
          body.appendChild(makeStatusRow("加载中…"));
        } else {
          body.appendChild(
            makeStatusRow(
              cached.length === 0
                ? "此项目下无会话（可能已全部物理删除）"
                : "无匹配会话",
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
        body.appendChild(makeStatusRow("继续加载中…"));
      }
    };

    // 跟踪展开状态 + 触发懒加载
    details.addEventListener("toggle", () => {
      if (details.open) {
        this.expandedProjects.add(projectKey(proj));
        if (!this.sessionCache.has(projectKey(proj))) {
          renderBody(); // 显示 "加载中…"
          void this.loadProjectSessions(proj).then(() => {
            // 加载完后再画一次 body
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
      this.statusEl.textContent = `已加载全部 ${this.projects.length} 个项目`;
      return;
    }

    this.loadAllBtn.disabled = true;
    const baseLabel = this.loadAllBtn.textContent;
    const total = pending.length;
    let done = 0;
    const queue = pending.slice();

    const worker = async (): Promise<void> => {
      while (queue.length > 0) {
        const proj = queue.shift();
        if (!proj) break;
        await this.loadProjectSessions(proj);
        done += 1;
        this.statusEl.textContent = `加载中 ${done}/${total} …`;
        this.loadAllBtn.textContent = `加载 ${done}/${total}`;
      }
    };

    try {
      await Promise.all(
        Array.from({ length: HistoryView.LOAD_ALL_CONCURRENCY }, () => worker()),
      );
      this.loadedAll = true;
      this.updateSearchPlaceholder();
      // 重画一次以应用搜索匹配（如果用户已经在搜索框输入）
      this.renderList();
    } finally {
      this.loadAllBtn.disabled = false;
      this.loadAllBtn.textContent = baseLabel ?? "全量加载";
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
      this.statusEl.textContent = `加载 ${toLoad.length} 个项目的会话…`;
      await Promise.all(toLoad.map((p) => this.loadProjectSessions(p)));
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

    // issue #12: fork 树展开 / 折叠按钮（只在有 children 时出现）
    if (childCount > 0) {
      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "history-fork-toggle";
      const expanded = this.expandedForks.has(e.sessionId);
      toggle.textContent = expanded ? "▼" : "▶";
      toggle.title = expanded
        ? `折叠 ${childCount} 个 fork 子会话`
        : `展开 ${childCount} 个 fork 子会话`;
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
      orphanMark.textContent = "↳";
      orphanMark.title = `本会话从 ${e.forkedFromSessionId.slice(0, 8)} fork 而来，但原 session 已不在本项目（可能跨项目 fork 或已物理删除）`;
      row.appendChild(orphanMark);
    }

    const starBtn = document.createElement("button");
    starBtn.type = "button";
    starBtn.className = "history-star";
    starBtn.textContent = e.starred ? "★" : "☆";
    starBtn.title = e.starred ? "取消标星" : "标星";
    if (e.starred) starBtn.classList.add("is-starred");
    starBtn.addEventListener("click", async (ev) => {
      ev.stopPropagation();
      try {
        const next = await invoke<EntryMetadata>("update_history_metadata", {
          sessionId: e.sessionId,
          patch: { starred: !e.starred },
        });
        const wasStarred = e.starred;
        e.starred = next.starred;
        // 同步 project 的 starred_count
        if (!wasStarred && next.starred) proj.starredCount += 1;
        else if (wasStarred && !next.starred)
          proj.starredCount = Math.max(0, proj.starredCount - 1);
        this.renderList();
      } catch (err) {
        console.warn("star update failed:", err);
      }
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
      makeChip(e.isLive ? "live" : "archived", e.isLive ? "history-live" : ""),
      makeChip(`${e.messageCountApprox} 条消息`),
      makeChip(formatTimestampSmart(e.updatedAt)),
    );
    main.appendChild(meta);

    row.appendChild(main);

    const actions = document.createElement("div");
    actions.className = "history-actions";

    const renameBtn = document.createElement("button");
    renameBtn.type = "button";
    renameBtn.className = "history-action";
    renameBtn.textContent = "✎"; // ✎ pencil（BMP，非 emoji）
    renameBtn.title = "重命名（中文 OK）";
    renameBtn.addEventListener("click", async (ev) => {
      ev.stopPropagation();
      const cur = e.customTitle ?? e.aiTitle ?? "";
      const next = window.prompt("自定义标题（留空恢复默认）", cur);
      if (next === null) return;
      try {
        const updated = await invoke<EntryMetadata>("update_history_metadata", {
          sessionId: e.sessionId,
          patch: { customTitle: next.trim() === "" ? null : next.trim() },
        });
        e.customTitle = updated.customTitle;
        this.renderList();
      } catch (err) {
        console.warn("rename failed:", err);
      }
    });
    actions.appendChild(renameBtn);

    const hideBtn = document.createElement("button");
    hideBtn.type = "button";
    hideBtn.className = "history-action";
    // hidden 时按钮指示"恢复显示"用 +；显示时按钮指示"隐藏"用 –（en-dash U+2013）
    hideBtn.textContent = e.hidden ? "+" : "–";
    hideBtn.title = e.hidden ? "取消隐藏" : "隐藏（不删，但默认列表不显示）";
    hideBtn.addEventListener("click", async (ev) => {
      ev.stopPropagation();
      try {
        const updated = await invoke<EntryMetadata>("update_history_metadata", {
          sessionId: e.sessionId,
          patch: { hidden: !e.hidden },
        });
        const wasHidden = e.hidden;
        e.hidden = updated.hidden;
        if (!wasHidden && updated.hidden) proj.hiddenCount += 1;
        else if (wasHidden && !updated.hidden)
          proj.hiddenCount = Math.max(0, proj.hiddenCount - 1);
        this.renderList();
      } catch (err) {
        console.warn("hide toggle failed:", err);
      }
    });
    actions.appendChild(hideBtn);

    const resumeBtn = document.createElement("button");
    resumeBtn.type = "button";
    resumeBtn.className = "history-action";
    resumeBtn.textContent = "↺"; // ↺ anticlockwise circle arrow ("replay")
    if (e.origin) {
      // issue #16：远端会话跑在远端机器上，本地无法 claude --resume（P1c 另议）
      resumeBtn.disabled = true;
      resumeBtn.title = `远端会话（${e.origin}）暂不支持 resume`;
    } else {
      resumeBtn.title = "在新终端窗口里 claude --resume 此会话";
      resumeBtn.addEventListener("click", async (ev) => {
        ev.stopPropagation();
        try {
          await invoke("resume_history_session", {
            sessionId: e.sessionId,
            cwd: e.projectPath,
          });
        } catch (err) {
          showActionFailureToast("恢复失败", String(err));
        }
      });
    }
    actions.appendChild(resumeBtn);

    const deleteBtn = document.createElement("button");
    deleteBtn.type = "button";
    deleteBtn.className = "history-action history-action-danger";
    deleteBtn.textContent = "✕"; // ✕ multiplication X
    // F11：远端会话删除经 SFTP（SS-G 用户数据写豁免，二次确认）；本地走既有物理删除。
    deleteBtn.title = e.origin
      ? `删除远端 [${e.origin}] 的 jsonl（经 SFTP，不可恢复）`
      : "物理删除 jsonl 文件（不可恢复）";
    deleteBtn.addEventListener("click", async (ev) => {
      ev.stopPropagation();
      const label = e.customTitle ?? e.aiTitle ?? e.sessionId.slice(0, 8);
      if (e.origin) {
        // 远端删除更危险（经 SFTP 删远端文件）→ 二次确认。
        const ok1 = window.confirm(
          `删除远端会话「${label}」（机器 ${e.origin}）？\n\n将经 SFTP 物理删除远端 jsonl 文件，Claude Code 之后也无法 resume。\n此操作不可恢复。`,
        );
        if (!ok1) return;
        const ok2 = window.confirm(
          `再次确认：永久删除远端 [${e.origin}] 的「${label}」？`,
        );
        if (!ok2) return;
        try {
          await invoke("delete_remote_history_session", {
            origin: e.origin,
            jsonlPath: e.jsonlPath,
          });
        } catch (err) {
          showActionFailureToast("远端删除失败", String(err));
          return;
        }
      } else {
        const ok = window.confirm(
          `物理删除会话「${label}」？\n\njsonl 文件会被直接删除，Claude Code 之后也无法 resume。\n此操作不可恢复。`,
        );
        if (!ok) return;
        try {
          await invoke("delete_history_session", {
            sessionId: e.sessionId,
            jsonlPath: e.jsonlPath,
          });
        } catch (err) {
          showActionFailureToast("删除失败", String(err));
          return;
        }
      }
      // 成功后：从缓存移除 + 同步 project counts（本地 / 远端一致）。
      const arr = this.sessionCache.get(projectKey(proj));
      if (arr) {
        const idx = arr.findIndex((x) => x.sessionId === e.sessionId);
        if (idx >= 0) arr.splice(idx, 1);
      }
      proj.sessionCount = Math.max(0, proj.sessionCount - 1);
      if (e.starred) proj.starredCount = Math.max(0, proj.starredCount - 1);
      if (e.hidden) proj.hiddenCount = Math.max(0, proj.hiddenCount - 1);
      // 项目内全部删完了 → 也从 projects 列表移除
      if (proj.sessionCount === 0) {
        this.projects = this.projects.filter(
          (p) => projectKey(p) !== projectKey(proj),
        );
        this.sessionCache.delete(projectKey(proj));
        this.expandedProjects.delete(projectKey(proj));
      }
      this.renderList();
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

