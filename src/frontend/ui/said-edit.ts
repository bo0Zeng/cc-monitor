/**
 * **编辑已发的一句**（桌面）：标签页里人说的那一句悬停出「编辑」，气泡就地变编辑框、原文填好，它后面的卡淡掉（示意从这里往后会退掉）。
 *
 * - 改动全攒在这一端：编辑 · 改字 · 选怎么退都不碰会话；改字落本地草稿（按那台 ＋ 会话 ＋ 那一句，重开还在）。
 * - 「发送」⇒ 问会话那台 `session-rewind-options`（带那一句），照回包摆「怎么退」那几项（缺省那一项先选上；不能选的灰着、下面写为什么）；主 agent 在干活时多一行「什么时候退」
 *   （缺省等这一轮跑完）。回包没有项 ⇒ 只写那台那一句为什么，不给确认。选项的字都是那台写好的，这里不写死。
 * - 「确认」⇒ 带 `rewind {to, how, when}` 的那一句交给输入队那一份待发（宿主给的 [`SaidEditIo.send`]：同编号重交 · 回执才算出去）。
 *   回执到了 ⇒ 草稿清掉、编辑框收起；那台拒了 ⇒ 字留着、说那台那一句；没回执 ⇒ 编辑框收起、草稿留着（待发那一排写「发送中」）。
 * - 卡上那颗「编辑」只往上冒一个 [`SAID_EDIT_EVENT`]（带那一句），由宿主接住开编辑器：卡不认识编辑器，只读窗口不挂它就不出。
 */
import { copyText } from "./copy-table";
import { el } from "./kit/el";
import { button } from "./kit/button";
import { radio } from "./kit/switch";
import { saidOf } from "./kit/detail";
import { LS_KEYS, safeGetJson, safeSetJson } from "./local-storage";
import type { Origin } from "./ipc/origin";
import type { PutEnd } from "./session-queue";
import type { Rewind } from "./generated/Rewind";
import type { RewindOptions } from "./generated/RewindOptions";
import s from "./said-edit.module.css";

/** 被编辑的那一句：哪台 · 哪条会话 · 记录里那一行的 id（即 `uuid`）· 原文。 */
export interface SaidAt {
  origin: Origin;
  sid: string;
  id: string;
  text: string;
}

/** 编辑器要的两口：问选项（退到 `to` 那一句之前）· 把改好的那一句（带 `rewind`）交给输入队那一份待发。 */
export interface SaidEditIo {
  options(origin: Origin, sid: string, to: string): Promise<RewindOptions>;
  send(origin: Origin, sid: string, text: string, rewind: Rewind): Promise<PutEnd>;
}

/** 卡上「编辑」点了往上冒的事件（`detail.at` 是那一句）。 */
export const SAID_EDIT_EVENT = "said-edit";

/** 给人说的那一句的卡挂上「编辑」（卡头右边，悬停才显）。 */
export function markEditable(card: HTMLElement, at: SaidAt): void {
  const b = button({
    label: copyText("saidEdit.card.edit"),
    kind: "ghost",
    size: "compact",
    onClick: () => card.dispatchEvent(new CustomEvent(SAID_EDIT_EVENT, { bubbles: true, detail: { at } })),
  });
  // 悬停才显：同分叉按钮那一套（全局 `styles.css` 的 `.has-said-edit:hover .said-edit-btn`）。
  b.classList.add("said-edit-btn");
  b.dataset.role = "said-edit";
  card.classList.add("has-said-edit");
  // 人说的那一句的卡头不显（`.card-user .card-header`），按钮直接挂在气泡上、贴在它左边。
  card.appendChild(b);
}

/** 草稿那一格的键：那台 ＋ 会话 ＋ 那一句。 */
const draftKey = (at: SaidAt): string => `${at.origin}\u0000${at.sid}\u0000${at.id}`;

function drafts(): Record<string, string> {
  const v = safeGetJson<Record<string, unknown>>(LS_KEYS.saidEditDrafts);
  const out: Record<string, string> = {};
  if (v !== null && typeof v === "object") for (const [k, t] of Object.entries(v)) if (typeof t === "string") out[k] = t;
  return out;
}

function saveDraft(at: SaidAt, text: string | null): void {
  const all = drafts();
  if (text === null) delete all[draftKey(at)];
  else all[draftKey(at)] = text;
  safeSetJson(LS_KEYS.saidEditDrafts, all);
}

/** 流里一张卡所在的那一行（`.stream-content` 的直接子元素；不在流里 ⇒ 卡自己）。 */
function rowOf(card: HTMLElement): HTMLElement {
  let e: HTMLElement = card;
  while (e.parentElement !== null && !e.parentElement.classList.contains("stream-content")) e = e.parentElement;
  return e.parentElement === null ? card : e;
}

/** 开着的那一个编辑框（`body` 是编辑时摘下的气泡正文，收起时放回原处）。 */
interface Open {
  at: SaidAt;
  card: HTMLElement;
  editor: HTMLElement;
  body: HTMLElement | null;
  faded: HTMLElement[];
}

/** 被编辑那一句后面的卡淡到多少。 */
const FADE = "0.35";

