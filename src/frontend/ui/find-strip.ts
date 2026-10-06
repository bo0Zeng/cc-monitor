/**
 * **会话内查找（框 ＋ 命中清单）**：历史页查看器的工具行与主窗口的会话内查找面板共用这一个。
 *
 * - 框：回车立刻找（组字中的回车归输入法）；给了 `autoMs` ⇒ 停这么久也自己找（主窗口）。`Ctrl+F` 在框里 ⇒ 全选。
 *   结果已经是框里这个词的 ⇒ 回车 / `F3` 下一条、`Shift+F3` 上一条，↑↓ 也走；选中一条就交宿主跳过去。
 * - 清单一页 `FIND_LIMIT` 条；没列完的滚到底自己续下一页（问那台后端 `skip` ＝ 已列的条数）。
 * - 跳到一条还没加载的：那一行下面写「未加载 · 加载后跳转」，取到就跳；取失败写原因 ＋［重试］。
 * - 两种摆法：就地展开（头一行「{n} 处 · ☐ 含工具 · ✕」，开着时登记在快捷键弹层栈上）· 面板（`panel`：
 *   头一行只有计数与键位提示，含工具那个勾由宿主摆 [`FindStrip.tools`]，开合与 Esc 归宿主）。
 * - 只排版：查什么、哪几处算命中、第几轮由那台后端（`history-find`）给。迟到的旧结果不盖新结果（代数）。
 */
import { button } from "./kit/button";
import { icon } from "./kit/icon";
import { spinner } from "./kit/progress";
import { checkbox } from "./kit/switch";
import { dispatcher, type OverlayHandle } from "./keybindings/registry";
import { imeComposing } from "./keybindings/ime";
import { copyText } from "./copy-table";
import type { FindHit, FindResult } from "./session-reads";
import type { JumpResult } from "./views/user-input-panel";
import s from "./find-strip.module.css";

/**
 * 命中片段去掉行内排版记号（与 `history-turns` 回复头同一套：`**` `__` 反引号，片段开头的 `#` / `>`）—— 片段是纯文本，
 * 记号留着就是「统一做**重试」那样一串。命中词那一段也去；去完空了（命中的就是记号本身）⇒ 那一段原样留着，高亮不丢。
 */
