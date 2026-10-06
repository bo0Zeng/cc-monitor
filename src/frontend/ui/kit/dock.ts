/**
 * 底部抽屉：主区底部、状态栏之上的一格，几页共用（页签就是状态栏上对应那枚 chip 的字）。
 *
 * - 是 `#app` 网格里的一行（不是浮层）：开着把上面的消息流往上推，不盖内容；关着那一行收成 0。
 * - 高：拖上边缘调（`min` 到 `max()`），键盘在边上 ↑ ↓ 调，双击回到缺省高；调完交 `onResize` 记住。
 * - 页签：点 / ← → / Home / End 换页（C5）；右边 Esc 提示 ＋ 关闭。
 * - 看得见 ⇔ 在 Esc 弹层栈上（一下 Esc 收起；页里先接走的除外）；不拦别的键（它跟着当前 tab 走，不盖住 tab 栏）。
 * - 换页 / 开关只交 `onChange`；页里画什么、空态怎么写由各页自己管。
 */
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { icon, type IconName } from "./icon";
import { button } from "./button";
import { copyText } from "../copy-table";
import s from "./dock.module.css";

export interface DockPageSpec<K extends string> {
  key: K;
  icon: IconName;
  /** 页签上的字（之后可经 [`Dock.setLabel`] 换）。 */
  label: string;
  body: HTMLElement;
}

export interface DockSpec<K extends string> {
  pages: DockPageSpec<K>[];
  /** 读屏名（文案表）。 */
  label: string;
  /** 开着哪一页（`null` ＝ 收着）。 */
  current: K | null;
  height: number;
  /** 缺省高（双击上边缘回到它）。 */
  defaultHeight: number;
  min: number;
  /** 此刻最高能多高（随窗口变，调的那一刻现算）。 */
  max: () => number;
  onChange: (page: K | null) => void;
  onResize: (height: number) => void;
  /** Esc 落到抽屉上时先问一句：页里自己接走了（例如输入框里的 Esc 只交还焦点）⇒ 回 `true`，抽屉不收。 */
  escInside?: () => boolean;
}

/** 键盘调高一步多少 px。 */
const KEY_STEP = 16;

export class Dock<K extends string> {
  readonly el: HTMLElement;
  private readonly bodies = new Map<K, HTMLElement>();
  private readonly tabs = new Map<K, HTMLButtonElement>();
  private readonly labels = new Map<K, HTMLSpanElement>();
  /** 页区：只挂开着的那一页。 */
  private readonly body: HTMLElement;
  private cur: K | null = null;
  private height: number;
  private stacked = false;
  private readonly layer: OverlayHandle & { passes: "all" } = {
    handleEsc: () => {
      if (this.spec.escInside?.() !== true) this.close();
      return true;
    },
    passes: "all",
  };

