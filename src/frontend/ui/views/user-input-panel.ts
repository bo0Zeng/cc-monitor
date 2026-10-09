/**
 * 「大纲」那块界面：历史查看器与实时窗口共用一份（开关 ＋ 清单 ＋ 每行点一下跳过去 ＋ 跳空了标出来）。
 * 宿主之间不同的只有两件，走 `UserInputPanelHost`：怎么跳（查看器要先把没渲染的那段渲出来，实时窗口只能从尾部取）；
 * 跳空了怎么解释（查看器是渲染时被剥成了空卡，实时窗口是这一条还没加载出来）。
 *
 * 标记加也有、减也有：`data-unjumpable`（变灰）与 `title`（那句人话）跳成功时两样都要撤。
 * 判据在 `user-input-panel.vitest.ts`「先跳空、后来跳得过去」那一格；活体读数在 `live-user-inputs.vitest.ts`。
 */
// 条目的形状由后端定（后端帧应答的成品，TS 形状住 `session-reads.ts`）；判定只住后端。
import type { UserInputEntry } from "../session-reads";
import { copyText } from "../copy-table";

/**
 * 这块界面的名字「大纲」：不用第一人称替用户说话、不把计数塞进控件标签；不叫「目录」（界面里「目录」已指工作目录）。
 * 两个宿主共用这一份。
 */
export const OUTLINE_LABEL = copyText("userInputPanel.outline.label");

/** 开关的 tooltip。一个住址：建开关与「要到清单」两处都用它。 */
const OUTLINE_HINT = copyText("userInputPanel.outline.hint");

/**
 * 「跳」的结局：真正落到的那张卡 / `null` = 落空。
 * 可以是 Promise：骨架接上之后，没物化的那段要先按偏移把正文取回来（异步）才建得出卡 ——
 * 同步那一下去找必然落空（`tab-stream-view.ts::jumpTo` 的头注）。
 */
export type JumpResult = HTMLElement | null | Promise<HTMLElement | null>;

/** 宿主要提供的两件事 —— 「怎么跳」和「跳空了怎么跟人解释」。 */
export interface UserInputPanelHost {
  /**
   * 把这一条跳过去。**返回真正落到的那张卡**；`null` = 落空。
   *
   * 🔴 返回值就是落点读数：面板**不自己再查一遍 DOM 去猜跳没跳到** ——
   * 「先渲染再找」这一段只有宿主知道自己做没做，猜出来的读数会在两条路上各错一次。
   */
  jumpTo(uuid: string): JumpResult;
  /** 跳空时挂在那一行上的一句人话。两条路的原因不同，所以由宿主给。 */
  readonly unjumpableHint: string;
  /**
   * 给了 ⇒ **开合归宿主**：开关按钮点下去调它，本类从此不碰 `panel.hidden` 与 `aria-expanded`
   * （实时 tab 把清单挂进查找面板的「大纲」模式，露不露由那块面板的模式决定）。
   * 不给 ⇒ 照旧自己开合（历史查看器）。
   */
  openOutline?(): void;
}

/**
 * 点一行之后**核一次落点**，把「跳不过去」的标记挂上或撤掉。大纲行与查找命中行**同一个住址**。
 *
 * 🔴 **两样都要撤**（`data-unjumpable` 与提示）—— 撤一样就是把「只加不减」从一个字段搬到另一个字段
 * （本文件头注那段 PM 实测）。异步的落点（Promise）落定之后再核。
 */
export function markJump(row: HTMLElement, landed: JumpResult, okTitle: string, hint: string): void {
  const apply = (el: HTMLElement | null): void => {
    if (el) {
      delete row.dataset.unjumpable;
      row.title = okTitle;
      return;
    }
    // 变灰**不在这里**：`styles.css` 的 `[data-unjumpable]`。呈现跟着标记走。
    row.dataset.unjumpable = "1";
    row.title = hint;
  };
  if (landed instanceof Promise) {
    landed.then(apply, () => apply(null));
  } else {
    apply(landed);
  }
}

export class UserInputPanel {
  /** 顶栏那个开关按钮。宿主自己决定把它挂到哪儿。 */
  readonly toggle: HTMLButtonElement;
  /** 清单面板。默认收着（`hidden`）⇒ 对宿主既有布局零影响。 */
  readonly panel: HTMLElement;
  private readonly host: UserInputPanelHost;

