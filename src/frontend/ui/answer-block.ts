/**
 * **答题块**（消息流末尾、贴着聊天线底部）：当前会话在等批准 / 回答 / 批计划时，说清等什么、在这里就地答。
 * 不是模态框：不挡读聊天、出现时不抢焦点；从「需手动」的入口跳过来时焦点才进块（[`AnswerBlock.focusFor`]）。
 *
 * 只排版 —— 字、语气、要不要附话、题目全照核心那一份 `needs.ask`（后端 `agents/asks.rs`），一个字不在这里拼：
 * - 头一行 `needs.head`（「等批准 · Bash」）＋ 子 agent 那一趟的种类 ＋ 已等多久；要批的全文（命令 · 计划原文）进等宽块，长了块内滚；
 * - 选项按核心排好的序分三行：允许那一族（第一颗是主按钮，批准一步的标数字键）· 拒绝那一族（只有「拒绝并停下」红字）·
 *   改到终端里答（幽灵、排末尾）。哪一项进哪一行照核心给的 `row`、红字照 `tone: fail`，不认选项 id、不按字；
 * - 要附话的（`note` 不是 none）点了原地换一行输入框：Enter 交、Esc 回；必附的空着不交；
 * - 提问：每题照原样排（单选圆点 · 多选方块 · 末尾一格自由文本），每题都答了「提交答案」才可点，按题 id 交 picks；
 * - 没挂上请求（`ask: null`：信任框 · 别处起的会话 · 放行联网 … 每一种 needs.kind 都走这一支）⇒ 核心写好的那一句（只能在终端里答 / 答题口没接上）＋
 *   ［看终端］，能去它的终端时再多一颗（Windows ↗［切到终端］· 远端 tmux［在终端里打开］）。
 *
 * 答出去之后等那台推 `changed {needs}` 重读（下一条挂着的换上来；没有了块收起）。别处先答了 ⇒ 块淡出收起、留一行（稳定态，
 * 下一条请求来了 / 切走才没）：点了才知道的（回包 `result: "gone"`）那一行照核心那一句；没点、重读里那条请求没了的照会话事实 `askEnd` 那一句
 * （后端没说 ⇒ 块收起、不补字）。`askEnd.stays`（替人答了那一种）⇒ 同一行切走再回来也在，那条会话下一次挂上请求才换掉。
 *
 * 限高与收起（只是这个块的视图状态，不进后端）：
 * - 块最高约会话区四成（CSS），头一行不滚，底下那一截（全文 · 题 · 选项）块内滚；`room` 报的就是封顶后的高；
 * - 挂着请求时头一行末尾一颗［收起］：收着只剩头一行（核心那一句 ＋ 已等），点头一行任一处展开。每个会话记收着的是哪一条请求：
 *   切走再回来仍收着；那个会话换上来新的一条 ⇒ 展开；从「需手动」入口跳过来（`focusFor`）⇒ 展开；
 * - 多题只展开正在答的那一题，别的收成一行（题头 ＋ 选了什么 / 题目），点那一行换过去；单选点了跳到下一道没答的；
 *   题目排出来超三行先折着 ＋［展开全部］。
 */
import type { Tab } from "./tab-model";
import type { Ask, AskChoice, AskQuestionItem, Needs } from "./session-reads";
import { copyText } from "./copy-table";
import { button, setBusy, setButtonLabel, setDisabled } from "./kit/button";
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

/** 画出来的一道题：框 · 收着时那一行 · 题目那一格（折三行）· 题头（核心那一格；没有 ⇒ null）。 */
interface DrawnQuestion {
  q: AskQuestionItem;
  box: HTMLElement;
  line: HTMLElement;
  text: HTMLElement;
  head: string | null;
}

