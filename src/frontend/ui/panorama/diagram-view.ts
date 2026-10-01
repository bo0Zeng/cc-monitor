/**
 * PN1b：全景页里「选图」那一整块 —— 选择器 ＋ 旋钮 ＋ 图 ＋ 图例 ＋ 诚实信号。
 *
 * ## 与 `views/panorama.ts` 的分工（解耦成一个宿主接口）
 * 本类只管「图」：取注册表、按旋钮拼请求、按形状选渲染器、摆图例与诚实信号、复制。
 * 仓是哪个、选中了哪个符号 / 文件、侧栏怎么开，都问宿主（[`DiagramHost`]）。
 * 宿主换了（比如以后单开一个窗口）本类不用动；本类换了宿主也不用动。
 *
 * ## 本仓零处写死图种名（CP2）
 * 选项从图种注册表现读（`api.diagramKinds` → 那台机器小程序的 `diagram_kinds` op）；「要不要先点一个符号」「显示哪些旋钮」都看注册表的
 * `params`；画成什么样看 `Diagram.body.shape`。上游加一种已有形状的新图 ⇒ 这里零改动。
 * 新形状 ⇒ 如实说「这一版还画不出」＋ 复制 Mermaid。
 *
 * ## 缩放 / 拖拽
 * 图铺满画布，画出来那一刻按视口适配（小图封顶 [`FIT_MAX_SCALE`] 倍）；滚轮以指针为锚缩放、按住拖动平移、
 * 顶栏「适配」回到适配；没动过时画布尺寸变了（拉窗口 · 开关侧栏）跟着重新适配。视口数学与气泡全景同一份
 * （`layout.ts` 的 `fitViewport` / `zoomAt`）。只动世界那一层的变换，节点与边一条不增不减（CP1）。
 *
 * 买到：人能在全景页上选图、拧旋钮、下钻、复制给 agent、缩放拖动看大图。
 * **买不到**：调用子图每次重画都向后端要一张新图，不做增量；真机 WebView2 上的拖动手感没量。
 */
