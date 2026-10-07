/**
 * **会话内查找面板（主窗口）：一块非模态浮层，两页「搜索」「大纲」。**
 *
 * - 锚在会话头下、正文区右上；不挡读（浮层外的流照样能滚）。切走 tab ⇒ 收起并出弹层栈。
 * - 搜索：共用 [`FindStrip`] 的面板摆法 —— 停 300ms 自己找、回车立刻找；每条「谁 · 第几轮 · 时刻」＋ 一行片段；
 *   ↑↓ / 回车 / `F3` 选，选中即跳；没加载的那一段取到就跳。
 * - 大纲：你说的每一句一行（`UserInputPanel` 那份清单原样挂进来），点了跳；与轮次刻度同一份数据。
 * - `Ctrl+F` ⇒ 打开到「搜索」、焦点进框；同一个入口再按一次 ⇒ 收起。Esc ⇒ 收起，焦点回到打开前的地方，跳过去的高亮一并去掉。
 */
import { UserInputPanel, OUTLINE_LABEL, type JumpResult } from "./user-input-panel";
import { FindStrip } from "../find-strip";
import type { FindHit, FindResult } from "../session-reads";
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { button } from "../kit/button";
import { tabs } from "../kit/tabs";
import { hitTime } from "./history-time";
import { copyText } from "../copy-table";
import s from "./session-find.module.css";

/** 两个模式。 */
export type FindMode = "search" | "outline";

/** 宿主要提供的：怎么查 · 怎么跳 · 跳空了怎么说（后两样与大纲共用）。 */
export interface SessionFindHost {
  /** 在这一份会话里找（问后端；宿主决定问哪台机器上的哪份文件）；`skip` ＝ 跳过前几条（续下一页）。 */
  search(query: string, includeTools: boolean, skip: number): Promise<FindResult>;
  /** 同 `UserInputPanelHost.jumpTo`：返回真正落到的那张卡（可以是 Promise —— 要先按偏移取回正文）。 */
  jumpTo(uuid: string): JumpResult;
  /** 跳空时那一行下面写的一句。 */
  readonly unjumpableHint: string;
}

/** 命中行头一行：谁 · 第几轮 · 时刻（第一句之前的不写轮）。 */
export function hitMeta(h: FindHit, now: number): string {
  // 种类由那台给（`history-find` 的 `kind`）：你 · agent 回报（子 agent 交回 / 发来的话 · 另一个会话发来的话）· 工具 · 那一家。
  const who =
    h.kind === "user"
      ? copyText("sessionFind.who.user")
      : h.kind === "report"
        ? copyText("sessionFind.who.report")
        : h.kind === "tool"
          ? copyText("sessionFind.who.tool")
          : copyText("sessionFind.who.assistant");
  const time = h.tsMs > 0 ? hitTime(h.tsMs, now) : "";
  if (h.turn > 0 && time) return copyText("sessionFind.hit.meta", { who, n: h.turn, time });
  if (h.turn > 0) return copyText("sessionFind.hit.metaNoTime", { who, n: h.turn });
  return time ? copyText("sessionFind.hit.metaNoTurn", { who, time }) : who;
}

export class SessionFindPanel {
  /** 整块（宿主挂到流上、跟着 tab 翻 `.active`）。 */
  readonly el: HTMLElement;
  /** 大纲那一半（与历史查看器同一个类）。宿主把它交给 `OutlineSource`。 */
  readonly outline: UserInputPanel;
  private readonly box: HTMLElement;
  private readonly modeTabs: HTMLElement;
  private readonly searchPane: HTMLElement;
  private readonly find: FindStrip;
  private mode: FindMode = "search";
  /** 打开前焦点在哪（收起时还回去）。 */
  private returnTo: HTMLElement | null = null;
  /** 开着时压在快捷键弹层栈上的那一格（Esc ⇒ 收起）。 */
  private readonly overlay: OverlayHandle = { handleEsc: () => this.close() };

