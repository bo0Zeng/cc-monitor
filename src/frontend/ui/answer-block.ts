/**
 * **答题块**（消息流末尾、贴着聊天线底部）：当前会话在等批准 / 回答 / 批计划时，说清等什么、在这里就地答。
 * 不是模态框：不挡读聊天、出现时不抢焦点；从「需手动」的入口跳过来时焦点才进块（[`AnswerBlock.focusFor`]）。
 *
 * 只排版 —— 字、语气、要不要附话、题目全照核心那一份 `needs.ask`（后端 `agents/asks.rs`），一个字不在这里拼：
 * - 头一行 `needs.head`（「等批准 · Bash」）＋ 子 agent 那一趟的种类 ＋ 已等多久；要批的全文（命令 · 计划原文）进等宽块，长了块内滚；
 * - 选项按核心排好的序分三行：允许那一族（第一颗是主按钮，批准一步的标数字键）· 拒绝那一族（只有「拒绝并停下」红字）·
 *   改到终端里答（幽灵、排末尾）。哪一项进哪一行按选项 id（协议里的闭集，`control::asks::verdict_of`），不按字；
 * - 要附话的（`note` 不是 none）点了原地换一行输入框：Enter 交、Esc 回；必附的空着不交；
 * - 提问：每题照原样排（单选圆点 · 多选方块 · 末尾一格自由文本），每题都答了「提交答案」才可点，按题 id 交 picks；
 * - 没挂上请求（`ask: null`：信任框 · 别处起的会话 · 放行联网 … 每一种 needs.kind 都走这一支）⇒ 核心写好的那一句（只能在终端里答 / 答题口没接上）＋
 *   ［看终端］，能去它的终端时再多一颗（Windows ↗［切到终端］· 远端 tmux［在终端里打开］）。
 *
 * 答出去之后等那台推 `changed {needs}` 重读（下一条挂着的换上来；没有了块收起）。别处先答了 ⇒ 块淡出收起、留一行（稳定态，
 * 下一条请求来了 / 切走才没）：点了才知道的（already_answered · closed_elsewhere）那一行照核心那一句；没点、重读里那条请求没了的写「已在别处答过」。
 * 没人答、那台替人答了（会话事实 `autoAnswered`）⇒ 同一行写核心那一句，切走再回来也在，那条会话下一次挂上提问才换掉。
 */
import type { Tab } from "./tab-model";
import type { Ask, AskChoice, AskQuestionItem, Needs } from "./session-reads";
import { copyText } from "./copy-table";
import { button, setBusy, setDisabled } from "./kit/button";
import { kbd } from "./kit/badge";
import { needsOf } from "./session-face";
import { waitedNow } from "./duration-format";
import type { AnswerReq } from "./generated/AnswerReq";
import s from "./answer-block.module.css";

/** 交给 `needs-answer` 的那一份（`sid` 由宿主按 tab 补上）。 */
export type AnswerRequest = Omit<AnswerReq, "sid">;

/** 答的结局：交到了 · 别处先答了 / 撤了（`said` 是核心那一句）· 没答成（别的原因，块照旧可答）。 */
export type AnswerOutcome = { kind: "answered" } | { kind: "gone"; said: string } | { kind: "failed"; said: string; detail: string };

export interface AnswerHost {
  active(): Tab | null;
  answer(tab: Tab, req: AnswerRequest): Promise<AnswerOutcome>;
  /** 再问一次这个会话的事实（needs 跟着换）。 */
  reread(sid: string): void;
  toast(title: string, detail?: string): void;
  /** 块高了 / 收了：消息流底部给它让出多高（像素；收起 ⇒ 0）。 */
  room?(px: number): void;
}

/** 一道题此刻答成什么样。 */
interface Pick {
  options: Set<string>;
  text: string;
}

export class AnswerBlock {
  readonly el: HTMLDivElement;
  /** 画的是哪一份（会话 · 请求 · 那一句）；同一份重画只换已等多久，选到一半的不丢。 */
  private drawn: string | null = null;
  private since: HTMLElement | null = null;
  private body: HTMLElement | null = null;
  private picks = new Map<string, Pick>();
  private pending = false;
  private gone = false;
  /** 此刻画着的那条请求（哪个会话 · 请求 id · 是不是这里答出去的）。 */
  private shown: { sid: string; id: string; ours: boolean } | null = null;
  /** 别处先答了留下的那一行（哪个会话 · 哪条请求 · 那一句）。 */
  private goneLine: { sid: string; ask: string; said: string } | null = null;
  /** 跳过来要焦点的那个会话：块画出来那一刻落进去（一次）。 */
  private wantFocus: string | null = null;