  constructor(host: UserInputPanelHost) {
    this.host = host;
    this.toggle = document.createElement("button");
    this.toggle.type = "button";
    this.toggle.className = "user-inputs-toggle";
    // 标签只有两个字 ⇒ 用 tooltip 说清它是干什么的（「最终形状」）。
    this.toggle.title = OUTLINE_HINT;
    this.toggle.addEventListener("click", () =>
      host.openOutline ? host.openOutline() : this.toggleOpen(),
    );
    this.panel = document.createElement("div");
    this.panel.className = "user-inputs";
    this.panel.hidden = true;
    this.clear();
  }

  /**
   * 换一份清单：清空再建表。
   *
   * 整表只在冷启动 / 文件被重写时换；新来的几条走 [`appendEntries`]。
   */
  setEntries(entries: readonly UserInputEntry[]): void {
    this.panel.replaceChildren();
    this.appendEntries(entries);
  }

  /**
   * 在末尾接上几条（增量：后端从上次的 `end` 接着给的那一截）。编号接着往下数。
   * 已有的行**一个字都不碰** —— 挂着的 `data-unjumpable` / 那句提示照旧挂着。
   */
  appendEntries(entries: readonly UserInputEntry[]): void {
    const base = this.panel.children.length;
    for (let i = 0; i < entries.length; i++) {
      this.panel.appendChild(this.buildRow(entries[i], base + i));
    }
    const n = this.panel.children.length;
    // 标签只说「点我干什么」，计数用间隔点挂在后面、0 条时不挂。
    this.toggle.textContent = n > 0 ? `${OUTLINE_LABEL} · ${n}` : OUTLINE_LABEL;
    this.toggle.title = OUTLINE_HINT;
    // 一条都没有 ⇒ 禁用。不给一个点了没反应的入口。
    this.toggle.disabled = n === 0;
  }

  /**
   * 这一趟**要不到**清单（老后端 / 本机后端不在 / 输出被截断）：清空、灰掉，
   * 原因挂在开关的提示上 —— 灰掉而不说为什么，与「一条都没有」长得一模一样。
   */
  setUnavailable(reason: string): void {
    this.clear();
    this.toggle.title = reason ? copyText("userInputPanel.setUnavailable.withReason", { reason }) : copyText("userInputPanel.setUnavailable.plain");
  }

  /** 清空并收起（换会话 / 关 tab）—— 旧会话的句子不许挂在新会话上。开合归宿主时只清不收。 */
  clear(): void {
    this.panel.replaceChildren();
    if (!this.host.openOutline) {
      this.panel.hidden = true;
      this.toggle.setAttribute("aria-expanded", "false");
    }
    this.toggle.title = OUTLINE_HINT;
    this.toggle.textContent = OUTLINE_LABEL;
    this.toggle.disabled = true;
  }

  /** 开关清单面板。`hidden` 而不是 `display` —— 与本仓其余处一致，也让判据好断。 */
  private toggleOpen(): void {
    const open = this.panel.hidden;
    this.panel.hidden = !open;
    this.toggle.setAttribute("aria-expanded", String(open));
  }

  private buildRow(entry: UserInputEntry, i: number): HTMLButtonElement {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "user-input-row";
    // 🔴 **刻意不叫 `data-uuid`**：那个名字在本仓有且只有一个意思 ——
    // 「这是一张渲染出来的消息卡」，`branch-fold.ts:236` 就是照它扫主线的。
    // 清单行不是卡。两件事共用一个属性名，下一个写 `[data-uuid]` 选择器的人就会数错。
    // （甲那一轮自抓：第一版真写成了 `data-uuid`，判据当场把卡和行混在一起数成 350。）
    row.dataset.inputUuid = entry.uuid;
    row.textContent = `${i + 1}. ${entry.excerpt}`;
    row.title = entry.excerpt;
    row.addEventListener("click", () => this.jump(entry, row));
    return row;
  }

  /**
   * 点一行 → 交给宿主去跳 → **回头核一次落点**。
   *
   * 落空不许静默（件 `§0c` / `KR45D3`）：用户看见的会是「点了一下，跳到了会话最后」
   * 或者「点了一下什么都没发生」，而没有任何东西说一句话。
   */
  private jump(entry: UserInputEntry, row: HTMLButtonElement): void {
    // 标记的加 / 撤搬进 [`markJump`]（查找命中行共用同一个住址）；两样都撤的纪律在那边。
    markJump(row, this.host.jumpTo(entry.uuid), entry.excerpt, this.host.unjumpableHint);
  }
}