import * as api from "./api";
import type { RepoAt } from "./api";
import { LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { clipForDiagram, type IndexStamp } from "./agent-clip";
import { honestyLine } from "./diagram-honesty";
import { legendFor, rendererFor, worldOf, type NodePick } from "./diagram-render";
import { fitViewport, zoomAt, type Viewport } from "./layout";
import type { DiagramKindInfo, DiagramRequest, PanoramaDiagram } from "./types";
import { copyText } from "../copy-table";

/** 宿主（`PanoramaView`）要提供的东西。 */
export interface DiagramHost {
  /** 当前看的仓（哪台机器上的哪个路径）；`null` = 没有可看的仓（无会话）。 */
  repo(): RepoAt | null;
  selectedSymbol(): string | null;
  selectedFile(): string | null;
  /** 调用子图下钻：把这个符号设为选中（宿主顺带开它的详情）。 */
  selectSymbol(id: string): void;
  /** 侧栏列一组可点的条目（团/模块的成员文件、类型的方法）。 */
  showList(title: string, subtitle: string, items: { label: string; title?: string; onClick: () => void }[]): void;
  /** 进文件详情 / 符号详情（复用宿主现成的侧栏）。 */
  openFile(file: string, group: string): void;
  openSymbol(id: string): void;
  /** 索引读数（CP7）。 */
  stamp(repo: RepoAt): Promise<IndexStamp | null>;
  /** 复制按钮（宿主统一实现剪贴板与反馈）。 */
  copyButton(label: string, key: string, make: () => string): HTMLButtonElement;
  /** 失败提示。 */
  toast(title: string, body: string): void;
}

/** 「气泡全景」：本仓自己的默认视图，不是上游的图种。 */
export const BUBBLE_VIEW = "";

/** 适配时四边留白（屏幕像素）。 */
const FIT_PAD = 24;
/** 适配最多放大几倍：一两个节点的小图不该被撑到满屏。 */
export const FIT_MAX_SCALE = 2;
/** 滚轮一格缩放的倍数（与气泡全景同）。 */
const WHEEL_FACTOR = 1.12;
/** 按下之后移动超过几像素算拖动（拖过之后松开那一下不当点击，同气泡全景）。 */
const DRAG_SLOP = 3;

/** 旋钮（只显示当前图种 `params` 里声明的那几个）。 */
interface Knobs {
  certain_only: boolean;
  exclude_tests: boolean;
  max_nodes: number;
}

export class DiagramPane {
  /** 顶栏里的选择器。 */
  readonly select: HTMLSelectElement;
  /** 旋钮与复制那一行（气泡视图时隐藏）。 */
  readonly bar: HTMLElement;
  /** 盖在气泡画布上的图层（气泡视图时隐藏）。 */
  readonly overlay: HTMLElement;

  private kinds: DiagramKindInfo[] | null = null;
  /** 上面那份注册表是哪台机器的。 */
  private kindsOrigin: Origin = LOCAL_ORIGIN;
  private current = BUBBLE_VIEW;
  private knobs: Knobs = { certain_only: true, exclude_tests: true, max_nodes: 12 };
  private seq = 0;
  private last: { info: DiagramKindInfo; view: PanoramaDiagram; stamp: IndexStamp | null; repo: RepoAt } | null =
    null;

  private canvasEl: HTMLElement;
  private legendEl: HTMLElement;
  private honestyEl: HTMLElement;
  private noteEl: HTMLElement;
  private knobEls = new Map<string, HTMLElement>();

  // 视口（世界那一层的平移 / 缩放）
  private vp: Viewport = { x: 0, y: 0, scale: 1 };
  private world: { el: SVGGElement; w: number; h: number } | null = null;
  /** 人缩放 / 拖动过 ⇒ 画布尺寸变了不再自动适配（点「适配」或换一张图才回来）。 */
  private touched = false;
  private pan: { x: number; y: number; ox: number; oy: number; moved: boolean } | null = null;
  /** 刚拖完：吞掉松手那一下的点击（不然拖图时会顺手下钻一个节点）。 */
  private swallowClick = false;

  constructor(private host: DiagramHost) {
    this.select = document.createElement("select");
    this.select.className = "panorama-diagram-select";
    this.select.dataset.pano = "diagram-kind";
    this.select.title = copyText("diagramView.picker.hint");
    this.select.addEventListener("change", () => void this.setKind(this.select.value));

    this.bar = document.createElement("div");
    this.bar.className = "panorama-diagram-bar";
    this.bar.style.display = "none";
    this.buildKnobs();

    this.overlay = document.createElement("div");
    this.overlay.className = "panorama-diagram";
    this.overlay.style.display = "none";
    this.noteEl = document.createElement("div");
    this.noteEl.className = "panorama-diagram-note";
    this.canvasEl = document.createElement("div");
    this.canvasEl.className = "panorama-diagram-canvas";
    this.legendEl = document.createElement("div");
    this.legendEl.className = "panorama-diagram-legend";
    this.honestyEl = document.createElement("div");
    this.honestyEl.className = "panorama-diagram-honesty";
    this.honestyEl.dataset.pano = "diagram-honesty";
    this.overlay.append(this.noteEl, this.canvasEl, this.legendEl, this.honestyEl);
    this.bindViewport();
    this.renderOptions();
  }

  /** 当前选的图种 id（气泡视图 = `BUBBLE_VIEW`）。 */
  kind(): string {
    return this.current;
  }

  /** 把当前那张图适配进画布（顶栏「适配」；画出来那一刻也走这里）。画布还没尺寸（没显示）⇒ 不动。 */
  fit(): void {
    const w = this.canvasEl.clientWidth;
    const h = this.canvasEl.clientHeight;
    if (!this.world || w <= 0 || h <= 0) return;
    this.vp = fitViewport(this.world.w, this.world.h, w, h, FIT_PAD, FIT_MAX_SCALE);
    this.touched = false;
    this.applyViewport();
  }

  /**
   * 取一次注册表（打开全景时调；同一台机器取过就不再取 —— 它编在那台的全景程序里，不会变）。
   * 按机器取：本机那份编在 monitor 里，远端那份问那台机器上的全景程序（两台版本可以不同）。
   */
  async ensureKinds(): Promise<void> {
    const origin: Origin = this.host.repo()?.origin ?? LOCAL_ORIGIN;
    if (this.kinds && this.kindsOrigin === origin) return;
    try {
      const got = await api.diagramKinds(origin);
      this.kinds = Array.isArray(got) ? got : null;
      this.kindsOrigin = origin;
    } catch (e) {
      this.host.toast(copyText("diagramView.kinds.failed"), String(e));
      this.kinds = null;
    }
    this.renderOptions();
  }

  /** 宿主的选中对象变了：刷新「先点一个符号」的灰态；团/模块图上的聚焦跟着重画。 */
  selectionChanged(): void {
    this.renderOptions();
    const info = this.info();
    if (info && this.last && this.last.info.id === info.id && this.last.view.diagram.body.shape === "clusters") {
      this.paint(this.last.view, info, true); // 同一张图只是换了描环 ⇒ 视口不动
    }
  }

  /** 仓变了：回到气泡视图（上一张图属于上一个仓）；换了机器就按新机器重取注册表。 */
  repoChanged(): void {
    this.last = null;
    void this.setKind(BUBBLE_VIEW);
    const origin: Origin = this.host.repo()?.origin ?? LOCAL_ORIGIN;
    if (this.kinds && this.kindsOrigin !== origin) void this.ensureKinds();
  }

  /** 切图。`BUBBLE_VIEW` = 回到气泡全景。 */
  async setKind(id: string): Promise<void> {
    this.current = id;
    this.select.value = id;
    const on = id !== BUBBLE_VIEW;
    this.bar.style.display = on ? "" : "none";
    this.overlay.style.display = on ? "" : "none";
    if (on) await this.redraw();
  }

  /** 按当前图种、旋钮、选中对象重画。 */
  async redraw(): Promise<void> {
    const info = this.info();
    const repo = this.host.repo();
    if (!info || !repo) return;
    this.syncKnobs(info);
    const needsSymbol = info.params.includes("symbol");
    const symbol = this.host.selectedSymbol();
    if (needsSymbol && !symbol) {
      this.showNote(copyText("diagramView.redraw.needSymbol", { title: info.title }));
      return;
    }
    const req: DiagramRequest = {};
    if (needsSymbol) req.symbol = symbol;
    if (info.params.includes("max_nodes")) req.max_nodes = this.knobs.max_nodes;
    if (info.params.includes("certain_only")) req.certain_only = this.knobs.certain_only;
    if (info.params.includes("exclude_tests")) req.exclude_tests = this.knobs.exclude_tests;
    const mine = ++this.seq;
    this.showNote(copyText("diagramView.redraw.drawing"));
    try {
      const [view, stamp] = await Promise.all([api.diagram(repo, info.id, req), this.host.stamp(repo)]);
      if (mine !== this.seq || !api.sameRepo(this.host.repo(), repo) || this.current !== info.id) return;
      this.last = { info, view, stamp, repo };
      this.paint(view, info);
    } catch (e) {
      if (mine !== this.seq) return;
      this.last = null;
      this.clearDrawing();
      this.showNote(copyText("diagramView.redraw.failed", { e: String(e) }));
    }
  }

  // === 内部 ===

  private info(): DiagramKindInfo | null {
    return this.kinds?.find((k) => k.id === this.current) ?? null;
  }

  private renderOptions(): void {
    const keep = this.current;
    this.select.replaceChildren();
    const bubble = document.createElement("option");
    bubble.value = BUBBLE_VIEW;
    bubble.textContent = copyText("diagramView.picker.bubble");
    this.select.appendChild(bubble);
    const hasSymbol = !!this.host.selectedSymbol();
    for (const k of this.kinds ?? []) {
      const o = document.createElement("option");
      o.value = k.id;
      const blocked = k.params.includes("symbol") && !hasSymbol;
      o.textContent = blocked ? copyText("diagramView.picker.blocked", { title: k.title }) : k.title;
      o.title = k.summary;
      o.disabled = blocked && keep !== k.id;
      this.select.appendChild(o);
    }
    this.select.value = keep;
  }

  private buildKnobs(): void {
    const check = (key: "certain_only" | "exclude_tests", label: string, title: string): void => {
      const wrap = document.createElement("label");
      wrap.title = title;
      const box = document.createElement("input");
      box.type = "checkbox";
      box.checked = this.knobs[key];
      box.dataset.pano = `knob-${key}`;
      box.addEventListener("change", () => {
        this.knobs[key] = box.checked;
        void this.redraw();
      });
      wrap.append(box, document.createTextNode(label));
      this.knobEls.set(key, wrap);
      this.bar.appendChild(wrap);
    };
    check("certain_only", copyText("diagramView.knobs.certainOnly"), copyText("diagramView.knobs.certainOnlyHint"));
    check("exclude_tests", copyText("diagramView.knobs.excludeTests"), copyText("diagramView.knobs.excludeTestsHint"));
    const nodes = document.createElement("label");
    nodes.title = copyText("diagramView.knobs.maxNodesHint");
    const num = document.createElement("input");
    num.type = "number";
    num.min = "1";
    num.max = "200";
    num.value = String(this.knobs.max_nodes);
    num.dataset.pano = "knob-max_nodes";
    num.addEventListener("change", () => {
      const n = Math.max(1, Math.min(200, Math.floor(Number(num.value) || 1)));
      num.value = String(n);
      this.knobs.max_nodes = n;
      void this.redraw();
    });
    nodes.append(document.createTextNode(copyText("diagramView.knobs.maxNodes")), num);
    this.knobEls.set("max_nodes", nodes);
    this.bar.appendChild(nodes);
    this.bar.appendChild(
      this.host.copyButton(copyText("panorama.copyButton.agent"), "diagram-copy-agent", () => this.clipText()),
    );
    this.bar.appendChild(
      this.host.copyButton(copyText("diagramView.knobs.copyMermaid"), "diagram-copy-mermaid", () => this.last?.view.mermaid ?? ""),
    );
  }

  /** 只显示当前图种认的旋钮。 */
  private syncKnobs(info: DiagramKindInfo): void {
    for (const [key, el] of this.knobEls) el.style.display = info.params.includes(key) ? "" : "none";
  }

  private clipText(): string {
    const l = this.last;
    if (!l) return "";
    const body = l.view.diagram.body;
    const center = "center" in body && typeof body.center === "string" ? body.center : null;
    return clipForDiagram(
      { repo: api.repoLabel(l.repo), stamp: l.stamp },
      { id: l.info.id, title: l.info.title },
      honestyLine(l.view.diagram.honesty),
      l.view.mermaid,
      center,
    );
  }

  private showNote(text: string): void {
    this.noteEl.textContent = text;
    this.noteEl.style.display = "";
  }

  private clearDrawing(): void {
    this.canvasEl.replaceChildren();
    this.world = null;
    this.legendEl.replaceChildren();
    this.honestyEl.textContent = "";
  }

  private paint(view: PanoramaDiagram, info: DiagramKindInfo, keepView = false): void {
    const { diagram } = view;
    const shape = diagram.body.shape;
    this.clearDrawing();
    // 诚实信号那一行**任何形状都常驻**（画不出的形状也照样说漏了多少）
    this.honestyEl.textContent = honestyLine(diagram.honesty);
    const render = rendererFor(shape);
    if (!render) {
      this.showNote(
        copyText("diagramView.paint.unsupported", { shape }),
      );
      return;
    }
    this.noteEl.style.display = "none";
    const svg = render(diagram.body, {
      onNode: (pick) => this.onNode(pick, info),
      focusFile: this.host.selectedFile(),
    });
    this.canvasEl.appendChild(svg);
    this.world = worldOf(svg);
    if (keepView) this.applyViewport();
    else this.fit();
    for (const item of legendFor(shape)) {
      const span = document.createElement("span");
      span.dataset.conf = item.conf;
      span.textContent = item.text;
      this.legendEl.appendChild(span);
    }
  }

  private applyViewport(): void {
    const { x, y, scale } = this.vp;
    this.world?.el.setAttribute("transform", `translate(${x} ${y}) scale(${scale})`);
  }

  /** 滚轮缩放 · 按住拖动 · 拖完吞点击 · 没动过时画布变了跟着适配。 */
  private bindViewport(): void {
    const c = this.canvasEl;
    c.addEventListener(
      "wheel",
      (e) => {
        if (!this.world) return;
        e.preventDefault();
        const rect = c.getBoundingClientRect();
        this.vp = zoomAt(this.vp, e.clientX - rect.left, e.clientY - rect.top, e.deltaY < 0 ? WHEEL_FACTOR : 1 / WHEEL_FACTOR);
        this.touched = true;
        this.applyViewport();
      },
      { passive: false },
    );
    c.addEventListener("mousedown", (e) => {
      this.swallowClick = false; // 上一次拖完若在画布外松手，那一下点击根本没来
      if (e.button !== 0 || !this.world) return;
      this.pan = { x: e.clientX, y: e.clientY, ox: this.vp.x, oy: this.vp.y, moved: false };
      c.classList.add("is-panning");
    });
    window.addEventListener("mousemove", (e) => {
      const p = this.pan;
      if (!p) return;
      const dx = e.clientX - p.x;
      const dy = e.clientY - p.y;
      if (!p.moved && Math.abs(dx) <= DRAG_SLOP && Math.abs(dy) <= DRAG_SLOP) return;
      p.moved = true;
      this.vp = { ...this.vp, x: p.ox + dx, y: p.oy + dy };
      this.touched = true;
      this.applyViewport();
    });
    window.addEventListener("mouseup", () => {
      const p = this.pan;
      if (!p) return;
      this.pan = null;
      c.classList.remove("is-panning");
      this.swallowClick = p.moved;
    });
    // 捕获阶段：先于节点自己的点击处理
    c.addEventListener(
      "click",
      (e) => {
        if (!this.swallowClick) return;
        this.swallowClick = false;
        e.stopPropagation();
        e.preventDefault();
      },
      true,
    );
    if (typeof ResizeObserver !== "undefined") {
      new ResizeObserver(() => {
        if (!this.touched) this.fit();
      }).observe(c);
    }
  }

  /** 下钻：团/模块 → 成员文件；调用图 → 以它为中心重画；类型 → 方法。 */
  private onNode(pick: NodePick, info: DiagramKindInfo): void {
    switch (pick.shape) {
      case "clusters": {
        const n = pick.node;
        this.host.showList(
          n.label,
          copyText("diagramView.node.cluster", { title: info.title, files: n.files, n: n.size }),
          n.member_files.map((f) => ({ label: f, onClick: () => this.host.openFile(f, n.label) })),
        );
        return;
      }
      case "call_graph":
        this.host.selectSymbol(pick.node.id);
        void this.redraw();
        return;
      case "type_graph": {
        const t = pick.node;
        const items = t.methods.map((m) => ({
          label: `${m.name}()`,
          title: m.symbol,
          onClick: () => this.host.openSymbol(m.symbol),
        }));
        if (t.symbol) {
          const sym = t.symbol;
          items.unshift({ label: copyText("diagramView.node.typeSelf", { name: t.name }), title: sym, onClick: () => this.host.openSymbol(sym) });
        }
        this.host.showList(t.name, copyText("diagramView.node.typeMembers", { fields: t.fields.length, methods: t.methods.length }), items);
        return;
      }
    }
  }
}
