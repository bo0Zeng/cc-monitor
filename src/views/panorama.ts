/**
 * Batch15-P2：代码全景视图（纯 canvas 自研，零图库依赖）。
 *
 * 顶栏「全景」按钮 → 本 overlay（照 `HistoryView` 的 body-level fixed overlay 范式，与
 * 历史浏览器同层级——都是"跳出实时流的分析型视图"）。对当前**本地**会话的 cwd 建 code-picture
 * 索引 → canvas 画「代码库地图」（子系统色块聚类 + 脊柱文件圆 + 入口点描环）+ 覆盖信号 banner
 * + 符号搜索 + 节点详情侧栏。
 *
 * ## hide 不卸载
 * 索引/overview/布局都贵，`close()` 只把 root `display:none`（不 `remove`），`open()` 再显示；
 * 缓存 `loadedRepo`，同仓重开直接复用已算好的布局，只有活跃仓变了才重新索引/加载。
 *
 * ## 渲染形态（P2 = 聚类气泡地图，非力导边图）
 * Overview 是文件级、**无文件间边**，故画确定性聚类气泡地图（坐标/命中/打包在纯逻辑
 * `../panorama/layout.ts`，vitest 覆盖）。函数级调用子图（有边、力导）留 P4。
 *
 * ## 诚实性
 * `unresolved_calls>0 || parse_errors>0` → 顶部覆盖信号 banner（`coverageBanner`）；符号
 * 详情里如实呈现每条边的 `confidence`（Exact/Heuristic/DynamicGuess，非 sound）。
 */

import * as api from "../panorama/api";
import type { Overview, NodeView, Symbol, Edge, Confidence } from "../panorama/types";
import {
  computeLayout,
  fitViewport,
  zoomAt,
  hitTest,
  coverageBanner,
  touchedFilesFromIds,
  countShown,
  type PanoramaLayout,
  type Viewport,
  type FileBubble,
} from "../panorama/layout";
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { showActionFailureToast } from "../error-toast";

/** main.ts 注入的活跃仓信息取值器（读活跃 tab 的 cwd/origin）。 */
type RepoInfoGetter = () => { cwd: string; origin: string | null } | null;

export class PanoramaView implements OverlayHandle {
  private root: HTMLElement;
  private mounted = false;
  private isOpen = false;

  // chrome
  private bannerEl!: HTMLElement;
  private canvas!: HTMLCanvasElement;
  private canvasWrap!: HTMLElement;
  private tooltipEl!: HTMLElement;
  private loadingEl!: HTMLElement;
  private messageEl!: HTMLElement;
  private searchInput!: HTMLInputElement;
  private sidebarEl!: HTMLElement;
  private refreshBtn!: HTMLButtonElement;
  // F70：会话高亮图例条 + 文本（默认隐藏）。
  private highlightBarEl!: HTMLElement;
  private highlightTextEl!: HTMLElement;

  // 状态
  private overview: Overview | null = null;
  private layout: PanoramaLayout | null = null;
  private viewport: Viewport = { x: 0, y: 0, scale: 1 };
  /** 已成功加载/索引的仓（活跃仓不变则重开复用，不再重索引）。 */
  private loadedRepo: string | null = null;
  /** 当前视图针对的仓（远端时为 null）。搜索/刷新/详情都用它。 */
  private repo: string | null = null;
  /** 加载代际号，防 repo 切换时旧异步结果覆盖新的（竞态）。 */
  private loadSeq = 0;
  /** 搜索代际号（同上）。 */
  private searchSeq = 0;
  /** F70：本会话改动集在图上的高亮（仓库相对文件段集；null=不高亮）。 */
  private touchedFiles: Set<string> | null = null;
  /** F70：repo 尚未加载完（enable-gate 未索引）时暂存待高亮请求。**带 repo 标签**——消费前校验
   * 属当前仓，防「暂存 B 的高亮 → 切 C 重开 → 套到 C 上」跨仓泄漏（图例骗人）。 */
  private pendingHighlight: { repo: string; files: string[] } | null = null;
  /** F70：高亮独立世代号——**不借 loadSeq**（借了会在 applyOverview 里推进 loadSeq、卡死 refresh
   * 按钮的 finally 判定）。切仓由 highlightSession 的 `this.repo !== repo` 检查兜。 */
  private highlightSeq = 0;

  // 交互
  private hovered: FileBubble | null = null;
  private drawScheduled = false;
  private panning = false;
  private panStart = { x: 0, y: 0 };
  private panOrigin = { x: 0, y: 0 };
  private movedDuringPress = false;

  constructor(private getRepo: RepoInfoGetter) {
    this.root = this.build();
    // 全局 resize：仅在打开时重算 canvas 尺寸并重画（隐藏时 no-op）。
    window.addEventListener("resize", () => {
      if (this.isOpen) {
        this.resizeCanvas();
        this.scheduleDraw();
      }
    });
  }

  // === 生命周期 ===

  async open(): Promise<void> {
    if (this.isOpen) return;
    if (!this.mounted) {
      document.body.appendChild(this.root);
      this.mounted = true;
    }
    this.root.style.display = "flex";
    this.isOpen = true;
    dispatcher.pushOverlay(this);
    this.resizeCanvas();
    await this.evaluateRepo();
  }

