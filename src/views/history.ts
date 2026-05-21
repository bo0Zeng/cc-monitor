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
 * 搜索：只匹配项目级字段（name / path）+ 已展开项目内的会话内容。
 * 想全文搜索得点"展开/收起"先全展开（触发批量加载）后再搜。
 *
 * 操作：star / 重命名 / 删除 / 恢复 全部走单条 IPC，本地状态在响应回来后同步。
 *  - star/hide 改变会更新缓存中那条 entry，并同步对应 project 的 starred/hidden_count
 *  - delete 从缓存移除条目，并 -1 project.session_count
 */

import { invoke } from "@tauri-apps/api/core";
import { SessionViewer } from "./session-viewer";

/** 项目级元数据，从 `list_history_projects` 拿。不含 session 内容。 */
interface HistoryProject {
  project_path: string;
  project_name: string;
  /** `<claude_dir>/projects/<encoded-dir>` 的完整路径，作 lazy load 的 key */
  project_dir: string;
  session_count: number;
  starred_count: number;
  hidden_count: number;
  last_activity: number;
  has_live: boolean;
}

/** 会话级详情，从 `list_history_sessions_in_project` 拿。 */
interface HistorySessionEntry {
  session_id: string;
  project_path: string;
  project_name: string;
  ai_title: string | null;
  first_user_excerpt: string;
  started_at: number;
  updated_at: number;
  jsonl_path: string;
  is_live: boolean;
  message_count_approx: number;
  starred: boolean;
  custom_title: string | null;
  hidden: boolean;
}

interface EntryMetadata {
  starred: boolean;
  custom_title: string | null;
  hidden: boolean;
  updated_at: number;
}

/** 组内会话排序模式（顶层布局固定按工作目录分组，不是 sort 选项）。 */
type SortMode = "updated_desc" | "started_desc";

export class HistoryView {
  private container: HTMLElement;
  private root: HTMLElement;
  /** message-stream 的原本子节点；视图关闭时还原 */
  private savedChildren: ChildNode[] = [];

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

  // 子元素
  private listEl!: HTMLElement;
  private searchInput!: HTMLInputElement;
  private statusEl!: HTMLElement;
  /** 列表模式的工具条+列表整体（切到查看器时整块隐藏） */
  private listShell!: HTMLElement;
  /** "全量加载" 按钮 ref；加载中要 disable */
  private loadAllBtn!: HTMLButtonElement;
  /** 当前打开的会话查看器（点击条目进入只读视图）；null = 列表模式 */
  private viewer: SessionViewer | null = null;
  /** project_dir → 用户展开状态。默认折叠；用户主动展开的记下来 */
  private expandedProjects = new Set<string>();

  constructor(container: HTMLElement) {
    this.container = container;
    this.root = this.build();
  }

  async open(): Promise<void> {
    if (this.isOpen) return;
    this.savedChildren = Array.from(this.container.childNodes);
    this.container.replaceChildren(this.root);
    this.isOpen = true;
    this.searchInput.value = "";
    this.filter = "";
    this.closeViewer();
    // 重新打开时清掉旧缓存（避免文件已被外部改动后展示陈旧数据）
    this.sessionCache.clear();
    this.loadingProjects.clear();
    this.loadedAll = false;
    this.updateSearchPlaceholder();
    await this.refresh();
    this.searchInput.focus();
  }

  /** 根据是否已全量加载更新搜索框 placeholder，告知用户搜索覆盖范围 */
  private updateSearchPlaceholder(): void {
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
    this.container.replaceChildren(...this.savedChildren);
    this.savedChildren = [];
    this.isOpen = false;
  }