  constructor(host: SessionFindHost) {
    this.el = document.createElement("div");
    this.el.className = "session-find";

    this.outline = new UserInputPanel({
      jumpTo: (uuid) => host.jumpTo(uuid),
      unjumpableHint: host.unjumpableHint,
      openOutline: () => this.toggle("outline"),
    });
    this.find = new FindStrip(
      { search: (q, tools, skip) => host.search(q, tools, skip), jumpTo: (uuid) => host.jumpTo(uuid), unjumpableHint: host.unjumpableHint },
      { autoMs: 300, panel: true, meta: (h) => hitMeta(h, Date.now()) },
    );

    this.box = document.createElement("div");
    this.box.className = s.sfPanel;
    this.box.dataset.role = "session-find-panel";
    this.box.hidden = true;
    this.box.addEventListener("keydown", (e) => {
      if (e.key === "F3" && !e.isComposing && !e.defaultPrevented) {
        e.preventDefault();
        this.find.step(e.shiftKey ? -1 : 1);
      }
    });

    const bar = document.createElement("div");
    bar.className = s.sfBar;
    this.modeTabs = tabs<FindMode>({
      items: [
        { key: "search", label: copyText("sessionFind.box.searchLabel") },
        { key: "outline", label: OUTLINE_LABEL },
      ],
      current: "search",
      label: copyText("sessionFind.box.modes"),
      onChange: (m) => {
        this.mode = m;
        this.applyMode();
      },
    });
    const close = button({
      label: copyText("sessionFind.box.closeHint"),
      kind: "icon",
      icon: "close",
      size: "compact",
      hint: copyText("sessionFind.box.closeHint"),
      onClick: () => this.close(),
    });
    bar.append(this.modeTabs, close);

    this.searchPane = document.createElement("div");
    this.searchPane.className = s.sfSearch;
    const row = document.createElement("div");
    row.className = s.sfQuery;
    row.append(this.find.box, this.find.tools);
    this.searchPane.append(row, this.find.strip);

    this.box.append(bar, this.searchPane, this.outline.panel);
    this.el.append(this.box);
    this.applyMode();
  }

  get isOpen(): boolean {
    return !this.box.hidden;
  }

  get currentMode(): FindMode {
    return this.mode;
  }

  /** 打开并切到某个模式。「搜索」⇒ 焦点进框、全选（再按 Ctrl+F 就是改词）。 */
  open(mode: FindMode): void {
    if (!this.isOpen) {
      const was = document.activeElement;
      this.returnTo = was instanceof HTMLElement && !this.box.contains(was) ? was : null;
    }
    this.mode = mode;
    this.modeTabs.querySelector<HTMLElement>(`[data-key="${mode}"]`)?.click();
    this.box.hidden = false;
    dispatcher.pushOverlay(this.overlay);
    this.applyMode();
    if (mode === "search") this.find.focus();
  }

  close(): void {
    const wasOpen = this.isOpen;
    this.box.hidden = true;
    dispatcher.popOverlay(this.overlay);
    this.applyMode();
    if (!wasOpen) return;
    // 跳过去的那处高亮一并去掉；焦点还回去（还在页里的话）。
    for (const el of document.querySelectorAll(".search-hit-flash")) el.classList.remove("search-hit-flash");
    const back = this.returnTo;
    this.returnTo = null;
    if (back?.isConnected) back.focus({ preventScroll: true });
  }

  /** 同一个入口：没开 / 开着别的模式 ⇒ 打开到这个模式；已经开在这个模式 ⇒ 收起。 */
  toggle(mode: FindMode): void {
    if (this.isOpen && this.mode === mode) this.close();
    else this.open(mode);
  }

  /** 换会话 / 关 tab：结果清空、在途那趟作废、收起（查询串清掉 —— 上一份会话的词对这一份没意义）。 */
  reset(): void {
    this.find.reset();
    this.outline.clear();
    this.close();
  }

  /** 模式与开合 ⇒ 哪一半露出来、大纲入口的 `aria-expanded`。 */
  private applyMode(): void {
    const outline = this.mode === "outline";
    this.searchPane.hidden = outline;
    this.outline.panel.hidden = !outline;
    this.outline.toggle.setAttribute("aria-expanded", String(this.isOpen && outline));
  }
}