  close(): void {
    if (!this.isOpen) return;
    this.root.style.display = "none";
    this.isOpen = false;
    this.hideTooltip();
    // F70：清暂存的待高亮请求（未消费就关掉 → 别留到下次开别的仓时误消费）。
    this.pendingHighlight = null;
    dispatcher.popOverlay(this);
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  /** OverlayHandle：Esc 优先收起侧栏，否则关整个全景视图。 */
  handleEsc(): boolean {
    if (this.sidebarEl.classList.contains("is-open")) {
      this.closeSidebar();
      return true;
    }
    this.close();
    return true;
  }

  // === 打开时决定：远端提示 / 复用 / 加载 ===

  private async evaluateRepo(): Promise<void> {
    const info = this.getRepo();
    if (!info || !info.cwd) {
      this.repo = null;
      this.showMessage(
        "无可索引的仓库",
        "当前没有活跃会话，或活跃会话没有已知工作目录。切到一个本地会话再打开全景。",
      );
      return;
    }
    if (info.origin !== null) {
      // 远端会话：代码在远端机，本地 code-picture 索引不到（诚实提示，不索引）。
      this.repo = null;
      this.showMessage(
        "全景仅支持本地仓库",
        `当前会话来自远端机 [${info.origin}]，代码不在本机，无法建立本地 code-picture 索引。切到一个本地会话再打开全景。`,
      );
      return;
    }
    this.repo = info.cwd;
    // 同仓且已加载过 → 直接复用（hide 不卸载的意义）。
    if (this.loadedRepo === info.cwd && this.overview && this.layout) {
      this.hideMessage();
      this.scheduleDraw();
      return;
    }
    await this.load(info.cwd);
  }

  /** 索引（如需）+ 拉 overview + 算布局 + 适配视口。 */
  private async load(repo: string): Promise<void> {
    const seq = ++this.loadSeq;
    this.hideMessage();
    this.closeSidebar();
    // F70：换仓/重载 → 清旧会话高亮 + 清暂存的待高亮请求（否则上一个仓的文件集/pending 会套到
    // 新仓气泡上，全压暗、图例骗人）。
    this.touchedFiles = null;
    this.pendingHighlight = null;
    this.updateHighlightLegend(0, 0);
    this.showLoading("检查索引状态…");
    try {
      const st = await api.status(repo);
      if (seq !== this.loadSeq) return;
      // F69（补 D20：代码分析默认关、每仓手动开启）：门只卡**首次启用**——从未索引
      // （symbols===0）= 未启用本仓分析 → **不自动扫描**，给显式「建立索引」手势。
      if (api.panoramaLoadDecision(st) === "enable-gate") {
        this.hideLoading();
        this.showMessage(
          "尚未为本仓建立代码索引",
          "全景图需先解析本仓代码（tree-sitter，大仓较慢）。索引是纯缓存，存在 cc-monitor 数据目录、不写进你的仓库。点下方按钮启用本仓代码分析。",
          {
            label: "建立索引（启用本仓代码分析）",
            // 显式一次性 disable，防连点（不依赖 hideMessage 的 display:none 隐式防护）。
            onClick: (btn) => {
              btn.disabled = true;
              void this.enableAndIndex(repo);
            },
          },
        );
        return;
      }
      // 已启用（symbols>0）：陈旧则自动重建（用户此前已 opt-in，保持新鲜属正常运行、非新
      // opt-in——避免静默展示过期图）。非陈旧直接加载。
      if (st.stale) {
        this.showLoading("索引已陈旧，重建中…");
        await api.index(repo);
        if (seq !== this.loadSeq) return;
      }
      this.showLoading("加载全景…");
      const ov = await api.overview(repo);
      if (seq !== this.loadSeq) return;
      this.applyOverview(ov, repo);
    } catch (e) {
      if (seq !== this.loadSeq) return;
      this.hideLoading();
      showActionFailureToast("全景加载失败", String(e));
      this.showMessage("加载失败", String(e));
    }
  }

  /** F69（D20 opt-in）：用户显式点「建立索引」才跑扫描（tree-sitter 全仓解析）——默认关。 */
  private async enableAndIndex(repo: string): Promise<void> {
    const seq = ++this.loadSeq;
    this.hideMessage();
    this.showLoading("首次建立索引中…（大仓较慢，请稍候）");
    try {
      await api.index(repo);
      if (seq !== this.loadSeq) return;
      this.showLoading("加载全景…");
      const ov = await api.overview(repo);
      if (seq !== this.loadSeq) return;
      this.applyOverview(ov, repo);
    } catch (e) {
      if (seq !== this.loadSeq) return;
      this.hideLoading();
      showActionFailureToast("建立索引失败", String(e));
      this.showMessage("建立索引失败", String(e));
    }
  }

  private applyOverview(ov: Overview, repo: string): void {
    this.overview = ov;
    this.loadedRepo = repo;
    this.layout = computeLayout(ov);
    this.hideLoading();
    this.updateBanner();
    this.fitView();
    if (this.layout.bubbles.length === 0) {
      this.showMessage(
        "暂无可视化的脊柱文件",
        "该仓的符号太少、或大部分文件解析失败——看顶部覆盖信号。仍可用上方搜索框查符号。",
      );
    } else {
      this.hideMessage();
    }
    this.scheduleDraw();
    // F70：repo 加载完，若有暂存的待高亮请求（点会话时仓还没索引）→ 校验属当前仓再消费。
    // **消费或丢弃都清掉**——pending 属别的仓（切仓后残留）则丢弃，绝不套到本仓上（防图例骗人）。
    if (this.pendingHighlight) {
      const pending = this.pendingHighlight;
      this.pendingHighlight = null;
      if (pending.repo === repo) void this.highlightSession(pending.files);
    }
  }

  /**
   * F70（护城河）：在全景图上高亮「本会话改过哪些代码节点」。`files` = 会话写类工具碰过的
   * 文件（绝对路径）→ 后端 `panorama_touching` 映射成符号 id → 取文件段 → 命中气泡描环、
   * 其余压暗。仅本地仓（`this.repo!=null`）；仓未索引（enable-gate）→ 暂存，索引好后补画。
   */
  async highlightSession(files: string[]): Promise<void> {
    if (!this.repo) {
      showActionFailureToast("无法高亮", "当前不是本地仓库视图，无法高亮会话改动。");
      return;
    }
    if (files.length === 0) {
      showActionFailureToast("无改动可高亮", "该会话没有用编辑类工具改过文件。");
      return;
    }
    const repo = this.repo;
    // 仓还没索引好（enable-gate 未建索引 / 正加载）→ 带 repo 标签暂存，applyOverview 校验后消费。
    if (!this.layout) {
      this.pendingHighlight = { repo, files };
      return;
    }
    const seq = ++this.highlightSeq; // 独立世代号（不借 loadSeq，免卡 refresh 按钮）
    try {
      const ids = await api.touching(repo, files, []);
      // 三重校验：本次高亮未被更晚的高亮作废 / 仓没变 / 布局还在（切仓由 this.repo!==repo 兜）。
      if (seq !== this.highlightSeq || this.repo !== repo || !this.layout) return;
      const touched = touchedFilesFromIds(ids);
      this.touchedFiles = touched;
      // 图例用「碰过的文件数」(files.length，含未解析/非脊柱的) vs「图上高亮数」(shown)——诚实
      // 呈现差值，别让"看着全高亮了"骗人（呼应全景诚实性铁律）。
      this.updateHighlightLegend(files.length, countShown(this.layout.bubbles, touched));
      this.scheduleDraw();
    } catch (e) {
      if (seq !== this.loadSeq) return;
      showActionFailureToast("高亮会话改动失败", String(e));
    }
  }

  /** F70：清除高亮，恢复常态。 */
  private clearHighlight(): void {
    this.touchedFiles = null;
    this.pendingHighlight = null;
    this.updateHighlightLegend(0, 0);
    this.scheduleDraw();
  }

  /** 刷新按钮：重建索引 → 重拉 overview → 重画。 */
  private async refresh(): Promise<void> {
    if (!this.repo) return;
    const repo = this.repo;
    const seq = ++this.loadSeq;
    this.refreshBtn.disabled = true;
    this.showLoading("重建索引中…");
    try {
      await api.reindex(repo);
      if (seq !== this.loadSeq) return;
      this.showLoading("加载全景…");
      const ov = await api.overview(repo);
      if (seq !== this.loadSeq) return;
      this.applyOverview(ov, repo);
    } catch (e) {
      if (seq !== this.loadSeq) return;
      this.hideLoading();
      showActionFailureToast("重建索引失败", String(e));
    } finally {
      if (seq === this.loadSeq) this.refreshBtn.disabled = false;
    }
  }

  // === DOM 构建 ===

  private build(): HTMLElement {
    const view = document.createElement("div");
    view.className = "panorama-view";
    view.style.display = "none";

    // 顶栏
    const bar = document.createElement("div");
    bar.className = "panorama-bar";

    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "panorama-btn panorama-back";
    closeBtn.textContent = "← 返回";
    closeBtn.addEventListener("click", () => this.close());
    bar.appendChild(closeBtn);

    const title = document.createElement("span");
    title.className = "panorama-title";
    title.textContent = "代码全景";
    bar.appendChild(title);

    this.searchInput = document.createElement("input");
    this.searchInput.type = "search";
    this.searchInput.className = "panorama-search";
    this.searchInput.placeholder = "搜索符号（函数 / 方法 / 类名子串）· 回车";
    let debounce: number | undefined;
    this.searchInput.addEventListener("input", () => {
      window.clearTimeout(debounce);
      const q = this.searchInput.value.trim();
      if (q === "") {
        this.closeSidebar();
        return;
      }
      debounce = window.setTimeout(() => void this.runSearch(q), 250);
    });
    this.searchInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        window.clearTimeout(debounce);
        const q = this.searchInput.value.trim();
        if (q) void this.runSearch(q);
      }
    });
    bar.appendChild(this.searchInput);

    const fitBtn = document.createElement("button");
    fitBtn.type = "button";
    fitBtn.className = "panorama-btn";
    fitBtn.textContent = "适配";
    fitBtn.title = "重置视图，居中铺满";
    fitBtn.addEventListener("click", () => {
      this.fitView();
      this.scheduleDraw();
    });
    bar.appendChild(fitBtn);

    this.refreshBtn = document.createElement("button");
    this.refreshBtn.type = "button";
    this.refreshBtn.className = "panorama-btn";
    this.refreshBtn.textContent = "刷新";
    this.refreshBtn.title = "重建索引（改了代码后刷新全景）";
    this.refreshBtn.addEventListener("click", () => void this.refresh());
    bar.appendChild(this.refreshBtn);

    // F71：文档漂移——列仓里 .md 指向已失效的悬空链接。
    const driftBtn = document.createElement("button");
    driftBtn.type = "button";
    driftBtn.className = "panorama-btn";
    driftBtn.textContent = "文档漂移";
    driftBtn.title = "仓里 .md 指向的目标文件/符号已失效（悬空链接）。反映上次索引快照，改了代码请先刷新。";
    driftBtn.addEventListener("click", () => void this.showDrift());
    bar.appendChild(driftBtn);

    view.appendChild(bar);

    // 覆盖信号 banner（默认隐藏）
    this.bannerEl = document.createElement("div");
    this.bannerEl.className = "panorama-banner";
    this.bannerEl.style.display = "none";
    view.appendChild(this.bannerEl);

    // F70：会话高亮图例条（默认隐藏）——碰了几个文件、图上高亮几个 + 清除按钮。
    this.highlightBarEl = document.createElement("div");
    this.highlightBarEl.className = "panorama-highlight-bar";
    this.highlightBarEl.style.display = "none";
    this.highlightTextEl = document.createElement("span");
    this.highlightBarEl.appendChild(this.highlightTextEl);
    const clearBtn = document.createElement("button");
    clearBtn.type = "button";
    clearBtn.className = "panorama-btn panorama-highlight-clear";
    clearBtn.textContent = "清除高亮";
    clearBtn.addEventListener("click", () => this.clearHighlight());
    this.highlightBarEl.appendChild(clearBtn);
    view.appendChild(this.highlightBarEl);

    // 主体：画布 + 侧栏
    const body = document.createElement("div");
    body.className = "panorama-body";

    this.canvasWrap = document.createElement("div");
    this.canvasWrap.className = "panorama-canvas-wrap";

    this.canvas = document.createElement("canvas");
    this.canvas.className = "panorama-canvas";
    this.canvasWrap.appendChild(this.canvas);
    this.bindCanvasEvents();

    this.tooltipEl = document.createElement("div");
    this.tooltipEl.className = "panorama-tooltip";
    this.tooltipEl.style.display = "none";
    this.canvasWrap.appendChild(this.tooltipEl);

    this.loadingEl = document.createElement("div");
    this.loadingEl.className = "panorama-loading";
    this.loadingEl.style.display = "none";
    this.canvasWrap.appendChild(this.loadingEl);

    this.messageEl = document.createElement("div");
    this.messageEl.className = "panorama-message";
    this.messageEl.style.display = "none";
    this.canvasWrap.appendChild(this.messageEl);

    body.appendChild(this.canvasWrap);

    this.sidebarEl = document.createElement("div");
    this.sidebarEl.className = "panorama-sidebar";
    body.appendChild(this.sidebarEl);

    view.appendChild(body);
    return view;
  }

  // === 覆盖信号 / loading / message ===

  private updateBanner(): void {
    const text = this.overview ? coverageBanner(this.overview) : null;
    if (text) {
      this.bannerEl.textContent = text;
      this.bannerEl.style.display = "";
    } else {
      this.bannerEl.style.display = "none";
    }
  }

  /** F70：更新会话高亮图例条。total=会话碰过的文件数；shown=其中图上有气泡（脊柱文件）的数。 */
  private updateHighlightLegend(total: number, shown: number): void {
    if (total <= 0) {
      this.highlightBarEl.style.display = "none";
      this.highlightTextEl.textContent = "";
      return;
    }
    const extra = total - shown;
    this.highlightTextEl.textContent =
      `本会话改了 ${total} 个文件，图上高亮 ${shown} 个` +
      (extra > 0 ? `（其余 ${extra} 个为非脊柱文件 / 不在本仓，未画）` : "");
    this.highlightBarEl.style.display = "";
  }

  private showLoading(text: string): void {
    this.loadingEl.replaceChildren();
    const spin = document.createElement("div");
    spin.className = "panorama-spinner";
    this.loadingEl.appendChild(spin);
    const label = document.createElement("div");
    label.className = "panorama-loading-text";
    label.textContent = text;
    this.loadingEl.appendChild(label);
    this.loadingEl.style.display = "";
  }
  private hideLoading(): void {
    this.loadingEl.style.display = "none";
  }

  private showMessage(
    headline: string,
    body: string,
    action?: { label: string; onClick: (btn: HTMLButtonElement) => void },
  ): void {
    this.messageEl.replaceChildren();
    const h = document.createElement("div");
    h.className = "panorama-message-headline";
    h.textContent = headline;
    this.messageEl.appendChild(h);
    const b = document.createElement("div");
    b.className = "panorama-message-body";
    b.textContent = body;
    this.messageEl.appendChild(b);
    // F69（D20 opt-in）：可选行动按钮（如「建立索引」——把扫描门在显式手势后）。
    if (action) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "panorama-btn panorama-message-action";
      btn.textContent = action.label;
      btn.addEventListener("click", () => action.onClick(btn));
      this.messageEl.appendChild(btn);
    }
    this.messageEl.style.display = "";
  }
  private hideMessage(): void {
    this.messageEl.style.display = "none";
  }

  // === canvas 尺寸（devicePixelRatio 清晰）===

  private resizeCanvas(): void {
    const rect = this.canvasWrap.getBoundingClientRect();
    const cssW = Math.max(0, Math.floor(rect.width));
    const cssH = Math.max(0, Math.floor(rect.height));
    if (cssW === 0 || cssH === 0) return;
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.round(cssW * dpr);
    this.canvas.height = Math.round(cssH * dpr);
    this.canvas.style.width = `${cssW}px`;
    this.canvas.style.height = `${cssH}px`;
  }

  private cssSize(): { w: number; h: number } {
    const dpr = window.devicePixelRatio || 1;
    return { w: this.canvas.width / dpr, h: this.canvas.height / dpr };
  }

  private fitView(): void {
    if (!this.layout) return;
    const { w, h } = this.cssSize();
    this.viewport = fitViewport(this.layout.width, this.layout.height, w, h);
  }

  // === 绘制 ===

  private scheduleDraw(): void {
    if (this.drawScheduled) return;
    this.drawScheduled = true;
    requestAnimationFrame(() => {
      this.drawScheduled = false;
      this.draw();
    });
  }

  private draw(): void {
    const ctx = this.canvas.getContext("2d");
    if (!ctx || !this.layout) return;
    const dpr = window.devicePixelRatio || 1;
    const { w, h } = this.cssSize();
    if (w === 0 || h === 0) return;
    // 基变换 = dpr（之后所有坐标用 CSS 像素，pan/zoom 走 viewport 手动变换 →
    // 文本用恒定 screen 字号，圆随 zoom 缩放）。
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const cs = getComputedStyle(document.documentElement);
    const text2 = cs.getPropertyValue("--text-2").trim() || "#a8a39a";
    const accent = cs.getPropertyValue("--accent").trim() || "#c96442";
    // canvas ctx.font 不认 var()，必须是具体字体栈 → 读 CSS 变量的解析值。
    const fontBase = cs.getPropertyValue("--font-base").trim() || "sans-serif";
    const fontMono = cs.getPropertyValue("--font-mono").trim() || "monospace";
    const vp = this.viewport;

    // 子系统色块区域
    for (const rg of this.layout.regions) {
      const x = rg.boxX * vp.scale + vp.x;
      const y = rg.boxY * vp.scale + vp.y;
      const rw = rg.boxW * vp.scale;
      const rh = rg.boxH * vp.scale;
      if (x + rw < 0 || y + rh < 0 || x > w || y > h) continue; // 视口剔除
      ctx.fillStyle = `hsl(${rg.hue} 55% 52% / 0.12)`;
      ctx.strokeStyle = `hsl(${rg.hue} 50% 60% / 0.55)`;
      ctx.lineWidth = 1.5;
      roundRectPath(ctx, x, y, rw, rh, Math.min(14, rw / 2, rh / 2));
      ctx.fill();
      ctx.stroke();
      // 标签（恒定字号，随 zoom 大致可读）。textAlign/Baseline 跨帧保留，显式设左对齐
      // （上一帧末尾气泡把它设成了 center）。
      ctx.font = `600 13px ${fontBase}`;
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      ctx.fillStyle = text2;
      const lx = rg.labelX * vp.scale + vp.x;
      const ly = rg.labelY * vp.scale + vp.y;
      const label = `${rg.label}  (${rg.fileCount})`;
      // 简单裁剪：标签不超出盒宽
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, rw, rh);
      ctx.clip();
      ctx.fillText(label, lx + 4, ly);
      ctx.restore();
    }

    // 脊柱文件圆
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const hl = this.touchedFiles; // F70：高亮态（null=不高亮；否则=本会话碰过的文件集）
    for (const b of this.layout.bubbles) {
      const cx = b.x * vp.scale + vp.x;
      const cy = b.y * vp.scale + vp.y;
      const r = b.r * vp.scale;
      if (cx + r < 0 || cy + r < 0 || cx - r > w || cy - r > h) continue; // 剔除
      // F70：高亮态下压暗未命中气泡，命中气泡满亮（下方再描粗环）。
      const touched = hl ? hl.has(b.file) : false;
      ctx.globalAlpha = hl && !touched ? 0.22 : 1;
      ctx.beginPath();
      ctx.arc(cx, cy, r, 0, Math.PI * 2);
      ctx.fillStyle = `hsl(${b.hue} 60% 55% / 0.9)`;
      ctx.fill();
      // 入口点描环
      if (b.isEntry) {
        ctx.lineWidth = Math.max(2, r * 0.14);
        ctx.strokeStyle = accent;
        ctx.stroke();
      } else {
        ctx.lineWidth = 1;
        ctx.strokeStyle = `hsl(${b.hue} 40% 30% / 0.7)`;
        ctx.stroke();
      }
      // F70：命中「本会话改动」→ 醒目描环 + 外发光。
      if (hl && touched) {
        ctx.beginPath();
        ctx.arc(cx, cy, r + 3, 0, Math.PI * 2);
        ctx.lineWidth = Math.max(3, r * 0.18);
        ctx.strokeStyle = accent;
        ctx.save();
        ctx.shadowColor = accent;
        ctx.shadowBlur = 12;
        ctx.stroke();
        ctx.restore();
      }
      // hover 高亮环
      if (this.hovered && this.hovered.file === b.file) {
        ctx.beginPath();
        ctx.arc(cx, cy, r + 3, 0, Math.PI * 2);
        ctx.lineWidth = 2.5;
        ctx.strokeStyle = accent;
        ctx.stroke();
      }
      // 文件名（够大才画，恒定字号）
      if (r >= 22) {
        const name = basename(b.file);
        const fs = Math.min(13, Math.max(9, r * 0.4));
        ctx.font = `${fs}px ${fontMono}`;
        ctx.fillStyle = "#141310";
        const maxChars = Math.max(3, Math.floor((r * 1.7) / (fs * 0.6)));
        ctx.fillText(truncate(name, maxChars), cx, cy);
      }
    }
    ctx.globalAlpha = 1; // F70：复位（高亮态压暗过 alpha，别泄漏到下一帧/其它绘制）
  }

  // === canvas 交互（pan / zoom / hover / click）===

  private bindCanvasEvents(): void {
    const c = this.canvas;

    c.addEventListener("wheel", (e) => {
      e.preventDefault();
      const rect = c.getBoundingClientRect();
      const sx = e.clientX - rect.left;
      const sy = e.clientY - rect.top;
      const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
      this.viewport = zoomAt(this.viewport, sx, sy, factor);
      this.updateHover(sx, sy);
      this.scheduleDraw();
    }, { passive: false });

    c.addEventListener("mousedown", (e) => {
      if (e.button !== 0) return;
      this.panning = true;
      this.movedDuringPress = false;
      this.panStart = { x: e.clientX, y: e.clientY };
      this.panOrigin = { x: this.viewport.x, y: this.viewport.y };
      c.classList.add("is-panning");
    });

    window.addEventListener("mousemove", (e) => {
      if (!this.isOpen) return;
      if (this.panning) {
        const dx = e.clientX - this.panStart.x;
        const dy = e.clientY - this.panStart.y;
        if (Math.abs(dx) > 3 || Math.abs(dy) > 3) this.movedDuringPress = true;
        this.viewport = {
          ...this.viewport,
          x: this.panOrigin.x + dx,
          y: this.panOrigin.y + dy,
        };
        this.hideTooltip();
        this.scheduleDraw();
        return;
      }
      // hover
      const rect = c.getBoundingClientRect();
      const inside =
        e.clientX >= rect.left &&
        e.clientX <= rect.right &&
        e.clientY >= rect.top &&
        e.clientY <= rect.bottom;
      if (!inside) {
        this.updateHover(-1, -1);
        return;
      }
      this.updateHover(e.clientX - rect.left, e.clientY - rect.top, e.clientX, e.clientY);
    });

    window.addEventListener("mouseup", (e) => {
      if (!this.panning) return;
      this.panning = false;
      c.classList.remove("is-panning");
      // 未拖动 = 点击 → 命中气泡则开文件详情。
      if (!this.movedDuringPress && e.button === 0) {
        const rect = c.getBoundingClientRect();
        const inside =
          e.clientX >= rect.left &&
          e.clientX <= rect.right &&
          e.clientY >= rect.top &&
          e.clientY <= rect.bottom;
        if (inside && this.layout) {
          const hit = hitTest(
            e.clientX - rect.left,
            e.clientY - rect.top,
            this.layout.bubbles,
            this.viewport,
          );
          if (hit) this.openFileDetail(hit);
        }
      }
    });
  }

  private updateHover(sx: number, sy: number, clientX?: number, clientY?: number): void {
    if (!this.layout || sx < 0) {
      if (this.hovered) {
        this.hovered = null;
        this.scheduleDraw();
      }
      this.hideTooltip();
      return;
    }
    const hit = hitTest(sx, sy, this.layout.bubbles, this.viewport);
    if (hit !== this.hovered) {
      this.hovered = hit;
      this.scheduleDraw();
    }
    if (hit) {
      this.canvas.style.cursor = "pointer";
      this.showTooltip(hit, clientX ?? 0, clientY ?? 0);
    } else {
      this.canvas.style.cursor = "grab";
      this.hideTooltip();
    }
  }

  private showTooltip(b: FileBubble, clientX: number, clientY: number): void {
    this.tooltipEl.replaceChildren();
    const path = document.createElement("div");
    path.className = "panorama-tt-path";
    path.textContent = b.file;
    this.tooltipEl.appendChild(path);
    const meta = document.createElement("div");
    meta.className = "panorama-tt-meta";
    meta.textContent =
      `score ${fmtScore(b.score)} · ${b.symbols} 符号 · ${b.subsystem}` +
      (b.isEntry ? " · 入口点" : "");
    this.tooltipEl.appendChild(meta);
    const wrapRect = this.canvasWrap.getBoundingClientRect();
    let x = clientX - wrapRect.left + 14;
    let y = clientY - wrapRect.top + 14;
    // 防溢出右/下边
    const ttW = 260;
    if (x + ttW > wrapRect.width) x = wrapRect.width - ttW - 8;
    if (y + 60 > wrapRect.height) y = clientY - wrapRect.top - 60;
    this.tooltipEl.style.left = `${Math.max(4, x)}px`;
    this.tooltipEl.style.top = `${Math.max(4, y)}px`;
    this.tooltipEl.style.display = "";
  }
  private hideTooltip(): void {
    this.tooltipEl.style.display = "none";
  }

  // === 搜索 → 符号列表 ===

  private async runSearch(query: string): Promise<void> {
    if (!this.repo) return;
    const repo = this.repo;
    const seq = ++this.searchSeq;
    this.openSidebar();
    this.renderSidebarStatus("搜索中…");
    try {
      const syms = await api.search(repo, query, 40);
      if (seq !== this.searchSeq || this.repo !== repo) return;
      this.renderSearchResults(query, syms);
    } catch (e) {
      if (seq !== this.searchSeq) return;
      this.renderSidebarStatus(`搜索失败：${String(e)}`);
      showActionFailureToast("符号搜索失败", String(e));
    }
  }

  private renderSearchResults(query: string, syms: Symbol[]): void {
    this.sidebarEl.replaceChildren();
    this.sidebarEl.appendChild(
      this.sidebarHeader(`搜索「${query}」`, `${syms.length} 个符号`),
    );
    if (syms.length === 0) {
      this.sidebarEl.appendChild(makeSideNote("无匹配符号。裸名不解析，试试函数/方法/类的名字子串。"));
      return;
    }
    this.sidebarEl.appendChild(this.buildSymbolList(syms));
  }

  /** F71：符号列表（name/kind/loc 行，点击进节点详情）——搜索结果 + 点文件列符号共用。 */
  private buildSymbolList(syms: Symbol[]): HTMLElement {
    const list = document.createElement("div");
    list.className = "panorama-sym-list";
    for (const s of syms) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "panorama-sym-row";
      const name = document.createElement("span");
      name.className = "panorama-sym-name";
      name.textContent = s.name;
      row.appendChild(name);
      const kind = document.createElement("span");
      kind.className = "panorama-sym-kind";
      kind.textContent = s.kind;
      row.appendChild(kind);
      const loc = document.createElement("span");
      loc.className = "panorama-sym-loc";
      loc.textContent = `${s.file}:${s.start_line}`;
      loc.title = s.id;
      row.appendChild(loc);
      row.addEventListener("click", () => void this.openNodeDetail(s.id));
      list.appendChild(row);
    }
    return list;
  }

  // === 节点详情（符号 + callers/callees + docs）===

  private async openNodeDetail(id: string): Promise<void> {
    if (!this.repo) return;
    const repo = this.repo;
    const seq = ++this.searchSeq;
    this.openSidebar();
    this.renderSidebarStatus("加载符号详情…");
    try {
      const nv = await api.node(repo, id);
      if (seq !== this.searchSeq || this.repo !== repo) return;
      if (!nv) {
        this.renderSidebarStatus(`未找到符号：${id}`);
        return;
      }
      this.renderNodeDetail(nv);
    } catch (e) {
      if (seq !== this.searchSeq) return;
      this.renderSidebarStatus(`加载失败：${String(e)}`);
      showActionFailureToast("符号详情加载失败", String(e));
    }
  }

  private renderNodeDetail(nv: NodeView): void {
    const s = nv.symbol;
    this.sidebarEl.replaceChildren();
    this.sidebarEl.appendChild(this.sidebarHeader(s.name, s.kind));

    const detail = document.createElement("div");
    detail.className = "panorama-node-detail";

    // 元信息
    const meta = document.createElement("div");
    meta.className = "panorama-node-meta";
    // F68：签名放最前（最有用）；后端拿不到（body 字段非标准/非可调用符号）则不显示这行。
    if (s.signature) appendMetaRow(meta, "签名", s.signature, true);
    appendMetaRow(meta, "全限定 id", s.id, true);
    appendMetaRow(meta, "位置", `${s.file}:${s.start_line}-${s.end_line}`, true);
    appendMetaRow(meta, "语言", s.lang, false);
    appendMetaRow(meta, "类型", s.kind, false);
    detail.appendChild(meta);

    // callees（它调用了谁）
    detail.appendChild(
      this.edgeSection("调用了（callees）", nv.callees, "to"),
    );
    // callers（谁调用了它）
    detail.appendChild(
      this.edgeSection("被调用（callers）", nv.callers, "from"),
    );

    // 关联文档
    if (nv.docs.length > 0) {
      const sec = document.createElement("div");
      sec.className = "panorama-node-section";
      const h = document.createElement("div");
      h.className = "panorama-node-section-title";
      h.textContent = `关联文档（${nv.docs.length}）`;
      sec.appendChild(h);
      for (const d of nv.docs) {
        const row = document.createElement("div");
        row.className = "panorama-doc-row";
        row.textContent = d.doc_path;
        row.title = `来源：${d.source}${d.target_symbol ? ` · ${d.target_symbol}` : ""}`;
        sec.appendChild(row);
      }
      detail.appendChild(sec);
    }

    this.sidebarEl.appendChild(detail);
  }

  /** callers/callees 一节：每条边显示对端符号 id + confidence + 调用行，可点击钻取。 */
  private edgeSection(title: string, edges: Edge[], endKey: "to" | "from"): HTMLElement {
    const sec = document.createElement("div");
    sec.className = "panorama-node-section";
    const h = document.createElement("div");
    h.className = "panorama-node-section-title";
    h.textContent = `${title}（${edges.length}）`;
    sec.appendChild(h);
    if (edges.length === 0) {
      const empty = document.createElement("div");
      empty.className = "panorama-edge-empty";
      empty.textContent = "（无 / 未解析）";
      sec.appendChild(empty);
      return sec;
    }
    for (const e of edges) {
      const otherId = e[endKey];
      const row = document.createElement("button");
      row.type = "button";
      row.className = "panorama-edge-row";
      const idEl = document.createElement("span");
      idEl.className = "panorama-edge-id";
      idEl.textContent = otherId;
      row.appendChild(idEl);
      const badges = document.createElement("span");
      badges.className = "panorama-edge-badges";
      const conf = document.createElement("span");
      conf.className = `panorama-conf conf-${e.confidence.toLowerCase()}`;
      conf.textContent = confidenceLabel(e.confidence);
      conf.title = confidenceHint(e.confidence);
      badges.appendChild(conf);
      if (e.call_site_line != null) {
        const line = document.createElement("span");
        line.className = "panorama-edge-line";
        line.textContent = `L${e.call_site_line}`;
        badges.appendChild(line);
      }
      row.appendChild(badges);
      row.addEventListener("click", () => void this.openNodeDetail(otherId));
      sec.appendChild(row);
    }
    return sec;
  }

  // === 文件详情（点气泡）===

  private openFileDetail(b: FileBubble): void {
    if (!this.repo) return; // 远端/无仓无从查符号（纵深防御，正常靠 message 遮罩挡住点击）
    this.openSidebar();
    this.sidebarEl.replaceChildren();
    this.sidebarEl.appendChild(this.sidebarHeader(basename(b.file), b.subsystem));

    const detail = document.createElement("div");
    detail.className = "panorama-node-detail";
    const meta = document.createElement("div");
    meta.className = "panorama-node-meta";
    appendMetaRow(meta, "文件", b.file, true);
    appendMetaRow(meta, "score", fmtScore(b.score), false);
    appendMetaRow(meta, "符号数", String(b.symbols), false);
    appendMetaRow(meta, "子系统", b.subsystem, false);
    if (b.isEntry) appendMetaRow(meta, "入口点", "是（entry point）", false);
    detail.appendChild(meta);
    this.sidebarEl.appendChild(detail);

    // F71（补「点文件不能列符号」遗留）：列该文件的符号 → 点符号进详情看 callers/callees。
    // 异步拉，占位「加载中」；竞态用 searchSeq 代际（与搜索/节点详情同一侧栏世代）。
    const symWrap = document.createElement("div");
    symWrap.className = "panorama-file-symbols";
    symWrap.appendChild(makeSideNote("加载符号…"));
    this.sidebarEl.appendChild(symWrap);
    void this.loadFileSymbols(b.file, symWrap);
  }

  /** F71：拉某文件的符号列表填进 symWrap。竞态用 searchSeq 代际防串（切文件/搜索作废本次）。 */
  private async loadFileSymbols(file: string, symWrap: HTMLElement): Promise<void> {
    if (!this.repo) return;
    const repo = this.repo;
    const seq = ++this.searchSeq;
    try {
      const syms = await api.symbolsInFile(repo, file);
      if (seq !== this.searchSeq || this.repo !== repo) return;
      symWrap.replaceChildren();
      if (syms.length === 0) {
        symWrap.appendChild(makeSideNote("该文件无已索引符号（符号太少 / 解析失败 / 非代码文件）。"));
        return;
      }
      const hd = document.createElement("div");
      hd.className = "panorama-sym-listhead";
      hd.textContent = `${syms.length} 个符号 · 点击看 callers/callees`;
      symWrap.appendChild(hd);
      symWrap.appendChild(this.buildSymbolList(syms));
    } catch (e) {
      if (seq !== this.searchSeq) return;
      symWrap.replaceChildren();
      symWrap.appendChild(makeSideNote(`加载符号失败：${String(e)}`));
    }
  }

  /** F71：文档漂移面板——列仓里 .md 指向已失效的悬空链接（doc → target + reason）。 */
  private async showDrift(): Promise<void> {
    if (!this.repo) {
      showActionFailureToast("无法查漂移", "当前不是本地仓库视图（远端或无仓）。");
      return;
    }
    const repo = this.repo;
    const seq = ++this.searchSeq;
    this.openSidebar();
    this.renderSidebarStatus("检查文档漂移…");
    try {
      const items = await api.drift(repo);
      if (seq !== this.searchSeq || this.repo !== repo) return;
      this.sidebarEl.replaceChildren();
      this.sidebarEl.appendChild(this.sidebarHeader("文档漂移", `${items.length} 处悬空链接`));
      this.sidebarEl.appendChild(
        makeSideNote("反映上次索引快照——改了代码请先「刷新」再查。"),
      );
      if (items.length === 0) {
        this.sidebarEl.appendChild(makeSideNote("没有悬空文档链接 ✓"));
        return;
      }
      const list = document.createElement("div");
      list.className = "panorama-drift-list";
      for (const it of items) {
        const row = document.createElement("div");
        row.className = "panorama-drift-row";
        const doc = document.createElement("div");
        doc.className = "panorama-drift-doc";
        doc.textContent = it.doc_path;
        row.appendChild(doc);
        const tgt = document.createElement("div");
        tgt.className = "panorama-drift-target";
        tgt.textContent = it.target_symbol
          ? `→ ${it.target_file}#${it.target_symbol}`
          : `→ ${it.target_file}`;
        row.appendChild(tgt);
        const reason = document.createElement("div");
        reason.className = "panorama-drift-reason";
        reason.textContent = it.reason;
        row.appendChild(reason);
        list.appendChild(row);
      }
      this.sidebarEl.appendChild(list);
    } catch (e) {
      if (seq !== this.searchSeq) return;
      this.renderSidebarStatus(`查漂移失败：${String(e)}`);
      showActionFailureToast("文档漂移查询失败", String(e));
    }
  }

  // === 侧栏基础 ===

  private openSidebar(): void {
    this.sidebarEl.classList.add("is-open");
    this.root.classList.add("sidebar-open");
    // 侧栏改变了画布可用宽度 → 下一帧重算尺寸再画。
    requestAnimationFrame(() => {
      this.resizeCanvas();
      this.scheduleDraw();
    });
  }
  private closeSidebar(): void {
    this.sidebarEl.classList.remove("is-open");
    this.root.classList.remove("sidebar-open");
    this.sidebarEl.replaceChildren();
    requestAnimationFrame(() => {
      this.resizeCanvas();
      this.scheduleDraw();
    });
  }

  private sidebarHeader(title: string, subtitle: string): HTMLElement {
    const head = document.createElement("div");
    head.className = "panorama-sidebar-head";
    const texts = document.createElement("div");
    texts.className = "panorama-sidebar-titles";
    const t = document.createElement("div");
    t.className = "panorama-sidebar-title";
    t.textContent = title;
    t.title = title;
    texts.appendChild(t);
    const st = document.createElement("div");
    st.className = "panorama-sidebar-subtitle";
    st.textContent = subtitle;
    texts.appendChild(st);
    head.appendChild(texts);
    const x = document.createElement("button");
    x.type = "button";
    x.className = "panorama-sidebar-close";
    x.textContent = "✕";
    x.title = "关闭侧栏 (Esc)";
    x.addEventListener("click", () => this.closeSidebar());
    head.appendChild(x);
    return head;
  }

  private renderSidebarStatus(text: string): void {
    this.sidebarEl.replaceChildren();
    this.sidebarEl.appendChild(this.sidebarHeader("符号", ""));
    this.sidebarEl.appendChild(makeSideNote(text));
  }
}

