/**
 * **编辑已发的一句**（桌面）：标签页里人说的那一句悬停出「编辑」，气泡就地变编辑框、原文填好，它后面的卡淡掉（示意从这里往后会退掉）。
 *
 * - 改动全攒在这一端：编辑 · 改字 · 选怎么退都不碰会话；改字落本地草稿（按那台 ＋ 会话 ＋ 那一句，重开还在）。
 * - 开框那一下就问会话那台 `session-rewind-options`（带那一句），退法就地摆在字下面：「怎么退」那几项（缺省那一项先选上；不能选的灰着、
 *   为什么写在同一行后面）；主 agent 在干活时多一行「什么时候退」（缺省等这一轮跑完）。读中主按钮灰着；回包没有项 · 问不成 ⇒ 只写为什么、
 *   没有主按钮。选项的字都是那台写好的，这里不写死。宿主报那条会话的忙闲变了（[`SaidEdit.activity`]）⇒ 再问一次、照新回包重摆
 *   （选过的那一项还在就留着）；界面自己不判在不在干活。
 * - 一个主按钮「回退并发送」（回车同它 · Shift+回车换行 · Esc 收起、草稿留着）⇒ 带 `rewind {to, how, when}` 的那一句交给输入队那一份待发
 *   （宿主给的 [`SaidEditIo.send`]：同编号重交 · 回执才算出去）。等回执时主按钮「发送中」灰着、选项锁住。
 *   回执到了 ⇒ 草稿清掉、编辑框收起；那台拒了 ⇒ 框不收、那一句错误色写在按钮上方；没回执 ⇒ 编辑框收起、草稿留着，
 *   气泡末尾一行小字指向待发那一排（那一句在待发里，这里不另记）。
 * - 卡上那颗「编辑」只往上冒一个 [`SAID_EDIT_EVENT`]（带那一句），由宿主接住开编辑器：卡不认识编辑器，只读窗口不挂它就不出。
 */
import { copyText } from "./copy-table";
import { el } from "./kit/el";
import { button, setBusy, setDisabled } from "./kit/button";
import { radio } from "./kit/switch";
import { saidOf } from "./kit/detail";
import { imeComposing } from "./keybindings/ime";
import { dispatcher, type OverlayHandle } from "./keybindings/registry";
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

/** 给人说的那一句的卡挂上「编辑」（贴在气泡左边，悬停才显）。 */
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
  box: HTMLTextAreaElement;
  pick: HTMLElement;
  refused: HTMLElement;
  foot: HTMLElement;
  confirm: HTMLButtonElement;
  overlay: OverlayHandle;
  /** 选上的那一项「怎么退」（回包里没有能选的 ⇒ 不在，没有主按钮）。 */
  how?: Rewind["how"];
  when: Rewind["when"];
  /** 第几问：晚到的旧回包不摆。 */
  asked: number;
  sending: boolean;
}

/** 被编辑那一句后面的卡淡到多少。 */
const FADE = "0.35";

export class SaidEdit {
  private open_: Open | null = null;
  /** 各会话上一次报来的「在干什么」（只比变没变，不判它是什么）。 */
  private readonly doing = new Map<string, string | undefined>();

  constructor(private readonly io: SaidEditIo) {}

  /** 宿主报来那条会话此刻在干什么（会话脸上那一格）：开着它的编辑框、值变了 ⇒ 再问一次退法。 */
  activity(sid: string, doing: string | undefined): void {
    const had = this.doing.has(sid);
    const was = this.doing.get(sid);
    this.doing.set(sid, doing);
    const o = this.open_;
    if (!had || was === doing || o === null || o.at.sid !== sid || o.sending) return;
    void this.ask(o);
  }

  /** 在 `card` 上就地编辑 `at` 那一句（开着别的 ⇒ 先收起那一个）；当场问退法。 */
  open(card: HTMLElement, at: SaidAt): void {
    this.close();
    card.querySelector(':scope > [data-role="said-edit-pending"]')?.remove();
    const body = card.querySelector<HTMLElement>(".card-body");
    const editor = el("div", s.editor);
    const box = el("textarea", s.box);
    box.dataset.role = "said-edit-box";
    box.value = drafts()[draftKey(at)] ?? at.text;
    box.addEventListener("input", () => saveDraft(at, box.value));
    const pick = el("div", s.pick);
    const refused = el("p", s.error);
    refused.dataset.role = "said-edit-refused";
    refused.hidden = true;
    const cancel = button({ label: copyText("saidEdit.bar.cancel"), onClick: () => this.close() });
    cancel.dataset.role = "said-edit-cancel";
    const confirm = button({ label: copyText("saidEdit.pick.confirm"), kind: "primary", onClick: () => void this.submit(o) });
    confirm.dataset.role = "said-edit-confirm";
    const foot = el("div", s.foot);
    foot.append(el("span", s.keys, copyText("saidEdit.bar.keys")), cancel, confirm);
    editor.append(el("p", s.hint, copyText("saidEdit.bar.hint")), box, pick, refused, foot);
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
    // Esc 走浮层栈：只收这个框，不连带关别的。
    const overlay: OverlayHandle = {
      handleEsc: () => {
        if (this.open_ === o) this.close();
        return true;
      },
      passes: "all",
    };
    const o: Open = { at, card, editor, body, faded, box, pick, refused, foot, confirm, overlay, when: "after_turn", asked: 0, sending: false };
    box.addEventListener("keydown", (ev) => {
      if (imeComposing(ev)) return;
      if (ev.key === "Enter" && !ev.shiftKey) {
        ev.preventDefault();
        void this.submit(o);
      } else if (ev.key === "Escape") {
        ev.preventDefault();
        ev.stopPropagation();
        this.close();
      }
    });
    this.open_ = o;
    dispatcher.pushOverlay(overlay);
    box.focus();
    pick.replaceChildren(el("p", s.note, copyText("saidEdit.pick.loading")));
    setDisabled(confirm, copyText("saidEdit.pick.loading"));
    void this.ask(o);
  }