export class AnswerBlock {
  readonly el: HTMLDivElement;
  /** 画的是哪一份（会话 · 请求 · 那一句）；同一份重画只换已等多久，选到一半的不丢。 */
  private drawn: string | null = null;
  private since: HTMLElement | null = null;
  private body: HTMLElement | null = null;
  /** 头一行底下会滚的那一截（收着时不画）。 */
  private scroll: HTMLElement | null = null;
  private foldBtn: HTMLButtonElement | null = null;
  /** 每个会话收着的是哪一条请求（会话 id → 请求 id）；那个会话换上来别的一条 ⇒ 对不上 ⇒ 展开。 */
  private folded = new Map<string, string>();
  /** 此刻画着的请求（哪个会话 · 哪一条）：收起那颗认它；没挂上请求 ⇒ null（没得收）。 */
  private foldKey: { sid: string; id: string } | null = null;
  private questions: DrawnQuestion[] = [];
  /** 正在答的那一题（题 id）。 */
  private current: string | null = null;
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

  /** 从「需手动」的入口跳到这个会话：收着的展开；块在 ⇒ 焦点落在第一颗；还没画出来 ⇒ 画出来那一刻再落。 */
  focusFor(sid: string): void {
    this.wantFocus = sid;
    if (this.folded.delete(sid) && this.foldKey?.sid === sid) this.paintFold();
    this.takeFocus();
  }

