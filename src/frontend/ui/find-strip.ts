/**
 * **会话内查找（框 ＋ 框下就地展开的命中清单）**：历史页查看器的工具行在用，主窗口的会话内查找（§5.12）要复用同一个。
 *
 * - 框：回车才搜（每次都是那台从头扫一遍文件，按回车本来就是这类框的常规手势）；`Ctrl+F` 在框里 ⇒ 全选。
 * - 命中清单就地展开在宿主摆它的位置（不是弹层：浮层一开焦点就离开输入框，没法接着改词）；头一行
 *   「{n} 处 · ☐ 含工具输出与思考 · ✕」，下面一处一行，点一处 ⇒ 交宿主跳。Esc / ✕ 收起（开着时登记在快捷键的弹层栈上）。
 * - 只排版：查什么、哪几处算命中由宿主问那台后端（`history-find`）；跳空了的标法与「你说过的话」清单行同一套（`markJump`）。
 * - 迟到的旧结果不盖新结果（代数）。
 */
import { button } from "./kit/button";
import { icon } from "./kit/icon";
import { checkbox } from "./kit/switch";
import { dispatcher, type OverlayHandle } from "./keybindings/registry";
import { imeComposing } from "./keybindings/ime";
import { copyText } from "./copy-table";
import type { FindHit, FindResult } from "./session-reads";
import { markJump, type JumpResult } from "./views/user-input-panel";
import s from "./find-strip.module.css";

export interface FindStripHost {
  /** 在这一份会话里找（宿主决定问哪台机器上的哪份文件）。 */
  search(query: string, includeTools: boolean): Promise<FindResult>;
  /** 跳到那一条（返回真正落到的那张卡；可以是 Promise）。 */
  jumpTo(uuid: string): JumpResult;
  /** 跳空时挂在那一行上的一句。 */
  readonly unjumpableHint: string;
}

export class FindStrip {
  /** 那个框（宿主摆进工具行）。 */
  readonly box: HTMLElement;
  /** 命中清单（宿主摆在工具行正下方）；收着时 `hidden`。 */
  readonly strip: HTMLElement;
  private readonly input: HTMLInputElement;
  private readonly head: HTMLElement;
  private readonly list: HTMLElement;
  private tools = false;
  private gen = 0;
  private readonly layer: OverlayHandle = {
    handleEsc: () => {
      this.close();
      return true;
    },
  };

  constructor(private readonly host: FindStripHost) {
    this.box = document.createElement("label");
    this.box.className = s.fsBox;
    this.box.appendChild(icon("search", "compact"));
    this.input = document.createElement("input");
    this.input.type = "search";
    this.input.className = s.fsInput;
    this.input.dataset.role = "find-input";
    this.input.placeholder = copyText("findStrip.box.placeholder");
    this.input.setAttribute("aria-label", copyText("findStrip.box.placeholder"));
    this.input.addEventListener("keydown", (e) => {
      if (imeComposing(e)) return; // 组字中的 Enter 归输入法，不拿半截拼音去查
      if (e.key === "Enter") {
        e.preventDefault();
        void this.run();
      } else if ((e.ctrlKey || e.metaKey) && e.code === "KeyF") {
        e.preventDefault();
        this.input.select();
      }
    });
    this.box.appendChild(this.input);

    this.strip = document.createElement("div");
    this.strip.className = s.fsStrip;
    this.strip.dataset.role = "find-strip";
    this.setShown(false);
    const bar = document.createElement("div");
    bar.className = s.fsBar;
    this.head = document.createElement("span");
    this.head.className = s.fsHead;
    this.head.dataset.role = "find-head";
    const tools = checkbox(copyText("findStrip.box.tools"), false, (on) => {
      this.tools = on;
      void this.run();
    });
    const close = button({
      label: copyText("findStrip.box.close"),
      kind: "icon",
      icon: "close",
      size: "compact",
      hint: copyText("findStrip.box.close"),
      onClick: () => this.close(),
    });
    bar.append(this.head, tools, close);
    this.list = document.createElement("div");
    this.list.className = s.fsList;
    this.strip.append(bar, this.list);
  }

  get isOpen(): boolean {
    return !this.strip.hidden;
  }

  /** `Ctrl+F`：焦点进框、全选（再按一次就是改词）。 */
  focus(): void {
    this.input.focus();
    this.input.select();
  }

  /** 按框里的词找一次（回车那一下）。空 ⇒ 收起、不发。 */
  async run(): Promise<void> {
    const q = this.input.value.trim();
    const gen = ++this.gen;
    this.list.replaceChildren();
    if (!q) {
      this.close();
      return;
    }
    this.open(copyText("findStrip.status.busy"));
    let res: FindResult;
    try {
      res = await this.host.search(q, this.tools);
    } catch (e) {
      res = { available: false, reason: String(e), hits: [], total: 0 };
    }
    if (gen !== this.gen) return; // 迟到的旧结果
    if (!res.available) {
      this.head.textContent = copyText("findStrip.status.unavailable", { reason: res.reason ?? "" });
      return;
    }
    for (const h of res.hits) this.list.appendChild(this.hitRow(h));
    const n = res.hits.length;
    this.head.textContent =
      n === 0
        ? copyText("findStrip.status.none", { q })
        : res.total > n
          ? copyText("findStrip.status.truncated", { total: res.total, n })
          : copyText("findStrip.status.count", { n });
  }

  /** 收起：清单清空、在途那趟作废（框里的词留着）。 */
  close(): void {
    if (this.isOpen) dispatcher.popOverlay(this.layer);
    this.setShown(false);
    this.gen++;
    this.list.replaceChildren();
  }

  /** 换会话：收起，词也清掉（上一份会话的词对这一份没意义）。 */
  reset(): void {
    this.close();
    this.input.value = "";
  }

  private open(head: string): void {
    if (!this.isOpen) dispatcher.pushOverlay(this.layer);
    this.setShown(true);
    this.head.textContent = head;
  }

  /** 命中清单露 / 收（`.fsStrip` 写了 display:flex ⇒ CSS 里另有一条 `[hidden]` 把它收住）。 */
  private setShown(on: boolean): void {
    this.strip.hidden = !on;
  }

  private hitRow(h: FindHit): HTMLButtonElement {
    const row = document.createElement("button");
    row.type = "button";
    row.className = s.fsHit;
    row.dataset.role = "find-hit";
    // 🔴 与「你说过的话」清单行同一条纪律：**不叫 `data-uuid`**（那个名字在本仓只指「一张渲染出来的消息卡」）。
    row.dataset.hitUuid = h.uuid;
    row.dataset.kind = h.kind;
    const mark = document.createElement("mark");
    mark.className = s.fsMark;
    mark.textContent = h.matched;
    row.append(document.createTextNode(h.before), mark, document.createTextNode(h.after));
    const plain = `${h.before}${h.matched}${h.after}`;
    row.title = plain;
    row.addEventListener("click", () => markJump(row, this.host.jumpTo(h.uuid), plain, this.host.unjumpableHint));
    return row;
  }
}