  constructor(private readonly host: AnswerHost) {
    this.el = document.createElement("div");
    this.el.className = s.abBlock;
    this.el.setAttribute("role", "group");
    this.el.hidden = true;
    this.el.addEventListener("keydown", (e) => this.onKey(e));
  }

  /** 从「需手动」的入口跳到这个会话：块在 ⇒ 焦点落在第一颗；还没画出来 ⇒ 画出来那一刻再落。 */
  focusFor(sid: string): void {
    this.wantFocus = sid;
    this.takeFocus();
  }

  render(now: number = Date.now()): void {
    const tab = this.host.active();
    const n = tab ? needsOf(tab) : null;
    const sid = tab?.sessionId ?? null;
    if (this.goneLine && this.goneLine.sid !== sid) this.goneLine = null;
    // 画着的那条请求没了、不是这里答掉的 ⇒ 别处先答了（终端那边 · 另一个窗口）：块收起、留一行，直到下一条请求来或切走。
    const shown = this.shown;
    if (shown && (shown.sid !== sid || n?.ask?.id !== shown.id)) {
      this.shown = null;
      if (shown.sid === sid && !shown.ours && !this.goneLine) this.goneLine = { sid: shown.sid, ask: shown.id, said: tab?.autoAnswered ?? copyText("askBlock.gone.elsewhere") };
    }
    if (this.goneLine) {
      // 点了才知道的那一种：请求还在（等重读）⇒ 淡出的块 ＋ 那一行照旧。
      if (n?.ask && n.ask.id === this.goneLine.ask) return;
      if (!n?.ask) {
        this.drawGone(this.goneLine.said);
        return;
      }
      this.goneLine = null;
    }
    // 上一条提问没人答、那台替人答过：同一行（核心那一句），下一条请求来了才换掉。
    if (tab && !n && tab.autoAnswered) {
      this.drawGone(tab.autoAnswered);
      return;
    }
    if (!tab || !n) {
      if (this.drawn !== "") {
        this.drawn = "";
        this.el.hidden = true;
        this.el.replaceChildren();
        delete this.el.dataset.gone;
        delete this.el.dataset.goneOnly;
        this.host.room?.(0);
      }
      return;
    }
    const key = [tab.sessionId, n.head, n.ask?.id ?? "", n.manualText ?? ""].join("\u0000");
    if (key !== this.drawn) {
      this.drawn = key;
      this.draw(tab, n);
    }
    if (n.ask) this.shown ??= { sid: tab.sessionId, id: n.ask.id, ours: false };
    const waited = waitedNow(n, now);
    if (this.since) this.since.textContent = waited ? copyText("askBlock.head.since", { waited }) : "";
    this.takeFocus();
    this.host.room?.(this.el.offsetHeight);
  }

  /** 块收起、只留「已在别处答过」那一行（稳定态：下一条请求来了 / 切走才没）。 */
  private drawGone(said: string): void {
    const key = `gone\u0000${said}`;
    if (this.drawn === key) return;
    this.drawn = key;
    this.since = null;
    this.el.hidden = false;
    this.el.dataset.gone = "true";
    this.el.dataset.goneOnly = "true";
    this.el.replaceChildren(this.goneRow(said));
    this.host.room?.(this.el.offsetHeight);
  }

  private goneRow(said: string): HTMLElement {
    const line = document.createElement("div");
    line.className = s.abGone;
    line.dataset.part = "gone";
    line.setAttribute("role", "status");
    line.textContent = said;
    return line;
  }

  private takeFocus(): void {
    const tab = this.host.active();
    if (this.wantFocus === null || this.el.hidden || tab?.sessionId !== this.wantFocus) return;
    const first = this.el.querySelector<HTMLElement>("button:not([aria-disabled=true]), input");
    if (!first) return;
    this.wantFocus = null;
    first.focus();
  }

  private draw(tab: Tab, n: Needs): void {
    this.picks = new Map();
    this.pending = false;
    this.gone = false;
    delete this.el.dataset.gone;
    delete this.el.dataset.goneOnly;
    this.el.hidden = false;
    this.el.setAttribute("aria-label", n.head);
    const ask = n.ask;
    const head = document.createElement("div");
    head.className = s.abHead;
    head.dataset.part = "head";
    const title = document.createElement("span");
    title.className = s.abTitle;
    title.textContent = ask && ask.questions.length > 0 ? copyText("askBlock.head.questions", { head: n.head, n: ask.questions.length }) : n.head;
    head.append(title);
    if (ask?.run) {
      const tag = document.createElement("span");
      tag.className = s.abTag;
      tag.dataset.part = "tag";
      tag.textContent = ask.run.kind ? copyText("askBlock.tag.run", { kind: ask.run.kind }) : copyText("askBlock.tag.runBare");
      head.append(tag);
    }
    this.since = document.createElement("span");
    this.since.className = s.abSince;
    this.since.dataset.part = "since";
    head.append(this.since);
    this.body = document.createElement("div");
    this.body.className = s.abBody;
    this.el.replaceChildren(head, this.body);
    if (!ask) {
      this.drawManual(n);
      return;
    }
    if (ask.questions.length === 0 && ask.detail !== "") {
      const pre = document.createElement("pre");
      pre.className = s.abDetail;
      pre.textContent = ask.detail;
      this.body.before(pre);
    }
    this.drawChoices(tab, ask);
  }

