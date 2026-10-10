/**
 * 计划「需手动」那一侧的排版（设计稿 planned-build 03）：详情 / 概览顶上那一条（四种：顶块走到看全局 · 判据红 · agent 提问 · 接手的会话停了）·
 * 退回过的那一格的「已退回 / 已落地」· 退回框。哪一条算不算、认可记在哪、那一行怎么拼、能不能送，全在后端；这里只排版、只转发。
 */
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { formDialog } from "../kit/dialog";
import { toast, failToast } from "../kit/toast";
import { writeClipboard } from "../clipboard";
import { copyText } from "../copy-table";
import { returnCell, PlanMiss, type PlanCell, type PlanNeed, type PlanSlice, type PlanWho } from "../plan-reads";
import { blockRoots, cellIndex } from "./plan-model";
import s from "./plan-review.module.css";

/** 计划页借给这里的几样。 */
export interface ReviewHost {
  origin(): import("../ipc/origin").Origin;
  workspace(): string;
  who(w: PlanWho): HTMLElement;
  /** 会话的名字（标签页标题；没有 ⇒ 前后几位）。 */
  sessionName(w: PlanWho | null): string;
  switchTo(sid: string): void;
  /** 那个会话的［恢复 ▾］菜单。 */
  resume(anchor: HTMLElement, sid: string): void;
  ack(need: PlanNeed): void;
  /** 下一条需手动（没有 ⇒ 不出那颗）。 */
  next: (() => void) | null;
  /** 送完了 ⇒ 重读这个工作区。 */
  reread(): void;
}

/** 这一条在这一页上该不该出：没认可的（agent 提问那一种不收认可，一直出到答了）。 */
export function openNeeds(slice: PlanSlice): PlanNeed[] {
  return slice.needs.filter((n) => !n.acked);
}

/** 一条需手动的那一条横条。`pos` ＝ 第几条 / 共几条（概览与详情同一个次序）。 */
export function needBar(host: ReviewHost, slice: PlanSlice, need: PlanNeed, pos: { i: number; n: number }): HTMLElement {
  const bar = document.createElement("div");
  bar.className = `${s.rvBar} plan-need`;
  bar.dataset.kind = need.kind;
  bar.tabIndex = 0;
  const head = document.createElement("div");
  head.className = s.rvHead;
  const title = document.createElement("span");
  title.className = s.rvTitle;
  title.append(icon(need.kind === "red" ? "warning" : "success", "compact"));
  const body: string[] = [];
  const acts: HTMLElement[] = [];
  const blk = slice.blocks.find((b) => b.id === need.block) ?? null;
  const byId = cellIndex(slice);
  const root = need.cell ? (byId.get(need.cell) ?? null) : null;
  const ackBtn = (): HTMLButtonElement => button({ label: copyText("plan.review.ack"), kind: "primary", size: "compact", onClick: () => host.ack(need) });
  if (need.kind === "top") {
    title.append(copyText("plan.review.topDone"));
    const p = slice.progress;
    body.push(copyText("plan.review.topDoneWhat", { phase: blk?.phase ?? "", done: p?.done ?? 0, of: (p?.done ?? 0) + (p?.open ?? 0) }));
    acts.push(ackBtn());
    if (root) acts.push(button({ label: copyText("plan.review.return"), icon: "send", size: "compact", onClick: () => openReturn(host, slice, root) }));
  } else if (need.kind === "red") {
    const red = need.red !== undefined ? slice.check.red[need.red] : undefined;
    title.append(copyText("plan.review.red", { rule: red?.rule ?? "", block: (root?.title ?? need.block) ?? "" }));
    if (red?.what) body.push(red.what);
    if (red?.fix) body.push(copyText("plan.review.redFix", { fix: red.fix }));
    acts.push(ackBtn());
    if (root) acts.push(button({ label: copyText("plan.review.tell"), icon: "send", size: "compact", onClick: () => openReturn(host, slice, root) }));
  } else if (need.kind === "ask") {
    title.append(copyText("plan.review.ask"));
    if (blk?.owner) title.appendChild(host.who(blk.owner));
    const sid = need.sid;
    if (sid) acts.push(button({ label: copyText("plan.review.askGo"), kind: "primary", size: "compact", onClick: () => host.switchTo(sid) }));
  } else {
    title.append(copyText("sessionState.planReview.ended"));
    if (blk?.owner) title.appendChild(host.who(blk.owner));
    const w = root?.whyCode;
    body.push(w?.kind === "inside" ? copyText("plan.review.endedWhat", { done: w.done, of: w.of }) : copyText("plan.review.endedOpen"));
    if (blk?.owner?.kind === "subagent") body.push(copyText("plan.review.endedSub"));
    const sid = need.sid;
    if (sid) {
      const r = button({ label: copyText("plan.review.resume"), kind: "primary", size: "compact", onClick: () => host.resume(r, sid) });
      acts.push(r);
    }
    acts.push(button({ label: copyText("plan.review.ack"), size: "compact", onClick: () => host.ack(need) }));
  }
  const side = document.createElement("span");
  side.className = s.rvSide;
  side.append(copyText("plan.review.pos", { i: pos.i + 1, n: pos.n }));
  const next = host.next;
  if (next && pos.n > 1) side.appendChild(button({ label: copyText("plan.review.next"), kind: "ghost", size: "compact", onClick: () => next() }));
  head.append(title, side);
  bar.appendChild(head);
  for (const b of body) {
    const p = document.createElement("div");
    p.className = s.rvBody;
    p.textContent = b;
    bar.appendChild(p);
  }
  const row = document.createElement("div");
  row.className = s.rvActs;
  row.append(...acts);
  if (need.kind !== "ask") {
    const note = document.createElement("span");
    note.className = s.rvNote;
    note.textContent = copyText("plan.review.ackNote");
    row.appendChild(note);
  }
  bar.appendChild(row);
  // 焦点在条上 Ctrl+Enter ＝ 认可（提问那一种没有认可）。
  bar.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && need.kind !== "ask") {
      e.preventDefault();
      host.ack(need);
    }
  });
  return bar;
}