  render(now: number = Date.now()): void {
    const tab = this.host.active();
    const n = tab ? needsOf(tab) : null;
    const sid = tab?.sessionId ?? null;
    if (this.goneLine && this.goneLine.sid !== sid) this.goneLine = null;
    // 画着的那条请求没了、不是这里答掉的 ⇒ 别处先答了（终端那边 · 另一个窗口 · 替人答了）：块收起、留后端写好的那一行，直到下一条请求来或切走。
    const shown = this.shown;
    if (shown && (shown.sid !== sid || n?.ask?.id !== shown.id)) {
      this.shown = null;
      const end = tab?.askEnd;
      if (shown.sid === sid && !shown.ours && !this.goneLine && end) this.goneLine = { sid: shown.sid, ask: shown.id, said: end.said };
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
    // 上一条请求收场那一格说留着（替人答过）：同一行（后端那一句），切走再回来也在，下一条请求来了才换掉。
    if (tab && !n && tab.askEnd?.stays) {
      this.drawGone(tab.askEnd.said);
      return;
    }
    if (!tab || !n) {
      if (this.drawn !== "") {
        this.drawn = "";
        this.el.hidden = true;
        this.el.replaceChildren();
        this.forget();
        delete this.el.dataset.gone;
        delete this.el.dataset.goneOnly;
        delete this.el.dataset.folded;
        this.host.room?.(0);
      }
      return;
    }
    const key = [tab.sessionId, n.head, n.ask?.id ?? "", n.manualText ?? ""].join("\u0000");
    if (key !== this.drawn) {
      this.drawn = key;
      this.draw(tab, n);
      this.measureClamp();
    }
    if (n.ask) this.shown ??= { sid: tab.sessionId, id: n.ask.id, ours: false };
    const waited = waitedNow(n, now);
    if (this.since) this.since.textContent = waited ? copyText("askBlock.head.since", { waited }) : "";
    this.takeFocus();
    this.host.room?.(this.el.offsetHeight);
  }

  /** 画着的那几格不再作数（块空了 / 只剩一行）。 */
  private forget(): void {
    this.since = null;
    this.body = null;
    this.scroll = null;
    this.foldBtn = null;
    this.foldKey = null;
    this.questions = [];
    this.current = null;
  }

  /** 块收起、只留「已在别处答过」那一行（稳定态：下一条请求来了 / 切走才没）。 */
  private drawGone(said: string): void {
    const key = `gone\u0000${said}`;
    if (this.drawn === key) return;
    this.drawn = key;
    this.forget();
    this.el.hidden = false;
    this.el.dataset.gone = "true";
    this.el.dataset.goneOnly = "true";
    delete this.el.dataset.folded;
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
    const first = this.scroll?.querySelector<HTMLElement>("button:not([aria-disabled=true]), input");
    if (!first) return;
    this.wantFocus = null;
    first.focus();
  }

  /** 收着 / 展开照 `folded` 画（头一行那颗换字），再照新高报让出多高。 */
  private paintFold(): void {
    const k = this.foldKey;
    const on = k !== null && this.folded.get(k.sid) === k.id;
    if (on) this.el.dataset.folded = "true";
    else delete this.el.dataset.folded;
    if (this.foldBtn) {
      setButtonLabel(this.foldBtn, on ? copyText("askBlock.fold.expand") : copyText("kit.dock.close"));
      this.foldBtn.dataset.fold = on ? "expand" : "collapse";
      this.foldBtn.setAttribute("aria-expanded", String(!on));
    }
    this.host.room?.(this.el.offsetHeight);
  }

  private setFolded(on: boolean): void {
    const k = this.foldKey;
    if (!k) return;
    if (on) this.folded.set(k.sid, k.id);
    else this.folded.delete(k.sid);
    this.paintFold();
    if (!on) this.measureClamp();
  }

  private draw(tab: Tab, n: Needs): void {
    this.picks = new Map();
    this.questions = [];
    this.current = null;
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
      tag.textContent = ask.run.text;
      head.append(tag);
    }
    this.since = document.createElement("span");
    this.since.className = s.abSince;
    this.since.dataset.part = "since";
    head.append(this.since);
    // 收着的那一条：同一个会话换上来别的一条 ⇒ 不再收着（展开一次）。
    const sid = tab.sessionId;
    if (ask && this.folded.has(sid) && this.folded.get(sid) !== ask.id) this.folded.delete(sid);
    this.foldKey = ask ? { sid, id: ask.id } : null;
    this.foldBtn = null;
    if (ask) {
      const fold = button({ label: copyText("kit.dock.close"), kind: "ghost", size: "compact", onClick: () => this.setFolded(this.el.dataset.folded !== "true") });
      fold.addEventListener("click", (e) => e.stopPropagation());
      this.foldBtn = fold;
      head.append(fold);
      head.addEventListener("click", () => {
        if (this.el.dataset.folded === "true") this.setFolded(false);
      });
    }
    this.body = document.createElement("div");
    this.body.className = s.abBody;
    this.scroll = document.createElement("div");
    this.scroll.className = s.abScroll;
    this.scroll.dataset.part = "scroll";
    this.scroll.append(this.body);
    this.el.replaceChildren(head, this.scroll);
    this.paintFold();
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
    this.questions = [];
    const parts: HTMLElement[] = ask.questions.map((q) => this.question(q));
    if (ask.questions.length > 0 && !ask.questions.some((q) => q.id === this.current)) this.current = this.nextOpen(null);
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
      const kind = where === "release" ? "ghost" : c.tone === "fail" ? "danger-text" : first ? "primary" : "secondary";
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
    const pick: Pick = this.picks.get(q.id) ?? { options: new Set(), text: "" };
    this.picks.set(q.id, pick);
    // 收着时那一行：点了这一题成了正在答的那一题。
    const line = document.createElement("div");
    line.className = s.abQLine;
    line.dataset.part = "qline";
    line.setAttribute("role", "button");
    line.tabIndex = 0;
    line.addEventListener("click", () => this.openQuestion(q.id));
    line.addEventListener("keydown", (e) => {
      if (e.key === " " || e.key === "Enter") {
        e.preventDefault();
        this.openQuestion(q.id);
      }
    });
    box.append(line);
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
    text.dataset.clamped = "true";
    text.textContent = q.text;
    box.append(text);
    this.questions.push({ q, box, line, text, head: qhead ?? null });
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
        // 单选点定了 ⇒ 往下一道没答的去（多选、自由文本还要接着填，不跳）。
        if (!q.multi) {
          const next = this.nextOpen(q.id);
          if (next !== null && next !== q.id) this.openQuestion(next);
        }
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

  private answered(id: string): boolean {
    const p = this.picks.get(id);
    return p !== undefined && (p.options.size > 0 || p.text.trim() !== "");
  }

  /** 下一道没答的（从 `after` 后面起，绕回开头）；都答了 ⇒ 留在 `after`（第一次画 ⇒ 第一道）。 */
  private nextOpen(after: string | null): string | null {
    const ids = this.questions.map((d) => d.q.id);
    const from = after === null ? 0 : ids.indexOf(after) + 1;
    for (let i = 0; i < ids.length; i++) {
      const id = ids[(from + i) % ids.length];
      if (!this.answered(id)) return id;
    }
    return after ?? ids[0] ?? null;
  }

  private openQuestion(id: string): void {
    this.current = id;
    this.paintQuestions();
    this.measureClamp();
    this.host.room?.(this.el.offsetHeight);
  }

  /** 正在答的那一题展开，别的收成一行：题头 ＋ 选了什么（没答的写题目）；字全是核心那几样，拼接只用分隔符。 */
  private paintQuestions(): void {
    const sep = copyText("kit.text.sep");
    for (const d of this.questions) {
      d.box.dataset.answering = String(d.q.id === this.current);
      const p = this.picks.get(d.q.id);
      const chosen = [...d.q.options.filter((o) => p?.options.has(o.id)).map((o) => o.text), ...(p && p.text.trim() !== "" ? [p.text.trim()] : [])];
      const parts = chosen.length > 0 ? [d.head ?? d.q.text, ...chosen] : [d.head, d.q.text].filter((x): x is string => x !== null && x !== "");
      d.line.textContent = parts.join(sep);
    }
  }

  /** 正在答那一题的题目：排出来超三行（折着的那一格装不下）才出［展开全部］；点了全显、不再收。 */
  private measureClamp(): void {
    const d = this.questions.find((x) => x.q.id === this.current);
    if (!d || this.el.dataset.folded === "true") return;
    const t = d.text;
    if (t.dataset.clamped !== "true" || d.box.querySelector("[data-part=qfull]")) return;
    if (t.scrollHeight <= t.clientHeight + 1) return;
    const wrap = document.createElement("div");
    wrap.className = s.abQFull;
    const full = button({
      label: copyText("agentWindow.brief.more"),
      kind: "ghost",
      size: "compact",
      onClick: () => {
        delete t.dataset.clamped;
        wrap.remove();
        this.host.room?.(this.el.offsetHeight);
      },
    });
    full.dataset.part = "qfull";
    wrap.append(full);
    t.after(wrap);
  }

  /** 「提交答案」那一颗：每题都答了才可点；收着的那几行跟着换字。 */
  private refreshSubmit(): void {
    this.paintQuestions();
    const submit = this.el.querySelector<HTMLButtonElement>("button[data-choice=answer]");
    if (!submit) return;
    const all = [...this.picks.keys()].every((id) => this.answered(id));
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
    const cancel = button({ label: copyText("kit.dialog.cancel"), kind: "ghost", onClick: back });
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
        this.scroll?.append(this.goneRow(out.said));
        this.host.reread(sid);
        return;
      case "failed":
        setBusy(b, null);
        this.pending = false;
        this.host.toast(out.said, out.detail);
        return;
    }
  }

  /** 数字键：只在焦点在块里时生效（输入框里打字不算；收着时选项看不见，不答）。 */
  private onKey(e: KeyboardEvent): void {
    if (e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return;
    if (this.el.dataset.folded === "true") return;
    const t = e.target as HTMLElement | null;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) return;
    if (!/^[1-9]$/.test(e.key)) return;
    const b = this.el.querySelector<HTMLButtonElement>(`button[data-key="${e.key}"]`);
    if (!b) return;
    e.preventDefault();
    b.click();
  }
}