  /** 收起编辑框：正文回来、后面的卡不再淡（草稿留着）。 */
  close(): void {
    const o = this.open_;
    if (o === null) return;
    this.open_ = null;
    dispatcher.popOverlay(o.overlay);
    if (o.body !== null) o.editor.replaceWith(o.body);
    else o.editor.remove();
    for (const n of o.faded) {
      delete n.dataset.saidEditAfter;
      n.style.opacity = "";
    }
  }

  /** 问那台此刻给哪几项，就地摆出来（选过的那一项还在且能选 ⇒ 留着）。 */
  private async ask(o: Open): Promise<void> {
    const n = ++o.asked;
    let got: RewindOptions;
    try {
      got = await this.io.options(o.at.origin, o.at.sid, o.at.id);
    } catch (e) {
      if (this.open_ === o && o.asked === n) this.only(o, saidOf(e));
      return;
    }
    if (this.open_ !== o || o.asked !== n || o.sending) return;
    if (got.how.length === 0) return this.only(o, got.said ?? "");
    const can = got.how.filter((r) => r.enabled);
    o.how = (can.find((r) => r.how === o.how) ?? can.find((r) => r.default) ?? can[0])?.how;
    o.when = (got.when.find((r) => r.when === o.when) ?? got.when.find((r) => r.default) ?? got.when[0])?.when ?? "after_turn";
    const [hows, howOpts] = this.group(copyText("saidEdit.pick.how"));
    for (const r of got.how) {
      const row = radio("said-edit-how", r.text, r.how === o.how, () => (o.how = r.how));
      if (!r.enabled) {
        // 不能选：灰着、点不了，那台那一句为什么写在同一行后面。
        row.setAttribute("aria-disabled", "true");
        row.querySelector("input")!.disabled = true;
        if (r.why !== undefined) row.appendChild(el("span", s.why, r.why));
      }
      howOpts.appendChild(row);
    }
    const rows: HTMLElement[] = [hows];
    if (got.when.length > 0) {
      const [whens, whenOpts] = this.group(copyText("saidEdit.pick.when"));
      for (const r of got.when) whenOpts.appendChild(radio("said-edit-when", r.text, r.when === o.when, () => (o.when = r.when)));
      rows.push(whens);
    }
    if (got.said !== undefined) rows.push(el("p", s.note, got.said));
    o.pick.replaceChildren(...rows);
    // 一项都不能选 ⇒ 只摆出来（各带为什么），没有主按钮。
    if (o.how === undefined) o.confirm.remove();
    else {
      if (!o.confirm.isConnected) o.foot.appendChild(o.confirm);
      setDisabled(o.confirm, null);
    }
  }

  /** 「回退方式」那样的一组：左边名字、右边几项（回 [整组, 放几项的那一格]）。 */
  private group(name: string): [HTMLElement, HTMLElement] {
    const g = el("div", s.group);
    g.setAttribute("role", "radiogroup");
    g.setAttribute("aria-label", name);
    const opts = el("div", s.opts);
    g.append(el("span", s.label, name), opts);
    return [g, opts];
  }

  /** 退不了（空项 · 拒 · 离线）：只写为什么，没有主按钮。 */
  private only(o: Open, why: string): void {
    o.how = undefined;
    o.pick.replaceChildren(el("p", s.note, why));
    o.confirm.remove();
  }

  /** 「回退并发送」（主按钮 · 回车）：带 `rewind` 的那一句交出去，照结局收起或留着。 */
  private async submit(o: Open): Promise<void> {
    const how = o.how;
    if (this.open_ !== o || o.sending || how === undefined || o.confirm.getAttribute("aria-disabled") === "true") return;
    o.sending = true;
    o.refused.hidden = true;
    setBusy(o.confirm, copyText("terminal.queue.sending"));
    const locked = [...o.pick.querySelectorAll("input")].filter((i) => !i.disabled);
    for (const i of locked) i.disabled = true;
    const text = o.box.value;
    const end = await this.io.send(o.at.origin, o.at.sid, text, { to: o.at.id, how, when: o.when });
    saveDraft(o.at, end.at === "receipt" ? null : text);
    o.sending = false;
    setBusy(o.confirm, null);
    for (const i of locked) i.disabled = false;
    if (end.at === "refused") {
      o.refused.textContent = end.said;
      o.refused.hidden = false;
      return;
    }
    if (this.open_ === o) this.close();
    if (end.at === "none") {
      const mark = el("p", s.pending, copyText("saidEdit.card.pending"));
      mark.dataset.role = "said-edit-pending";
      o.card.appendChild(mark);
    }
  }
}
