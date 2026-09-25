/**
 * PN1b（`设计/97 §7.3`）：全景页里「选图」那一整块 —— 选择器 ＋ 旋钮 ＋ 图 ＋ 图例 ＋ 诚实信号。
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
 * 买到：人能在全景页上选图、拧旋钮、下钻、复制给 agent。
 * **买不到**：图没有缩放 / 拖拽；调用子图每次重画都向后端要一张新图，不做增量。
 */
import * as api from "./api";
import type { RepoAt } from "./api";
import { LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { clipForDiagram, type IndexStamp } from "./agent-clip";
import { honestyLine } from "./diagram-honesty";
import { legendFor, rendererFor, type NodePick } from "./diagram-render";
import type { DiagramKindInfo, DiagramRequest, PanoramaDiagram } from "./types";

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
  /** 〔RM1c〕上面那份注册表是哪台机器的。 */
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

  constructor(private host: DiagramHost) {
    this.select = document.createElement("select");
    this.select.className = "panorama-diagram-select";
    this.select.dataset.pano = "diagram-kind";
    this.select.title = "选一种图；需要符号的图要先点一个符号";
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
    this.renderOptions();
  }

  /** 当前选的图种 id（气泡视图 = `BUBBLE_VIEW`）。 */
  kind(): string {
    return this.current;
  }

  /**
   * 取一次注册表（打开全景时调；同一台机器取过就不再取 —— 它编在那台的全景程序里，不会变）。
   * 〔RM1c〕按机器取：本机那份编在 monitor 里，远端那份问那台机器上的全景程序（两台版本可以不同）。
   */
  async ensureKinds(): Promise<void> {
    const origin: Origin = this.host.repo()?.origin ?? LOCAL_ORIGIN;
    if (this.kinds && this.kindsOrigin === origin) return;
    try {
      const got = await api.diagramKinds(origin);
      this.kinds = Array.isArray(got) ? got : null;
      this.kindsOrigin = origin;
    } catch (e) {
      this.host.toast("读不到图种清单", String(e));
      this.kinds = null;
    }
    this.renderOptions();
  }

  /** 宿主的选中对象变了：刷新「先点一个符号」的灰态；团/模块图上的聚焦跟着重画。 */
  selectionChanged(): void {
    this.renderOptions();
    const info = this.info();
    if (info && this.last && this.last.info.id === info.id && this.last.view.diagram.body.shape === "clusters") {
      this.paint(this.last.view, info);
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
      this.showNote(`「${info.title}」要以一个符号为中心：先在搜索或文件详情里点一个符号。`);
      return;
    }
    const req: DiagramRequest = {};
    if (needsSymbol) req.symbol = symbol;
    if (info.params.includes("max_nodes")) req.max_nodes = this.knobs.max_nodes;
    if (info.params.includes("certain_only")) req.certain_only = this.knobs.certain_only;
    if (info.params.includes("exclude_tests")) req.exclude_tests = this.knobs.exclude_tests;
    const mine = ++this.seq;
    this.showNote("画图中…");
    try {
      const [view, stamp] = await Promise.all([api.diagram(repo, info.id, req), this.host.stamp(repo)]);
      if (mine !== this.seq || !api.sameRepo(this.host.repo(), repo) || this.current !== info.id) return;
      this.last = { info, view, stamp, repo };
      this.paint(view, info);
    } catch (e) {
      if (mine !== this.seq) return;
      this.last = null;
      this.clearDrawing();
      this.showNote(`画不出这张图：${String(e)}`);
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
    bubble.textContent = "气泡全景";
    this.select.appendChild(bubble);
    const hasSymbol = !!this.host.selectedSymbol();
    for (const k of this.kinds ?? []) {
      const o = document.createElement("option");
      o.value = k.id;
      const blocked = k.params.includes("symbol") && !hasSymbol;
      o.textContent = blocked ? `${k.title}（先点一个符号）` : k.title;
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
    check("certain_only", "只看确定的连接", "全靠名字凑、一条确定的都没有的连接不画；滤掉多少写在图下面");
    check("exclude_tests", "排除测试", "架构讲的是产品的结构；排掉多少写在图下面");
    const nodes = document.createElement("label");
    nodes.title = "最多画几个节点；没画的写在图下面";
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
    nodes.append(document.createTextNode("节点上限 "), num);
    this.knobEls.set("max_nodes", nodes);
    this.bar.appendChild(nodes);
    this.bar.appendChild(
      this.host.copyButton("复制给 agent", "diagram-copy-agent", () => this.clipText()),
    );
    this.bar.appendChild(
      this.host.copyButton("复制 Mermaid", "diagram-copy-mermaid", () => this.last?.view.mermaid ?? ""),
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
    this.legendEl.replaceChildren();
    this.honestyEl.textContent = "";
  }

  private paint(view: PanoramaDiagram, info: DiagramKindInfo): void {
    const { diagram } = view;
    const shape = diagram.body.shape;
    this.clearDrawing();
    // 诚实信号那一行**任何形状都常驻**（画不出的形状也照样说漏了多少）
    this.honestyEl.textContent = honestyLine(diagram.honesty);
    const render = rendererFor(shape);
    if (!render) {
      this.showNote(
        `这一版还画不出这种形状的图（shape=${shape}）。可以点「复制 Mermaid」拿到上游画好的文本。`,
      );
      return;
    }
    this.noteEl.style.display = "none";
    const svg = render(diagram.body, {
      onNode: (pick) => this.onNode(pick, info),
      focusFile: this.host.selectedFile(),
    });
    this.canvasEl.appendChild(svg);
    for (const item of legendFor(shape)) {
      const span = document.createElement("span");
      span.dataset.conf = item.conf;
      span.textContent = item.text;
      this.legendEl.appendChild(span);
    }
  }

  /** 下钻：团/模块 → 成员文件；调用图 → 以它为中心重画；类型 → 方法。 */
  private onNode(pick: NodePick, info: DiagramKindInfo): void {
    switch (pick.shape) {
      case "clusters": {
        const n = pick.node;
        this.host.showList(
          n.label,
          `${info.title} · ${n.files} 个文件 · ${n.size} 个符号`,
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
          items.unshift({ label: `${t.name}（类型本身）`, title: sym, onClick: () => this.host.openSymbol(sym) });
        }
        this.host.showList(t.name, `${t.fields.length} 个字段 · ${t.methods.length} 个方法`, items);
        return;
      }
    }
  }
}
