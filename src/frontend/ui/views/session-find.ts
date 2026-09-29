/**
 * 〔SE2 · `设计/10 §2.2b ④` · `§6 步 6`〕**会话内查找面板：一块面板两个模式（搜索 / 大纲）。**
 *
 * # 它取代了什么
 *
 * 实时 tab 从前给大纲单独建了一块悬浮层（`.live-user-inputs`：开关 ＋ 清单），`§2.2b ④` 定的终态是
 * **并进 Ctrl+F 那块面板**：两者是同一件事的两个入口 ——
 * ```
 * Ctrl+F ：给我一段文字 → 告诉我它在这个会话的哪里 → 跳过去
 * 大纲   ：给我一份我的输入清单 → 选一条 → 跳过去
 * ```
 * 共用同一套「跳」（宿主给的 `jumpTo`：骨架先物化、按偏移取回的正文落完、再滚）。
 * ⇒ 跳只有一个住址；悬浮层只剩一块（z-index 参与者少一个）。
 *
 * # 形状
 *
 * ```
 * .session-find                      ← 悬浮在流上（原 .live-user-inputs 的位置，`.active` 跟着 tab 翻）
 *   [大纲 · N]                       ← 入口按钮（就是 UserInputPanel.toggle，文案 / 禁用规则一字不变）
 *   .session-find-panel（hidden）
 *     [搜索] [大纲]            [✕]
 *     搜索：输入框（Enter 搜）· 含工具内容 · 一行状态 · 命中清单
 *     大纲：UserInputPanel.panel（那份清单原样挂进来）
 * ```
 *
 * - 两个入口：Ctrl+F（动作 `session.find`）⇒ 打开并切到「搜索」、焦点进输入框；点「大纲」按钮 ⇒ 打开并切到「大纲」。
 *   同一个入口再按一次 ⇒ 收起。Esc ⇒ 收起（开着时登记在快捷键的弹层栈上，与历史 / 设置那几块同一套 LIFO；
 *   切走 tab ⇒ 收起并出栈，免得 Esc 去关一块看不见的面板）。
 * - **Enter 才搜**，不做边打边搜：每次都是后端从头扫一遍文件（`--find-in-session`），防抖要起定时器，
 *   而按 Enter 本来就是这类面板的常规手势。迟到的旧结果不覆盖新结果（代数）。
 * - 命中行跳空了的标法与大纲行**同一套**（[`markJump`]：`data-unjumpable` ＋ 提示，加也删也有）。
 * - 查不了（老后端 / 本机后端不在 / 截断）⇒ 状态行说清原因（原因由 monitor 给，本类不猜）。
 */
import { UserInputPanel, OUTLINE_LABEL, markJump, type JumpResult } from "./user-input-panel";
import type { FindHit, FindResult } from "../session-reads";
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { copyText } from "../copy-table";

/** 两个模式。 */
export type FindMode = "search" | "outline";

/** 宿主要提供的：怎么查 · 怎么跳 · 跳空了怎么解释（后两样与大纲共用）。 */
export interface SessionFindHost {
  /** 在这一份会话里找（问后端；宿主决定问哪台机器上的哪份文件）。 */
  search(query: string, includeTools: boolean): Promise<FindResult>;
  /** 同 `UserInputPanelHost.jumpTo`：返回真正落到的那张卡（可以是 Promise —— 要先按偏移取回正文）。 */
  jumpTo(uuid: string): JumpResult;
  /** 跳空时挂在那一行上的一句人话。 */
  readonly unjumpableHint: string;
}

const SEARCH_LABEL = copyText("sessionFind.box.searchLabel");
const PLACEHOLDER = copyText("sessionFind.box.placeholder");
const TOOLS_LABEL = copyText("sessionFind.box.tools");
const TOOLS_HINT = copyText("sessionFind.box.toolsHint");
const CLOSE_HINT = copyText("sessionFind.box.closeHint");
const IDLE_STATUS = copyText("sessionFind.status.idle");
const BUSY_STATUS = copyText("sessionFind.status.busy");
const NONE_STATUS = copyText("sessionFind.status.none");