export class SaidEdit {
  private open_: Open | null = null;

  constructor(private readonly io: SaidEditIo) {}

  /** 在 `card` 上就地编辑 `at` 那一句（开着别的 ⇒ 先收起那一个）。 */
  open(card: HTMLElement, at: SaidAt): void {
    this.close();
    const body = card.querySelector<HTMLElement>(".card-body");
    const editor = el("div", s.editor);
    const box = el("textarea", s.box);
    box.dataset.role = "said-edit-box";
    box.value = drafts()[draftKey(at)] ?? at.text;
    box.addEventListener("input", () => saveDraft(at, box.value));
    const pick = el("div", s.pick);
    const bar = el("div", s.bar);
    const cancel = button({ label: copyText("saidEdit.bar.cancel"), onClick: () => this.close() });
    cancel.dataset.role = "said-edit-cancel";
    const send = button({ label: copyText("saidEdit.bar.send"), kind: "primary", onClick: () => void this.ask(at, box, pick) });
    send.dataset.role = "said-edit-send";
    bar.append(cancel, send);
    editor.append(el("p", s.hint, copyText("saidEdit.bar.hint")), box, bar, pick);
    // 正文先摘下（收起时放回原处），编辑框占它的位置。
    if (body !== null) body.replaceWith(editor);
    else card.appendChild(editor);
    const faded: HTMLElement[] = [];
    for (let n = rowOf(card).nextElementSibling; n !== null; n = n.nextElementSibling) {
      if (!(n instanceof HTMLElement)) continue;
      n.dataset.saidEditAfter = "1";
      n.style.opacity = FADE;
      faded.push(n);
    }
    this.open_ = { at, card, editor, body, faded };
    box.focus();
  }

  /** 收起编辑框：正文回来、后面的卡不再淡（草稿留着）。 */
  close(): void {
    const o = this.open_;
    if (o === null) return;
    this.open_ = null;
    if (o.body !== null) o.editor.replaceWith(o.body);
    else o.editor.remove();
    for (const n of o.faded) {
      delete n.dataset.saidEditAfter;
      n.style.opacity = "";
    }
  }

  /** 「发送」：问那台此刻给哪几项，摆出来。 */
  private async ask(at: SaidAt, box: HTMLTextAreaElement, pick: HTMLElement): Promise<void> {
    pick.replaceChildren(el("p", s.note, copyText("saidEdit.pick.loading")));
    let got: RewindOptions;
    try {
      got = await this.io.options(at.origin, at.sid, at.id);
    } catch (e) {
      pick.replaceChildren(el("p", s.note, saidOf(e)));
      return;
    }
    if (this.open_?.at !== at) return;
    if (got.how.length === 0) {
      pick.replaceChildren(el("p", s.note, got.said ?? ""));
      return;
    }
    let how = (got.how.find((r) => r.enabled && r.default) ?? got.how.find((r) => r.enabled))?.how;
    let when = (got.when.find((r) => r.default) ?? got.when[0])?.when ?? "after_turn";
    const hows = el("div", s.group);
    hows.setAttribute("role", "radiogroup");
    hows.append(el("span", s.label, copyText("saidEdit.pick.how")));
    for (const r of got.how) {
      const row = radio("said-edit-how", r.text, r.how === how, () => (how = r.how));
      if (!r.enabled) {
        // 不能选：灰着、点不了，下面写那台那一句为什么。
        row.setAttribute("aria-disabled", "true");
        row.querySelector("input")!.disabled = true;
      }
      hows.appendChild(row);
      if (r.why !== undefined) hows.appendChild(el("p", s.note, r.why));
    }
    const rows: HTMLElement[] = [hows];
    if (got.when.length > 0) {
      const whens = el("div", s.group);
      whens.setAttribute("role", "radiogroup");
      whens.append(el("span", s.label, copyText("saidEdit.pick.when")));
      for (const r of got.when) whens.appendChild(radio("said-edit-when", r.text, r.when === when, () => (when = r.when)));
      rows.push(whens);
    }
    pick.replaceChildren(...rows);
    // 一项都不能选 ⇒ 只摆出来（各带为什么），不给确认。
    if (how !== undefined) {
      const confirm = button({
        label: copyText("saidEdit.pick.confirm"),
        kind: "primary",
        onClick: () => {
          if (how !== undefined) void this.confirm(at, box.value, { to: at.id, how, when }, pick);
        },
      });
      confirm.dataset.role = "said-edit-confirm";
      pick.appendChild(confirm);
    }
    if (got.said !== undefined) pick.appendChild(el("p", s.note, got.said));
  }

  /** 「回退并发送」：带 `rewind` 的那一句交出去，照结局收起或留着（拒了 ⇒ 那台那一句写在选项下面）。 */
  private async confirm(at: SaidAt, text: string, rewind: Rewind, pick: HTMLElement): Promise<void> {
    const end = await this.io.send(at.origin, at.sid, text, rewind);
    if (end.at === "receipt") saveDraft(at, null);
    else saveDraft(at, text);
    if (end.at === "refused") {
      pick.appendChild(el("p", s.note, end.said));
      return;
    }
    if (this.open_?.at === at) this.close();
  }
}