// === 无状态小工具 ===

function basename(path: string): string {
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  if (max <= 1) return s.slice(0, Math.max(0, max));
  return s.slice(0, max - 1) + "…";
}

function fmtScore(n: number): string {
  return Number.isInteger(n) ? String(n) : n.toFixed(1);
}

function confidenceLabel(c: Confidence): string {
  switch (c) {
    case "Exact":
      return "精确";
    case "Heuristic":
      return "启发";
    case "DynamicGuess":
      return "动态猜测";
  }
}
function confidenceHint(c: Confidence): string {
  switch (c) {
    case "Exact":
      return "Exact：全局唯一名匹配（不代表验证过 import/作用域）";
    case "Heuristic":
      return "Heuristic：多候选，启发式选定";
    case "DynamicGuess":
      return "DynamicGuess：方法调用等接收者类型未知，尽力猜测";
  }
}

function appendMetaRow(parent: HTMLElement, label: string, value: string, mono: boolean): void {
  const row = document.createElement("div");
  row.className = "panorama-meta-row";
  const l = document.createElement("span");
  l.className = "panorama-meta-label";
  l.textContent = label;
  row.appendChild(l);
  const v = document.createElement("span");
  v.className = mono ? "panorama-meta-value is-mono" : "panorama-meta-value";
  v.textContent = value;
  v.title = value;
  row.appendChild(v);
  parent.appendChild(row);
}

function makeSideNote(text: string): HTMLElement {
  const note = document.createElement("div");
  note.className = "panorama-side-note";
  note.textContent = text;
  return note;
}

/** 圆角矩形路径（不依赖 ctx.roundRect，老 WebView2 也稳）。 */
function roundRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  const rad = Math.max(0, Math.min(r, w / 2, h / 2));
  ctx.beginPath();
  ctx.moveTo(x + rad, y);
  ctx.arcTo(x + w, y, x + w, y + h, rad);
  ctx.arcTo(x + w, y + h, x, y + h, rad);
  ctx.arcTo(x, y + h, x, y, rad);
  ctx.arcTo(x, y, x + w, y, rad);
  ctx.closePath();
}