  /** 打开只读查看器，列表 UI 临时隐藏 */
  private openViewer(entry: HistorySessionEntry): void {
    if (this.viewer) this.viewer.dispose();
    this.viewer = new SessionViewer(() => this.closeViewer());
    this.root.appendChild(this.viewer.element);
    this.listShell.style.display = "none";
    const displayTitle =
      entry.custom_title ??
      entry.ai_title ??
      entry.first_user_excerpt ??
      entry.session_id.slice(0, 8);
    const proj = entry.project_name || entry.project_path || "(未知项目)";
    const subtitle =
      entry.project_path && entry.project_path !== proj
        ? `${proj}  ·  ${entry.project_path}`
        : proj;
    void this.viewer.load({
      jsonlPath: entry.jsonl_path,
      displayTitle,
      subtitle,
    });
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

  private async refresh(): Promise<void> {
    this.statusEl.textContent = "加载项目列表…";
    this.listEl.replaceChildren();
    this.sessionCache.clear();
    this.loadingProjects.clear();
    try {
      const raw = await invoke<HistoryProject[]>("list_history_projects");
      this.projects = raw;
      this.renderList();
    } catch (e) {
      this.statusEl.textContent = `加载失败：${String(e)}`;
    }
  }

  /** 懒加载某个项目下的会话详情。重复调返回同一个 Promise。 */
  private loadProjectSessions(projectDir: string): Promise<void> {
    const inFlight = this.loadingProjects.get(projectDir);
    if (inFlight) return inFlight;
    if (this.sessionCache.has(projectDir)) return Promise.resolve();

    const p = (async () => {
      try {
        const items = await invoke<HistorySessionEntry[]>(
          "list_history_sessions_in_project",
          { projectDir },
        );
        this.sessionCache.set(projectDir, items);
      } catch (e) {
        console.warn(`load sessions in ${projectDir} failed:`, e);
        // 失败也写空数组，避免反复重试；用户可点"刷新"清缓存重来
        this.sessionCache.set(projectDir, []);
      } finally {
        this.loadingProjects.delete(projectDir);
      }
    })();
    this.loadingProjects.set(projectDir, p);
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

    this.searchInput = document.createElement("input");
    this.searchInput.type = "search";
    this.searchInput.className = "history-search";
    // placeholder 在 updateSearchPlaceholder 里根据 loadedAll 动态设
    this.searchInput.addEventListener("input", () => {
      this.filter = this.searchInput.value.trim().toLowerCase();
      this.renderList();
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

    const expandAllBtn = document.createElement("button");
    expandAllBtn.type = "button";
    expandAllBtn.className = "history-refresh";
    expandAllBtn.textContent = "展开/收起";
    expandAllBtn.title = "切换所有项目组的展开状态";
    expandAllBtn.addEventListener("click", () => void this.toggleAll());
    bar.appendChild(expandAllBtn);

    // 全量加载：把每个项目的 session 详情都拉到缓存，让搜索可命中 session 内容
    this.loadAllBtn = document.createElement("button");
    this.loadAllBtn.type = "button";
    this.loadAllBtn.className = "history-refresh";
    this.loadAllBtn.textContent = "全量加载";
    this.loadAllBtn.title =
      "拉取所有项目的会话详情，加载后搜索可匹配 session 内容（ai-title / 首条消息 / sid）";
    this.loadAllBtn.addEventListener("click", () => void this.loadAllSessions());
    bar.appendChild(this.loadAllBtn);

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

    const refreshBtn = document.createElement("button");
    refreshBtn.type = "button";
    refreshBtn.className = "history-refresh";
    refreshBtn.textContent = "刷新";
    refreshBtn.addEventListener("click", () => void this.refresh());
    bar.appendChild(refreshBtn);

    this.listShell.appendChild(bar);

    this.statusEl = document.createElement("div");
    this.statusEl.className = "history-status";
    this.listShell.appendChild(this.statusEl);

    this.listEl = document.createElement("div");
    this.listEl.className = "history-list";
    this.listShell.appendChild(this.listEl);

    // 首次构造时给一份合理 placeholder（open() 里会再刷新一次以反映 loadedAll）
    this.updateSearchPlaceholder();

    return view;
  }

  // === 列表渲染 ===

  private renderList(): void {
    this.listEl.replaceChildren();
    if (this.projects.length === 0) {
      this.statusEl.textContent =
        "尚无历史会话。新会话写入 <claude_dir>/projects/ 后会出现在这里。";
      return;
    }

    // 项目过滤：按项目级字段匹配；若该项目缓存了 sessions，再加上 session 级匹配
    const filteredProjects = this.projects.filter((p) =>
      this.matchProject(p),
    );

    // 项目排序：live > starred > last_activity desc（与后端默认一致，前端不改）
    const sorted = filteredProjects.slice().sort((a, b) => {
      if (a.has_live !== b.has_live)
        return Number(b.has_live) - Number(a.has_live);
      const aStar = a.starred_count > 0;
      const bStar = b.starred_count > 0;
      if (aStar !== bStar) return Number(bStar) - Number(aStar);
      return b.last_activity - a.last_activity;
    });

    const searchActive = this.filter.length > 0;
    const total = this.projects.reduce((n, p) => n + p.session_count, 0);
    const filteredTotal = sorted.reduce((n, p) => n + p.session_count, 0);
    this.statusEl.textContent =
      `${sorted.length} 个项目 · ${filteredTotal} 个会话` +
      (filteredTotal !== total ? ` / 共 ${total}` : "");

    for (const proj of sorted) {
      // 搜索激活 OR 用户主动展开 → 展开（搜索激活时还要确保数据已加载）
      const expanded = searchActive || this.expandedProjects.has(proj.project_dir);
      this.listEl.appendChild(this.buildProjectGroup(proj, expanded));
      // 搜索激活但尚未加载该项目 → 触发加载
      if (searchActive && expanded && !this.sessionCache.has(proj.project_dir)) {
        void this.loadProjectSessions(proj.project_dir).then(() =>
          this.renderList(),
        );
      }
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
    name.textContent = proj.project_name || "(未知项目)";
    header.appendChild(name);

    const pathLbl = document.createElement("span");
    pathLbl.className = "history-group-path";
    pathLbl.textContent = proj.project_path;
    pathLbl.title = proj.project_path;
    header.appendChild(pathLbl);

    const stats = document.createElement("span");
    stats.className = "history-group-stats";
    const chips: string[] = [`${proj.session_count} 个会话`];
    if (proj.has_live) chips.push("● live");
    if (proj.starred_count > 0) chips.push(`★ ${proj.starred_count}`);
    if (this.showHidden && proj.hidden_count > 0)
      chips.push(`隐藏 ${proj.hidden_count}`);
    chips.push(formatTime(proj.last_activity));
    stats.textContent = chips.join(" · ");
    header.appendChild(stats);

    details.appendChild(header);

    const body = document.createElement("div");
    body.className = "history-group-body";
    details.appendChild(body);

    // 渲染 body：根据缓存命中情况
    const renderBody = () => {
      body.replaceChildren();
      const cached = this.sessionCache.get(proj.project_dir);
      if (cached === undefined) {
        if (this.loadingProjects.has(proj.project_dir)) {
          body.appendChild(makeStatusRow("加载中…"));
        } else {
          body.appendChild(makeStatusRow("点击加载…"));
        }
        return;
      }
      const visible = cached
        .filter((e) => (this.showHidden ? true : !e.hidden))
        .filter((e) => this.matchSession(e));
      if (visible.length === 0) {
        body.appendChild(
          makeStatusRow(
            cached.length === 0
              ? "此项目下无会话（可能已全部物理删除）"
              : "无匹配会话",
          ),
        );
        return;
      }
      const sorted = this.applySort(visible);
      for (const e of sorted) body.appendChild(this.buildEntryRow(e, proj));
    };

    // 跟踪展开状态 + 触发懒加载
    details.addEventListener("toggle", () => {
      if (details.open) {
        this.expandedProjects.add(proj.project_dir);
        if (!this.sessionCache.has(proj.project_dir)) {
          renderBody(); // 显示 "加载中…"
          void this.loadProjectSessions(proj.project_dir).then(() => {
            // 加载完后再画一次 body
            if (details.isConnected) renderBody();
          });
        } else {
          renderBody();
        }
      } else {
        this.expandedProjects.delete(proj.project_dir);
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
    const pending = this.projects.filter((p) => !this.sessionCache.has(p.project_dir));
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
        await this.loadProjectSessions(proj.project_dir);
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
    const projectDirs = this.projects.map((p) => p.project_dir);
    if (projectDirs.length === 0) return;
    const allExpanded = projectDirs.every((d) => this.expandedProjects.has(d));
    if (allExpanded) {
      this.expandedProjects.clear();
      this.renderList();
      return;
    }
    // 全展开：触发未缓存项目的并发加载
    for (const d of projectDirs) this.expandedProjects.add(d);
    this.renderList();
    const toLoad = projectDirs.filter((d) => !this.sessionCache.has(d));
    if (toLoad.length > 0) {
      this.statusEl.textContent = `加载 ${toLoad.length} 个项目的会话…`;
      await Promise.all(toLoad.map((d) => this.loadProjectSessions(d)));
      this.renderList();
    }
  }

  // === 过滤 / 排序 ===

  /** 项目级匹配：name / path / project_dir。命中后可能再叠 session 级匹配 */
  private matchProject(p: HistoryProject): boolean {
    if (!this.filter) return true;
    const hay = `${p.project_name}\n${p.project_path}\n${p.project_dir}`.toLowerCase();
    if (hay.includes(this.filter)) return true;
    // project 元数据不命中时，看看缓存里的 sessions 是否有命中（仅对已加载项目）
    const cached = this.sessionCache.get(p.project_dir);
    if (!cached) return false;
    return cached.some((e) => this.matchSession(e));
  }

  /** 会话级匹配：ai_title / customTitle / first_user / sessionId */
  private matchSession(e: HistorySessionEntry): boolean {
    if (!this.filter) return true;
    const hay = [
      e.ai_title ?? "",
      e.custom_title ?? "",
      e.first_user_excerpt,
      e.session_id,
    ]
      .join("\n")
      .toLowerCase();
    return hay.includes(this.filter);
  }

  private applySort(items: HistorySessionEntry[]): HistorySessionEntry[] {
    const arr = items.slice();
    switch (this.sort) {
      case "started_desc":
        arr.sort(
          (a, b) =>
            Number(b.starred) - Number(a.starred) || b.started_at - a.started_at,
        );
        break;
      case "updated_desc":
      default:
        arr.sort(
          (a, b) =>
            Number(b.starred) - Number(a.starred) || b.updated_at - a.updated_at,
        );
        break;
    }
    return arr;
  }

  // === 会话行 ===

  private buildEntryRow(
    e: HistorySessionEntry,
    proj: HistoryProject,
  ): HTMLElement {
    const row = document.createElement("div");
    row.className = "history-entry";
    if (e.hidden) row.classList.add("is-hidden-entry");
    if (e.is_live) row.classList.add("is-live-entry");

    row.addEventListener("click", (ev) => {
      const target = ev.target as HTMLElement | null;
      if (target?.closest(".history-star, .history-action")) return;
      this.openViewer(e);
    });

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
          sessionId: e.session_id,
          patch: { starred: !e.starred },
        });
        const wasStarred = e.starred;
        e.starred = next.starred;
        // 同步 project 的 starred_count
        if (!wasStarred && next.starred) proj.starred_count += 1;
        else if (wasStarred && !next.starred)
          proj.starred_count = Math.max(0, proj.starred_count - 1);
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
      e.custom_title ??
      e.ai_title ??
      e.first_user_excerpt ??
      e.session_id.slice(0, 8);
    title.textContent = displayTitle;
    main.appendChild(title);

    if (e.custom_title && e.ai_title && e.custom_title !== e.ai_title) {
      const subtitle = document.createElement("div");
      subtitle.className = "history-subtitle";
      subtitle.textContent = e.ai_title;
      main.appendChild(subtitle);
    }

    if (e.first_user_excerpt && e.first_user_excerpt !== displayTitle) {
      const excerpt = document.createElement("div");
      excerpt.className = "history-excerpt";
      excerpt.textContent = e.first_user_excerpt;
      main.appendChild(excerpt);
    }

    const meta = document.createElement("div");
    meta.className = "history-meta";
    meta.append(
      makeChip(e.is_live ? "live" : "archived", e.is_live ? "history-live" : ""),
      makeChip(`${e.message_count_approx} 条消息`),
      makeChip(formatTime(e.updated_at)),
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
      const cur = e.custom_title ?? e.ai_title ?? "";
      const next = window.prompt("自定义标题（留空恢复默认）", cur);
      if (next === null) return;
      try {
        const updated = await invoke<EntryMetadata>("update_history_metadata", {
          sessionId: e.session_id,
          patch: { customTitle: next.trim() === "" ? null : next.trim() },
        });
        e.custom_title = updated.custom_title;
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
          sessionId: e.session_id,
          patch: { hidden: !e.hidden },
        });
        const wasHidden = e.hidden;
        e.hidden = updated.hidden;
        if (!wasHidden && updated.hidden) proj.hidden_count += 1;
        else if (wasHidden && !updated.hidden)
          proj.hidden_count = Math.max(0, proj.hidden_count - 1);
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
    resumeBtn.title = "在新终端窗口里 claude --resume 此会话";
    resumeBtn.addEventListener("click", async (ev) => {
      ev.stopPropagation();
      try {
        await invoke("resume_history_session", {
          sessionId: e.session_id,
          cwd: e.project_path,
        });
      } catch (err) {
        window.alert(`恢复失败：${String(err)}`);
      }
    });
    actions.appendChild(resumeBtn);

    const deleteBtn = document.createElement("button");
    deleteBtn.type = "button";
    deleteBtn.className = "history-action history-action-danger";
    deleteBtn.textContent = "✕"; // ✕ multiplication X
    deleteBtn.title = "物理删除 jsonl 文件（不可恢复）";
    deleteBtn.addEventListener("click", async (ev) => {
      ev.stopPropagation();
      const label = e.custom_title ?? e.ai_title ?? e.session_id.slice(0, 8);
      const ok = window.confirm(
        `物理删除会话「${label}」？\n\njsonl 文件会被直接删除，Claude Code 之后也无法 resume。\n此操作不可恢复。`,
      );
      if (!ok) return;
      try {
        await invoke("delete_history_session", {
          sessionId: e.session_id,
          jsonlPath: e.jsonl_path,
        });
        // 从缓存移除 + 同步 project counts
        const arr = this.sessionCache.get(proj.project_dir);
        if (arr) {
          const idx = arr.findIndex((x) => x.session_id === e.session_id);
          if (idx >= 0) arr.splice(idx, 1);
        }
        proj.session_count = Math.max(0, proj.session_count - 1);
        if (e.starred)
          proj.starred_count = Math.max(0, proj.starred_count - 1);
        if (e.hidden)
          proj.hidden_count = Math.max(0, proj.hidden_count - 1);
        // 项目内全部删完了 → 也从 projects 列表移除
        if (proj.session_count === 0) {
          this.projects = this.projects.filter(
            (p) => p.project_dir !== proj.project_dir,
          );
          this.sessionCache.delete(proj.project_dir);
          this.expandedProjects.delete(proj.project_dir);
        }
        this.renderList();
      } catch (err) {
        window.alert(`删除失败：${String(err)}`);
      }
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

function formatTime(ms: number): string {
  if (!ms) return "—";
  try {
    const d = new Date(ms);
    const today = new Date();
    const sameDay =
      d.getFullYear() === today.getFullYear() &&
      d.getMonth() === today.getMonth() &&
      d.getDate() === today.getDate();
    if (sameDay) {
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    }
    return d.toLocaleString([], {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return String(ms);
  }
}