  /**
   * 这里答不了（没挂上请求）：只写核心那一句为什么（自带的会话级设置读不出 · 自定义启动器起的 · 只能在终端里答的那种框）。
   * 块里不放「看终端」「切到终端」这类主按钮：终端只在会话头上「⋯」里当逃生口。
   */
  private drawManual(n: Needs): void {
    const row = document.createElement("div");
    row.className = s.abManual;
    const said = document.createElement("span");
    said.dataset.part = "manual";
    said.textContent = n.manualText ?? "";
    row.append(said);
    this.body!.replaceChildren(row);
  }

  /** 选项那几行（提问那一种先排题）。 */
  private drawChoices(tab: Tab, ask: Ask): void {
    const parts: HTMLElement[] = ask.questions.map((q) => this.question(q));
    const rows = new Map<string, HTMLElement>();
    // 段照核心给的 `row`；批计划那几项并成一行，有题时「不回答」与「提交答案」同一行。
    const oneRow = ask.kind === "plan" || ask.questions.length > 0;
    let key = 0;
    let first = true;
    for (const c of ask.choices) {
      const where = c.row;
      const line = oneRow ? "main" : where;
      let row = rows.get(line);
      if (!row) {
        row = document.createElement("div");
        row.className = s.abRow;
        row.dataset.row = line;
        rows.set(line, row);
      }
      const kind = where === "release" ? "ghost" : c.id === "deny-stop" ? "danger-text" : first ? "primary" : "secondary";
      first = false;
      const b = button({ label: c.note === "none" ? c.text : copyText("askBlock.choice.more", { label: c.text }), kind, onClick: () => this.pick(tab, ask, c, b) });
      b.dataset.choice = c.id;
      if (ask.kind === "approve" && where === "main" && key < 9) {
        b.dataset.key = String(++key);
        b.append(kbd(b.dataset.key));
      }
      row.append(b);
    }
    this.body!.replaceChildren(...parts, ...rows.values());
    this.refreshSubmit();
  }

