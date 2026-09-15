/**
 * K-R45：「我说过的 N 句」那块**界面**，两条路共用一份。
 *
 * # 为什么是共用而不是各写一份
 *
 * `KR45D1`（甲 · 历史查看器）与 `KR45D2`（乙 · 实时窗口）要的界面**逐字是同一样**：
 * 一个开关 + 一块清单 + 每行点一下跳过去 + 跳空了标出来。
 * 件里写死了「不许把甲的实现照抄一份过来 —— 这个仓一整天在治的正是『一个东西两个住址』」。
 *
 * ⚠ **上一轮没有抽它是对的**：那时只有一个调用方，抽出来叫**投机抽象**
 * （本仓的规矩是「拆由具体架构病证成，不由将来可能有人用证成」）。
 * **本轮第二个调用方真的落地了**（`tabs.ts` 的实时窗口），这一份才成立。
 *
 * # 宿主之间真正不同的只有两件事，所以只有这两件走 `UserInputPanelHost`
 *
 * - **怎么跳**：查看器要先把没渲染的那一段渲出来（`uuidToIdx` + `UnrenderedRanges`），
 *   实时窗口没有这两样（`TailWindow` 只能从尾部取）—— 见件 `§5.2` 的 C 段。
 * - **跳空了怎么解释**：查看器是「渲染时被剥成了空卡」，实时窗口是「这一条还没加载出来」。
 *   两句话说的是两件事，写死一句就有一半是假话。
 *
 * # 🔴 标记是**加也有、减也有**的，两个字段都算
 *
 * `data-unjumpable`（变灰的钩子，样式住 `styles.css`）与 `title`（那句人话）
 * **跳成功时两样都要撤**。这条不是洁癖：
 * - 上一轮把变灰从 `row.style.opacity`（只加不减）搬到 `data-unjumpable` 上，
 *   PM 09-10 第二拍切刀实测 —— 把 `delete` 那一句整句删掉，**全量门禁 11 格无一红**
 *   ⇒ 「加」有判据、「删」零判据，形状与它声称修掉的那个一模一样。
 * - 而当时 `title` 那一半**连删的代码都没有**：跳成功了，「跳不过去」那句提示还挂着。
 * ⇒ 判据住 `user-input-panel.vitest.ts`「先跳空、后来跳得过去」那一格，
 *   活体读数（真会发生的那条转移）住 `live-user-inputs.vitest.ts`。
 */
import type { UserInputEntry } from "./user-input-index";

/** 宿主要提供的两件事 —— 「怎么跳」和「跳空了怎么跟人解释」。 */
export interface UserInputPanelHost {
  /**
   * 把这一条跳过去。**返回真正落到的那张卡**；`null` = 落空。
   *
   * 🔴 返回值就是落点读数：面板**不自己再查一遍 DOM 去猜跳没跳到** ——
   * 「先渲染再找」这一段只有宿主知道自己做没做，猜出来的读数会在两条路上各错一次。
   */
  jumpTo(uuid: string): HTMLElement | null;
  /** 跳空时挂在那一行上的一句人话。两条路的原因不同，所以由宿主给。 */
  readonly unjumpableHint: string;
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
    this.toggle.addEventListener("click", () => this.toggleOpen());
    this.panel = document.createElement("div");
    this.panel.className = "user-inputs";
    this.panel.hidden = true;
    this.clear();
  }

  /**
   * 换一份清单。
   *
   * 🔴 **按 uuid 就地对账，不整表重建**：实时那条路每来一句用户输入就调一次，
   * 整表重建会把已经标上的 `data-unjumpable` **连带抹掉** —— 那是「只加不减」的
   * 镜像形（一次**假的「减」**）：这一行明明还是跳不过去的，标记却自己没了。
   * 查看器那条路每次 `load()` 先 `clear()`，走的是「没有旧行」这一支，行为不变。
   */
  setEntries(entries: readonly UserInputEntry[]): void {
    for (let i = 0; i < entries.length; i++) {
      const old = this.panel.children[i] as HTMLButtonElement | undefined;
      if (old && old.dataset.inputUuid === entries[i].uuid) {
        // 同一条：只刷序号+摘要。**不碰 `title`** —— 它可能正挂着「跳不过去」那句。
        old.textContent = `${i + 1}. ${entries[i].excerpt}`;
        continue;
      }
      const row = this.buildRow(entries[i], i);
      if (old) this.panel.replaceChild(row, old);
      else this.panel.appendChild(row);
    }
    while (this.panel.children.length > entries.length) this.panel.lastElementChild!.remove();
    this.toggle.textContent = `我说过的 ${entries.length} 句`;
    // 一条都没有 ⇒ 禁用。不给一个点了没反应的入口。
    this.toggle.disabled = entries.length === 0;
  }

  /** 清空并收起（换会话 / 关 tab）—— 旧会话的句子不许挂在新会话上。 */
  clear(): void {
    this.panel.replaceChildren();
    this.panel.hidden = true;
    this.toggle.textContent = "我说过的 0 句";
    this.toggle.disabled = true;
    this.toggle.setAttribute("aria-expanded", "false");
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
    if (this.host.jumpTo(entry.uuid)) {
      // 🔴 **两样都要撤**。撤一样就是把「只加不减」从一个字段搬到另一个字段。
      delete row.dataset.unjumpable;
      row.title = entry.excerpt;
      return;
    }
    // 变灰**不在这里**：`styles.css` 的 `.user-input-row[data-unjumpable]`。
    // 呈现跟着标记走 ⇒ 上面那句 `delete` 一执行，灰也自动没了。
    row.dataset.unjumpable = "1";
    row.title = this.host.unjumpableHint;
  }
}
