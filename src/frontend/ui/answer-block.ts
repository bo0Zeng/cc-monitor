/**
 * **答题块**（消息流末尾、贴着聊天线底部）：当前会话在等批准 / 回答 / 批计划时，说清等什么、在这里就地答（设计稿 `调研/设计稿/答批准/` 甲1–甲7）。
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
 * 答出去之后等那台推 `changed {needs}` 重读（下一条挂着的换上来；没有了块收起）；别处先答了（already_answered · closed_elsewhere）⇒
 * 块淡出、按钮不可点、toast 照核心那一句、随即重读。
 */
import type { Tab } from "./tab-model";
import type { Ask, AskChoice, AskQuestionItem, Needs } from "./session-reads";
import { copyText } from "./copy-table";
import { button, setBusy, setDisabled } from "./kit/button";
import { kbd } from "./kit/badge";
import { needsOf } from "./session-face";
import { waitedNow } from "./duration-format";
import { isRemoteOrigin } from "./ipc/origin";
import { hasTerminal } from "./tab-session-state";
import { terminalFrontAvailable } from "./terminal-front";
import s from "./answer-block.module.css";

/** 去哪个终端答：Windows 有 ↗ ⇒ 切到终端；远端在 tmux 里 ⇒ 在终端里打开；都不行 ⇒ 只给看终端（抽屉里那一页）。 */
function terminalWhere(tab: Tab): "front" | "attach" | null {
  if (terminalFrontAvailable() && hasTerminal(tab.state)) return "front";
  if (isRemoteOrigin(tab.origin) && tab.state.liveness === "live" && tab.state.recoverability === "attachable") return "attach";
  return null;
}

/** 交给 `needs-answer` 的那一份（`sid` 由宿主按 tab 补上）。 */
export interface AnswerRequest {
  ask: string;
  choice: string;
  note?: string;
  picks?: Record<string, { options: string[]; text: string }>;
}

/** 答的结局：交到了 · 别处先答了 / 撤了（`said` 是核心那一句）· 没答成（别的原因，块照旧可答）。 */
export type AnswerOutcome = { kind: "answered" } | { kind: "gone"; said: string } | { kind: "failed"; said: string; detail: string };

export interface AnswerHost {
  active(): Tab | null;
  answer(tab: Tab, req: AnswerRequest): Promise<AnswerOutcome>;
  /** 再问一次这个会话的事实（needs 跟着换）。 */
  reread(sid: string): void;
  /** 抽屉的终端页换到这个会话。 */
  viewTerminal(sid: string): void;
  /** Windows：切到它的终端窗口（↗）。 */
  front(sid: string): void;
  /** 远端 tmux：在本机终端里打开它。 */
  attach(sid: string): void;
  toast(title: string, detail?: string): void;
  /** 块高了 / 收了：消息流底部给它让出多高（像素；收起 ⇒ 0）。 */
  room?(px: number): void;
}

/** 选项进哪一行（按协议里的选项 id；字只照抄）。 */
function rowOf(c: AskChoice): "main" | "deny" | "release" {
  if (c.id === "release") return "release";
  if (c.id === "deny" || c.id === "deny-note" || c.id === "deny-stop") return "deny";
  return "main";
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
    if (!tab || !n) {
      if (this.drawn !== "") {
        this.drawn = "";
        this.el.hidden = true;
        this.el.replaceChildren();
        delete this.el.dataset.gone;
        this.host.room?.(0);
      }
      return;
    }
    const key = [tab.sessionId, n.head, n.ask?.id ?? "", n.manualText ?? ""].join("\u0000");
    if (key !== this.drawn) {
      this.drawn = key;
      this.draw(tab, n);
    }
    const waited = waitedNow(n, now);
    if (this.since) this.since.textContent = waited ? copyText("askBlock.head.since", { waited }) : "";
    this.takeFocus();
    this.host.room?.(this.el.offsetHeight);
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
      this.drawManual(tab, n);
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

  private drawManual(tab: Tab, n: Needs): void {
    const row = document.createElement("div");
    row.className = s.abManual;
    const said = document.createElement("span");
    said.dataset.part = "manual";
    said.textContent = n.manualText ?? "";
    const sid = tab.sessionId;
    const acts = document.createElement("span");
    acts.className = s.abActs;
    acts.append(button({ label: copyText("askBlock.manual.terminal"), onClick: () => this.host.viewTerminal(sid) }));
    const where = terminalWhere(tab);
    if (where === "front") acts.append(button({ label: copyText("needs.bar.front"), icon: "front", onClick: () => this.host.front(sid) }));
    else if (where === "attach") acts.append(button({ label: copyText("needs.bar.openTerm"), onClick: () => this.host.attach(sid) }));
    row.append(said, acts);
    this.body!.replaceChildren(row);
  }

  /** 选项那几行（提问那一种先排题）。 */
  private drawChoices(tab: Tab, ask: Ask): void {
    const parts: HTMLElement[] = ask.questions.map((q) => this.question(q));
    const rows = new Map<string, HTMLElement>();
    let key = 0;
    let first = true;
    for (const c of ask.choices) {
      const where = rowOf(c);
      let row = rows.get(where);
      if (!row) {
        row = document.createElement("div");
        row.className = s.abRow;
        row.dataset.row = where;
        rows.set(where, row);
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
        this.host.reread(sid);
        return;
      case "gone":
        setBusy(b, null);
        this.gone = true;
        this.el.dataset.gone = "true";
        for (const x of this.el.querySelectorAll<HTMLButtonElement>("button")) setDisabled(x, out.said);
        for (const x of this.el.querySelectorAll<HTMLInputElement>("input")) x.disabled = true;
        this.host.toast(out.said);
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