  private question(q: AskQuestionItem): HTMLElement {
    const box = document.createElement("div");
    box.className = s.abQuestion;
    box.dataset.question = q.id;
    const pick: Pick = { options: new Set(), text: "" };
    this.picks.set(q.id, pick);
    const qhead = q.multi ? (q.header ? copyText("askBlock.question.multi", { header: q.header }) : copyText("askBlock.question.multiBare")) : q.header;
    if (qhead) {
      const h = document.createElement("span");
      h.className = s.abQHead;
      h.dataset.part = "qhead";
      h.textContent = qhead;
      box.append(h);
    }
    const text = document.createElement("span");
    text.className = s.abQText;
    text.dataset.part = "qtext";
    text.textContent = q.text;
    box.append(text);
    const opts: HTMLElement[] = [];
    const paint = () => {
      for (const o of opts) o.setAttribute("aria-checked", String(pick.options.has(o.dataset.option ?? "")));
    };
    for (const o of q.options) {
      const row = document.createElement("div");
      row.className = s.abOption;
      row.dataset.option = o.id;
      row.setAttribute("role", q.multi ? "checkbox" : "radio");
      row.tabIndex = 0;
      const mark = document.createElement("span");
      mark.className = s.abMark;
      mark.dataset.multi = String(q.multi);
      const label = document.createElement("span");
      label.textContent = o.text;
      row.append(mark, label);
      if (o.detail) {
        const d = document.createElement("span");
        d.className = s.abOptionDetail;
        d.textContent = o.detail;
        row.append(d);
      }
      const toggle = () => {
        if (this.pending || this.gone) return;
        if (q.multi) {
          if (pick.options.has(o.id)) pick.options.delete(o.id);
          else pick.options.add(o.id);
        } else {
          pick.options = new Set([o.id]);
          pick.text = "";
          other.value = "";
        }
        paint();
        this.refreshSubmit();
      };
      row.addEventListener("click", toggle);
      row.addEventListener("keydown", (e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          toggle();
        }
      });
      opts.push(row);
      box.append(row);
    }
    const otherRow = document.createElement("div");
    otherRow.className = s.abOption;
    const mark = document.createElement("span");
    mark.className = s.abMark;
    mark.dataset.multi = String(q.multi);
    const other = document.createElement("input");
    other.className = s.abOther;
    other.dataset.other = q.id;
    other.placeholder = copyText("askBlock.question.other");
    other.setAttribute("aria-label", copyText("askBlock.question.other"));
    other.addEventListener("input", () => {
      pick.text = other.value;
      if (!q.multi && other.value.trim() !== "") pick.options.clear();
      otherRow.setAttribute("aria-checked", String(other.value.trim() !== ""));
      paint();
      this.refreshSubmit();
    });
    otherRow.append(mark, other);
    box.append(otherRow);
    paint();
    return box;
  }

  /** 「提交答案」那一颗：每题都答了才可点。 */
  private refreshSubmit(): void {
    const submit = this.el.querySelector<HTMLButtonElement>("button[data-choice=answer]");
    if (!submit) return;
    const all = [...this.picks.values()].every((p) => p.options.size > 0 || p.text.trim() !== "");
    setDisabled(submit, all ? null : copyText("askBlock.question.needAll"));
  }

  private pick(tab: Tab, ask: Ask, c: AskChoice, b: HTMLButtonElement): void {
    if (this.pending || this.gone) return;
    if (c.note !== "none") {
      this.noteBox(tab, ask, c);
      return;
    }
    const req: AnswerRequest = { ask: ask.id, choice: c.id };
    if (c.id === "answer" && ask.questions.length > 0) {
      req.picks = Object.fromEntries([...this.picks].map(([id, p]) => [id, { options: [...p.options], text: p.text.trim() }]));
    }
    void this.send(tab, req, b);
  }

  /** 要附话的那一项：选项那几行原地换成一行输入框（Enter 交 · Esc 回）。 */
  private noteBox(tab: Tab, ask: Ask, c: AskChoice): void {
    const box = document.createElement("div");
    box.className = s.abNote;
    const input = document.createElement("input");
    input.className = s.abNoteInput;
    input.dataset.note = c.id;
    input.setAttribute("aria-label", c.text);
    const back = () => {
      this.drawChoices(tab, ask);
      this.el.querySelector<HTMLElement>(`button[data-choice="${c.id}"]`)?.focus();
    };
    const submit = () => {
      const note = input.value.trim();
      if (c.note === "required" && note === "") return;
      void this.send(tab, note === "" ? { ask: ask.id, choice: c.id } : { ask: ask.id, choice: c.id, note }, go);
    };
    input.addEventListener("keydown", (e) => {
      if (e.isComposing) return;
      if (e.key === "Enter") {
        e.preventDefault();
        submit();
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        back();
      }
    });
    const go = button({ label: c.text, kind: "primary", onClick: submit });
    go.dataset.choice = c.id;
    const cancel = button({ label: copyText("askBlock.note.cancel"), kind: "ghost", onClick: back });
    box.append(input, go, cancel);
    this.body!.replaceChildren(box);
    input.focus();
  }

  private async send(tab: Tab, req: AnswerRequest, b: HTMLButtonElement): Promise<void> {
    if (this.pending || this.gone) return;
    this.pending = true;
    setBusy(b, copyText("askBlock.choice.sending"));
    const drawn = this.drawn;
    const out = await this.host.answer(tab, req);
    if (this.drawn !== drawn) return; // 等的时候已经换成下一条了
    const sid = tab.sessionId;
    switch (out.kind) {
      case "answered":
        // 等那台推 changed {needs} 重读；块留着、不可再点。也顺手问一次（推送丢了也换得上）。
        if (this.shown && this.shown.id === req.ask) this.shown.ours = true;
        this.host.reread(sid);
        return;
      case "gone":
        setBusy(b, null);
        this.gone = true;
        this.el.dataset.gone = "true";
        for (const x of this.el.querySelectorAll<HTMLButtonElement>("button")) setDisabled(x, out.said);
        for (const x of this.el.querySelectorAll<HTMLInputElement>("input")) x.disabled = true;
        this.goneLine = { sid, ask: req.ask, said: out.said };
        this.el.append(this.goneRow(out.said));
        this.host.reread(sid);
        return;
      case "failed":
        setBusy(b, null);
        this.pending = false;
        this.host.toast(out.said, out.detail);
        return;
    }
  }

  /** 数字键：只在焦点在块里时生效（输入框里打字不算）。 */
  private onKey(e: KeyboardEvent): void {
    if (e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return;
    const t = e.target as HTMLElement | null;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) return;
    if (!/^[1-9]$/.test(e.key)) return;
    const b = this.el.querySelector<HTMLButtonElement>(`button[data-key="${e.key}"]`);
    if (!b) return;
    e.preventDefault();
    b.click();
  }
}