export class SessionFindPanel {
  /** 整块（宿主挂到流上、跟着 tab 翻 `.active`）。 */
  readonly el: HTMLElement;
  /** 大纲那一半（与历史查看器同一个类）。宿主把它交给 `OutlineSource`。 */
  readonly outline: UserInputPanel;
  private readonly box: HTMLElement;
  private readonly searchTab: HTMLButtonElement;
  private readonly outlineTab: HTMLButtonElement;
  private readonly searchPane: HTMLElement;
  private readonly input: HTMLInputElement;
  private readonly tools: HTMLInputElement;
  private readonly status: HTMLElement;
  private readonly hits: HTMLElement;
  private mode: FindMode = "search";
  /** 每发一次查找加一；回来的代数不是最新的 ⇒ 作废（迟到的旧结果不许盖掉新结果）。 */
  private gen = 0;
  /** 开着时压在快捷键弹层栈上的那一格（Esc ⇒ 收起）。 */
  private readonly overlay: OverlayHandle = { handleEsc: () => this.close() };

  constructor(private readonly host: SessionFindHost) {
    this.el = document.createElement("div");
    this.el.className = "session-find";

    this.outline = new UserInputPanel({
      jumpTo: (uuid) => host.jumpTo(uuid),
      unjumpableHint: host.unjumpableHint,
      // 开合交给本面板：大纲按钮 ⇒ 打开并切到「大纲」（再按一次收起）
      openOutline: () => this.toggle("outline"),
    });

    this.box = document.createElement("div");
    this.box.className = "session-find-panel";
    this.box.hidden = true;

    const bar = document.createElement("div");
    bar.className = "session-find-modes";
    bar.setAttribute("role", "tablist");
    this.searchTab = this.modeTab(SEARCH_LABEL, "search");
    this.outlineTab = this.modeTab(OUTLINE_LABEL, "outline");
    const close = document.createElement("button");
    close.type = "button";
    close.className = "session-find-close";
    close.textContent = copyText("sessionFind.box.close");
    close.title = CLOSE_HINT;
    close.setAttribute("aria-label", CLOSE_HINT);
    close.addEventListener("click", () => this.close());
    bar.append(this.searchTab, this.outlineTab, close);

    this.searchPane = document.createElement("div");
    this.searchPane.className = "session-find-search";
    const row = document.createElement("div");
    row.className = "session-find-query";
    this.input = document.createElement("input");
    this.input.type = "search";
    this.input.className = "session-find-input";
    this.input.placeholder = PLACEHOLDER;
    this.input.setAttribute("aria-label", PLACEHOLDER);
    this.input.addEventListener("keydown", (e) => this.onKey(e));
    const toolsLabel = document.createElement("label");
    toolsLabel.className = "session-find-tools";
    toolsLabel.title = TOOLS_HINT;
    this.tools = document.createElement("input");
    this.tools.type = "checkbox";
    const toolsText = document.createElement("span");
    toolsText.textContent = TOOLS_LABEL;
    toolsLabel.append(this.tools, toolsText);
    row.append(this.input, toolsLabel);
    this.status = document.createElement("div");
    this.status.className = "session-find-status";
    this.status.textContent = IDLE_STATUS;
    this.hits = document.createElement("div");
    this.hits.className = "session-find-hits";
    this.searchPane.append(row, this.status, this.hits);

    this.box.append(bar, this.searchPane, this.outline.panel);
    this.el.append(this.outline.toggle, this.box);
    this.applyMode();
  }

  get isOpen(): boolean {
    return !this.box.hidden;
  }

  get currentMode(): FindMode {
    return this.mode;
  }

  /** 打开并切到某个模式。「搜索」⇒ 焦点进输入框、全选（再按 Ctrl+F 就是改查询）。 */
  open(mode: FindMode): void {
    this.mode = mode;
    this.box.hidden = false;
    dispatcher.pushOverlay(this.overlay);
    this.applyMode();
    if (mode === "search") {
      this.input.focus();
      this.input.select();
    }
  }