/** 退回过的那一格：已退回（等它改）/ 已落地。没退回过 ⇒ `null`。 */
export function returnedBar(host: ReviewHost, cell: PlanCell): HTMLElement | null {
  const r = cell.returned;
  if (!r) return null;
  const bar = document.createElement("div");
  bar.className = `${s.rvBar} plan-returned`;
  bar.dataset.kind = r.state === "landed" ? "landed" : "returned";
  const t = document.createElement("div");
  t.className = s.rvBody;
  if (r.state === "landed") t.textContent = r.by === "child" ? copyText("plan.bar.landedChild", { title: r.child?.title ?? r.child?.id ?? "" }) : copyText("plan.bar.landedBody");
  else t.textContent = copyText("plan.bar.returned", { session: host.sessionName(r.to), time: r.atText ?? "" });
  bar.appendChild(t);
  return bar;
}

/** 退回框（做完了的格 ⇒「退回」；没做完 ⇒「说给负责的」，同一个框）。 */
export function openReturn(host: ReviewHost, slice: PlanSlice, cell: PlanCell): void {
  const done = cell.statusCode === "done";
  // 管这一格的那一块：从它自己往上找第一个块根格。
  const roots = blockRoots(slice);
  const byId = cellIndex(slice);
  let up: PlanCell | undefined = cell;
  while (up && !roots.has(up.id)) up = up.parent ? byId.get(up.parent) : undefined;
  const blk = up ? (roots.get(up.id) ?? null) : (slice.blocks.find((b) => b.id === "project") ?? null);
  const blkTitle = up ? (up.title ?? up.id) : copyText("plan.head.top");
  const owner = blk?.owner ?? cell.owner;
  const canSend = (w: PlanWho | null): boolean => w !== null && w.sid !== null && w.alive && w.activity !== "needs_you";
  let to: "owner" | "signer" = !canSend(owner) && canSend(cell.signer) ? "signer" : "owner";
  const box = document.createElement("div");
  box.className = s.rvForm;

  const whatHead = document.createElement("div");
  whatHead.className = s.rvFormHead;
  const whatLabel = document.createElement("label");
  whatLabel.textContent = copyText("plan.return.what");
  whatHead.append(whatLabel);
  const ta = document.createElement("textarea");
  ta.className = `${s.rvText} plan-return-text`;
  ta.rows = 3;
  whatLabel.htmlFor = ta.id = "plan-return-text";

  const toHead = document.createElement("div");
  toHead.className = s.rvFormHead;
  toHead.textContent = copyText("plan.return.to");
  const opts = document.createElement("div");
  opts.className = s.rvOpts;
  const option = (role: "owner" | "signer", w: PlanWho | null): HTMLElement => {
    const lab = document.createElement("label");
    lab.className = s.rvOpt;
    const r = document.createElement("input");
    r.type = "radio";
    r.name = "plan-return-to";
    r.checked = to === role;
    r.disabled = w === null;
    r.addEventListener("change", () => {
      to = role;
      sync();
    });
    const what = document.createElement("span");
    what.className = s.rvOptText;
    if (w) what.appendChild(host.who(w));
    what.append(
      w === null
        ? role === "signer"
          ? copyText("plan.return.noSigner")
          : copyText("plan.return.noOwner")
        : role === "owner"
          ? copyText("plan.return.roleOwner", { block: blkTitle })
          : copyText("plan.return.roleSigner"),
    );
    lab.append(r, what);
    if (r.disabled) lab.dataset.unavailable = "true";
    return lab;
  };
  opts.append(option("owner", owner), option("signer", cell.signer));
  // 默认那一项送不了、开框时已改选上一级 ⇒ 说一句（之后人自己改选不再说）。
  const switched: HTMLElement[] = [];
  if (to === "signer") {
    const n = document.createElement("div");
    n.className = s.rvNote;
    n.textContent = copyText("plan.return.autoSwitched");
    switched.push(n);
  }

  const lineHead = document.createElement("div");
  lineHead.className = s.rvFormHead;
  const lineLabel = document.createElement("span");
  lineLabel.textContent = copyText("plan.return.line");
  const lineNote = document.createElement("span");
  lineNote.className = s.rvNote;
  lineNote.textContent = copyText("plan.return.lineNote");
  lineHead.append(lineLabel, lineNote);
  const line = document.createElement("div");
  line.className = `${s.rvLine} plan-return-line`;
  // 送出的那一行由后端拼（`bePlan.return.line`，同一条文案）；框里跟着输入预览同一条，送出后以后端回的为准。
  const preview = (): string => copyText("bePlan.return.line", { id: cell.id, title: cell.title ?? "", text: ta.value.split(/\s+/).filter(Boolean).join(" ") });
  const sync = (): void => {
    line.textContent = preview();
    h.refresh();
  };
  box.append(whatHead, ta, toHead, opts, ...switched, lineHead, line);

  const h = formDialog({
    title: done ? copyText("plan.return.title", { title: cell.title ?? cell.id }) : copyText("plan.return.titleSend", { title: cell.title ?? cell.id }),
    action: done ? copyText("plan.return.send") : copyText("plan.return.sendTell"),
    body: box,
    wide: true,
    first: () => ta,
    blocked: () => (ta.value.trim() === "" ? copyText("plan.return.what") : null),
    dirty: () => ta.value.trim() !== "",
    submit: async () => {
      try {
        const r = await returnCell(host.origin(), { workspace: host.workspace(), slice: slice.name, id: cell.id, text: ta.value, to });
        if (r.result === "delivered" || r.result === "unsure") {
          toast(r.result === "delivered" ? copyText("plan.return.sent", { session: host.sessionName(r.to) }) : copyText("plan.return.unsure"), "", { level: r.result === "delivered" ? "success" : "warn" });
          host.reread();
          return null;
        }
        if (r.result === "copy") {
          const copy = button({
            label: copyText("plan.return.copyLine"),
            icon: "copy",
            size: "compact",
            onClick: () => void writeClipboard(r.line).then(() => toast(copyText("plan.return.copied"), "", { level: "success" }), (e: unknown) => failToast(copyText("detail.act.failed"), e, { level: "error" })),
          });
          line.textContent = r.line;
          lineHead.appendChild(copy);
        }
        return r.said ?? "";
      } catch (e) {
        return e instanceof PlanMiss ? e.said : String(e);
      }
    },
  });
  ta.addEventListener("input", sync);
  ta.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      if (ta.value.trim() !== "") h.submit();
    }
  });
  sync();
  void h.done;
}