  constructor(private readonly spec: DockSpec<K>) {
    this.height = spec.height;
    // 外层是网格里那一格（收着时 `hidden`，自己不写 display）；里层才是竖排的头 ＋ 页。
    this.el = document.createElement("section");
    this.el.className = s.dockSlot;
    this.el.setAttribute("aria-label", spec.label);
    const inner = document.createElement("div");
    inner.className = s.dock;
    const edge = document.createElement("div");
    edge.className = s.dockEdge;
    edge.setAttribute("role", "separator");
    edge.setAttribute("aria-orientation", "horizontal");
    edge.setAttribute("aria-label", copyText("kit.dock.resize"));
    edge.tabIndex = 0;
    this.wireEdge(edge);
    const head = document.createElement("div");
    head.className = s.dockHead;
    const strip = document.createElement("div");
    strip.className = s.dockTabs;
    strip.setAttribute("role", "tablist");
    strip.setAttribute("aria-label", spec.label);
    for (const p of spec.pages) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = s.dockTab;
      b.setAttribute("role", "tab");
      b.dataset.key = p.key;
      const t = document.createElement("span");
      t.textContent = p.label;
      b.append(icon(p.icon, "compact"), t);
      b.addEventListener("click", () => this.show(p.key));
      strip.appendChild(b);
      this.tabs.set(p.key, b);
      this.labels.set(p.key, t);
      const page = document.createElement("div");
      page.className = s.dockPage;
      page.setAttribute("role", "tabpanel");
      page.appendChild(p.body);
      this.bodies.set(p.key, page);
    }
    strip.addEventListener("keydown", (ev) => this.onStripKey(ev));
    const sp = document.createElement("span");
    sp.className = s.dockSp;
    const esc = document.createElement("kbd");
    esc.className = s.dockKey;
    esc.textContent = copyText("kit.dock.escKey");
    const x = button({ label: copyText("kit.dock.close"), kind: "icon", icon: "close", hint: copyText("kit.dock.close"), size: "compact", onClick: () => this.close() });
    head.append(strip, sp, esc, x);
    this.body = document.createElement("div");
    this.body.className = s.dockBody;
    inner.append(edge, head, this.body);
    this.el.appendChild(inner);
    this.go(spec.current, false);
  }

  /** 开着哪一页（`null` ＝ 收着）。 */
  get current(): K | null {
    return this.cur;
  }

  /** 页签上的字换成 `text`。 */
  setLabel(key: K, text: string): void {
    const t = this.labels.get(key);
    if (t && t.textContent !== text) t.textContent = text;
  }

  /** 开到这一页（开着别的页 ⇒ 换页，不关）。 */
  show(key: K): void {
    this.go(key, true);
  }

  /** 点状态栏那一枚 / 按它的键：这一页开着 ⇒ 收起；否则开到这一页。 */
  toggle(key: K): void {
    this.go(this.cur === key ? null : key, true);
  }

  close(): void {
    this.go(null, true);
  }

  private go(next: K | null, notify: boolean): void {
    const changed = next !== this.cur;
    this.cur = next;
    this.el.hidden = next === null;
    this.el.style.height = `${this.clamp(this.height)}px`;
    for (const [k, b] of this.tabs) {
      const on = k === next;
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
    }
    const page = next === null ? undefined : this.bodies.get(next);
    if (page && this.body.firstChild !== page) this.body.replaceChildren(page);
    const shown = next !== null;
    if (shown !== this.stacked) {
      this.stacked = shown;
      if (shown) dispatcher.pushOverlay(this.layer);
      else dispatcher.popOverlay(this.layer);
    }
    if (changed && notify) this.spec.onChange(next);
  }

  private clamp(h: number): number {
    return Math.round(Math.max(this.spec.min, Math.min(Math.max(this.spec.min, this.spec.max()), h)));
  }

  private resize(h: number): void {
    const next = this.clamp(h);
    if (next === this.height) return;
    this.height = next;
    this.el.style.height = `${next}px`;
    this.spec.onResize(next);
  }

  private wireEdge(edge: HTMLElement): void {
    edge.addEventListener("pointerdown", (ev) => {
      if (ev.button !== 0) return;
      ev.preventDefault();
      const startY = ev.clientY;
      const startH = this.el.getBoundingClientRect().height;
      edge.setPointerCapture?.(ev.pointerId);
      const move = (e: PointerEvent): void => this.resize(startH + (startY - e.clientY));
      const up = (): void => {
        edge.removeEventListener("pointermove", move);
        edge.removeEventListener("pointerup", up);
        edge.removeEventListener("pointercancel", up);
      };
      edge.addEventListener("pointermove", move);
      edge.addEventListener("pointerup", up);
      edge.addEventListener("pointercancel", up);
    });
    edge.addEventListener("dblclick", () => this.resize(this.spec.defaultHeight));
    edge.addEventListener("keydown", (ev) => {
      if (ev.key !== "ArrowUp" && ev.key !== "ArrowDown") return;
      ev.preventDefault();
      this.resize(this.height + (ev.key === "ArrowUp" ? KEY_STEP : -KEY_STEP));
    });
  }

  private onStripKey(ev: KeyboardEvent): void {
    if (ev.isComposing || this.cur === null) return;
    const keys = this.spec.pages.map((p) => p.key);
    const i = keys.indexOf(this.cur);
    const n = keys.length;
    const to = ev.key === "ArrowRight" ? (i + 1) % n : ev.key === "ArrowLeft" ? (i - 1 + n) % n : ev.key === "Home" ? 0 : ev.key === "End" ? n - 1 : -1;
    if (to < 0) return;
    ev.preventDefault();
    this.show(keys[to]);
    this.tabs.get(keys[to])?.focus();
  }
}