  close(): void {
    this.box.hidden = true;
    dispatcher.popOverlay(this.overlay);
    this.applyMode();
  }

  /** 同一个入口：没开 / 开着别的模式 ⇒ 打开到这个模式；已经开在这个模式 ⇒ 收起。 */
  toggle(mode: FindMode): void {
    if (this.isOpen && this.mode === mode) this.close();
    else this.open(mode);
  }

  /** 换会话 / 关 tab：结果清空、在途那趟作废、收起（查询串留着 —— 同一个 tab 里下次还想找同一个词）。 */
  reset(): void {
    this.gen++;
    this.hits.replaceChildren();
    this.status.textContent = IDLE_STATUS;
    this.outline.clear();
    this.close();
  }

  /** 按当前输入框的内容找一次（Enter 的那一下）。空查询 ⇒ 清空、不发。 */
  async runSearch(): Promise<void> {
    const q = this.input.value.trim();
    const gen = ++this.gen;
    this.hits.replaceChildren();
    if (!q) {
      this.status.textContent = IDLE_STATUS;
      return;
    }
    this.status.textContent = BUSY_STATUS;
    let res: FindResult;
    try {
      res = await this.host.search(q, this.tools.checked);
    } catch (e) {
      res = { available: false, reason: String(e), hits: [], total: 0 };
    }
    if (gen !== this.gen) return; // 迟到的旧结果
    if (!res.available) {
      this.status.textContent = copyText("sessionFind.runSearch.unavailable", { reason: res.reason ?? "" });
      return;
    }
    for (const h of res.hits) this.hits.appendChild(this.buildHit(h));
    const n = res.hits.length;
    this.status.textContent =
      n === 0 ? NONE_STATUS : res.total > n ? copyText("sessionFind.runSearch.truncated", { total: res.total, n }) : copyText("sessionFind.runSearch.count", { n });
  }

  /**
   * 输入框里的键：Enter ⇒ 找。Esc 不在这里（走快捷键的弹层栈，见 `open`）。
   * Ctrl/⌘+F 在输入框里 ⇒ 全选（快捷键在可编辑元素里不派发，不接住它就落到 webview 自带的查找条上）。
   */
  private onKey(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      void this.runSearch();
    } else if ((e.ctrlKey || e.metaKey) && e.code === "KeyF") {
      e.preventDefault();
      this.input.select();
    }
  }

  private modeTab(label: string, mode: FindMode): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "session-find-mode";
    b.dataset.mode = mode;
    b.setAttribute("role", "tab");
    b.textContent = label;
    b.addEventListener("click", () => this.open(mode));
    return b;
  }

  /** 模式与开合 ⇒ 哪一半露出来、哪个标签按下、入口按钮的 `aria-expanded`。 */
  private applyMode(): void {
    const outline = this.mode === "outline";
    this.searchPane.hidden = outline;
    this.outline.panel.hidden = !outline;
    this.searchTab.setAttribute("aria-selected", String(!outline));
    this.outlineTab.setAttribute("aria-selected", String(outline));
    this.outline.toggle.setAttribute("aria-expanded", String(this.isOpen && outline));
  }

  private buildHit(h: FindHit): HTMLButtonElement {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "session-find-hit";
    // 🔴 与大纲行同一条纪律：**不叫 `data-uuid`**（那个名字在本仓只指「一张渲染出来的消息卡」）。
    row.dataset.hitUuid = h.uuid;
    row.dataset.kind = h.kind;
    const mark = document.createElement("mark");
    mark.textContent = h.matched;
    // 全部 textContent（防 XSS，同历史浏览器的片段）
    row.append(document.createTextNode(h.before), mark, document.createTextNode(h.after));
    const plain = `${h.before}${h.matched}${h.after}`;
    row.title = plain;
    row.addEventListener("click", () => {
      markJump(row, this.host.jumpTo(h.uuid), plain, this.host.unjumpableHint);
    });
    return row;
  }
}