export function plainSnippet(h: { before: string; matched: string; after: string }): { before: string; matched: string; after: string } {
  const strip = (t: string): string => t.replace(/\*\*|__|`/g, "");
  const matched = strip(h.matched) || h.matched;
  return { before: strip(h.before).replace(/^(?:\s*[#>]+)+\s*/, ""), matched, after: strip(h.after) };
}

export interface FindStripHost {
  /** 在这一份会话里找（宿主决定问哪台机器上的哪份文件）；`skip` ＝ 跳过前几条（续下一页）。 */
  search(query: string, includeTools: boolean, skip: number): Promise<FindResult>;
  /** 跳到那一条（返回真正落到的那张卡；可以是 Promise —— 要先取回那一段；取失败 ⇒ reject 带原因）。 */
  jumpTo(uuid: string): JumpResult;
  /** 落空（找不到那张卡）时那一行下面写的一句。 */
  readonly unjumpableHint: string;
}

export interface FindStripOptions {
  /** 停多少毫秒自己找；不给 ⇒ 回车才找。 */
  autoMs?: number;
  /** 命中行的头一行（谁 · 第几轮 · 时刻）；不给 ⇒ 只一行片段。 */
  meta?: (h: FindHit) => string;
  /** 面板摆法（见头注）。 */
  panel?: boolean;
}

interface Row {
  hit: FindHit;
  el: HTMLElement;
  state: HTMLElement;
}

export class FindStrip {
  /** 那个框（宿主摆进工具行 / 面板顶）。 */
  readonly box: HTMLElement;
  /** 命中清单（宿主摆在框下）；就地展开摆法收着时 `hidden`。 */
  readonly strip: HTMLElement;
  /** 「含工具」那个勾（面板摆法由宿主摆；就地摆法已在清单头一行里）。 */
  readonly tools: HTMLElement;
  private readonly input: HTMLInputElement;
  private readonly head: HTMLElement;
  private readonly list: HTMLElement;
  private withTools = false;
  private gen = 0;
  private rows: Row[] = [];
  private total = 0;
  private selected = -1;
  /** 清单此刻是哪个词、勾没勾工具找出来的（回车时判「再找」还是「下一条」）。 */
  private shown: string | null = null;
  private more: Promise<void> | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private readonly layer: OverlayHandle = {
    handleEsc: () => {
      this.close();
      return true;
    },
  };

  constructor(
    private readonly host: FindStripHost,
    private readonly opts: FindStripOptions = {},
  ) {
    this.box = document.createElement("label");
    this.box.className = s.fsBox;
    if (opts.panel) this.box.dataset.layout = "panel";
    this.box.appendChild(icon("search", "compact"));
    this.input = document.createElement("input");
    this.input.type = "search";
    this.input.className = s.fsInput;
    this.input.dataset.role = "find-input";
    const placeholder = opts.panel ? copyText("sessionFind.box.placeholder") : copyText("findStrip.box.placeholder");
    this.input.placeholder = placeholder;
    this.input.setAttribute("aria-label", placeholder);
    this.input.addEventListener("keydown", (e) => this.onKey(e));
    this.input.addEventListener("input", () => this.armAuto());
    this.box.appendChild(this.input);

    this.tools = checkbox(opts.panel ? copyText("sessionFind.box.tools") : copyText("findStrip.box.tools"), false, (on) => {
      this.withTools = on;
      if (this.input.value.trim()) void this.run();
    });

    this.strip = document.createElement("div");
    this.strip.className = s.fsStrip;
    this.strip.dataset.role = "find-strip";
    if (opts.panel) this.strip.dataset.layout = "panel";
    const bar = document.createElement("div");
    bar.className = s.fsBar;
    this.head = document.createElement("span");
    this.head.className = s.fsHead;
    this.head.dataset.role = "find-head";
    if (opts.panel) {
      const keys = document.createElement("span");
      keys.className = s.fsKeys;
      keys.textContent = copyText("sessionFind.status.keys");
      bar.append(this.head, keys);
      this.setShown(true);
    } else {
      const close = button({
        label: copyText("findStrip.box.close"),
        kind: "icon",
        icon: "close",
        size: "compact",
        hint: copyText("findStrip.box.close"),
        onClick: () => this.close(),
      });
      bar.append(this.head, this.tools, close);
      this.setShown(false);
    }
    this.list = document.createElement("div");
    this.list.className = s.fsList;
    this.list.setAttribute("role", "listbox");
    this.list.addEventListener("scroll", () => {
      if (this.list.scrollTop + this.list.clientHeight >= this.list.scrollHeight - 24) void this.loadMore();
    });
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

  /** 按框里的词从头找一次。空 ⇒ 清单清空（就地摆法收起）、不发。 */
  async run(): Promise<void> {
    this.disarm();
    const q = this.input.value.trim();
    const gen = ++this.gen;
    this.rows = [];
    this.total = 0;
    this.selected = -1;
    this.more = null;
    this.list.replaceChildren();
    if (!q) {
      this.shown = null;
      if (this.opts.panel) this.head.textContent = "";
      else this.close();
      return;
    }
    this.open(copyText("findStrip.status.busy"));
    const key = this.queryKey(q);
    const res = await this.ask(q, 0);
    if (gen !== this.gen) return; // 迟到的旧结果
    this.shown = key;
    if (!res.available) {
      this.head.textContent = copyText("findStrip.status.unavailable", { reason: res.reason ?? "" });
      return;
    }
    this.total = res.total;
    this.append(res.hits);
    this.sayCount(q);
  }

  /** 选下一条 / 上一条并跳过去（到头绕回）。清单空 ⇒ 什么都不做。 */
  step(delta: number): void {
    const n = this.rows.length;
    if (n === 0) return;
    const i = this.selected < 0 ? (delta > 0 ? 0 : n - 1) : (this.selected + delta + n) % n;
    this.select(i);
    if (i === n - 1) void this.loadMore();
  }

  /** 收起：清单清空、在途那趟作废（框里的词留着）。 */
  close(): void {
    this.disarm();
    if (!this.opts.panel) {
      if (this.isOpen) dispatcher.popOverlay(this.layer);
      this.setShown(false);
    }
    this.gen++;
    this.rows = [];
    this.selected = -1;
    this.shown = null;
    this.list.replaceChildren();
    if (this.opts.panel) this.head.textContent = "";
  }

  /** 换会话：收起，词也清掉（上一份会话的词对这一份没意义）。 */
  reset(): void {
    this.close();
    this.input.value = "";
  }

  private queryKey(q: string): string {
    return `${this.withTools ? 1 : 0}\u0000${q}`;
  }

  private async ask(q: string, skip: number): Promise<FindResult> {
    try {
      return await this.host.search(q, this.withTools, skip);
    } catch (e) {
      return { available: false, reason: String(e), hits: [], total: 0 };
    }
  }

  /** 滚到底 / 走到最后一条：没列完就续下一页（同一时刻只一趟）。 */
  private loadMore(): Promise<void> {
    if (this.more) return this.more;
    if (this.shown === null || this.rows.length >= this.total) return Promise.resolve();
    const q = this.input.value.trim();
    if (this.queryKey(q) !== this.shown) return Promise.resolve();
    const gen = this.gen;
    this.more = this.ask(q, this.rows.length).then((res) => {
      if (gen !== this.gen) return;
      this.more = null;
      if (!res.available) return;
      this.append(res.hits);
      this.sayCount(q);
    });
    return this.more;
  }

  private sayCount(q: string): void {
    const n = this.rows.length;
    this.head.textContent =
      n === 0
        ? copyText("findStrip.status.none", { q })
        : this.total > n
          ? this.opts.panel
            ? copyText("sessionFind.status.truncated", { total: this.total, n })
            : copyText("findStrip.status.truncated", { total: this.total, n })
          : this.opts.panel
            ? copyText("sessionFind.status.count", { n })
            : copyText("findStrip.status.count", { n });
  }

  private armAuto(): void {
    if (this.opts.autoMs === undefined) return;
    this.disarm();
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.run();
    }, this.opts.autoMs);
  }

  private disarm(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  private onKey(e: KeyboardEvent): void {
    if (imeComposing(e)) return; // 组字中的回车 / 方向键归输入法
    const fresh = this.shown !== null && this.shown === this.queryKey(this.input.value.trim());
    if (e.key === "Enter") {
      e.preventDefault();
      if (fresh && this.rows.length > 0) this.step(e.shiftKey ? -1 : 1);
      else void this.run();
    } else if (e.key === "F3") {
      e.preventDefault();
      this.step(e.shiftKey ? -1 : 1);
    } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && this.rows.length > 0) {
      e.preventDefault();
      this.step(e.key === "ArrowDown" ? 1 : -1);
    } else if ((e.ctrlKey || e.metaKey) && e.code === "KeyF") {
      e.preventDefault();
      this.input.select();
    }
  }

  private open(head: string): void {
    if (!this.opts.panel && !this.isOpen) dispatcher.pushOverlay(this.layer);
    this.setShown(true);
    this.head.textContent = head;
  }

  /** 命中清单露 / 收（`.fsStrip` 写了 display:flex ⇒ CSS 里另有一条 `[hidden]` 把它收住）。 */
  private setShown(on: boolean): void {
    this.strip.hidden = !on;
  }

  private append(hits: FindHit[]): void {
    for (const h of hits) {
      const row = this.hitRow(h, this.rows.length);
      this.rows.push(row);
      this.list.appendChild(row.el);
    }
  }

  private select(i: number): void {
    const prev = this.rows[this.selected];
    if (prev) prev.el.setAttribute("aria-selected", "false");
    this.selected = i;
    const row = this.rows[i];
    if (!row) return;
    row.el.setAttribute("aria-selected", "true");
    row.el.scrollIntoView?.({ block: "nearest" });
    this.jump(row);
  }

  private jump(row: Row): void {
    const landed = this.host.jumpTo(row.hit.uuid);
    const settle = (el: HTMLElement | null): void => {
      if (el) {
        delete row.el.dataset.unjumpable;
        this.setState(row, null);
      } else {
        row.el.dataset.unjumpable = "1";
        this.setState(row, this.host.unjumpableHint);
      }
    };
    if (!(landed instanceof Promise)) {
      settle(landed);
      return;
    }
    this.setState(row, copyText("sessionFind.jump.pending"), spinner());
    landed.then(settle, (e: unknown) => {
      row.el.dataset.unjumpable = "1";
      const retry = button({
        label: copyText("sessionFind.jump.retry"),
        kind: "ghost",
        size: "compact",
        onClick: (ev) => {
          ev.stopPropagation();
          this.jump(row);
        },
      });
      this.setState(row, copyText("sessionFind.fetchFailed.line", { reason: e instanceof Error ? e.message : String(e) }), undefined, retry);
    });
  }

  private setState(row: Row, text: string | null, lead?: HTMLElement, tail?: HTMLElement): void {
    row.state.replaceChildren();
    row.state.hidden = text === null;
    if (text === null) return;
    if (lead) row.state.appendChild(lead);
    const t = document.createElement("span");
    t.textContent = text;
    row.state.appendChild(t);
    if (tail) row.state.appendChild(tail);
  }

  private hitRow(h: FindHit, index: number): Row {
    const el = document.createElement("div");
    el.className = s.fsHit;
    el.dataset.role = "find-hit";
    el.setAttribute("role", "option");
    el.setAttribute("aria-selected", "false");
    // 🔴 与「你说过的话」清单行同一条纪律：**不叫 `data-uuid`**（那个名字在本仓只指「一张渲染出来的消息卡」）。
    el.dataset.hitUuid = h.uuid;
    el.dataset.kind = h.kind;
    if (this.opts.meta) {
      const meta = document.createElement("div");
      meta.className = s.fsMeta;
      meta.textContent = this.opts.meta(h);
      el.appendChild(meta);
    }
    const t = plainSnippet(h);
    const snip = document.createElement("div");
    snip.className = s.fsSnip;
    const mark = document.createElement("mark");
    mark.className = s.fsMark;
    mark.textContent = t.matched;
    snip.append(document.createTextNode(t.before), mark, document.createTextNode(t.after));
    el.title = `${t.before}${t.matched}${t.after}`;
    const state = document.createElement("div");
    state.className = s.fsState;
    state.dataset.role = "find-state";
    state.hidden = true;
    el.append(snip, state);
    el.addEventListener("mousedown", (e) => e.preventDefault()); // 焦点留在框里（接着改词 / ↑↓）
    el.addEventListener("click", () => this.select(index));
    return { hit: h, el, state };
  }
}
