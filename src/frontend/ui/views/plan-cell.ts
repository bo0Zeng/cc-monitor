/**
 * **一个节点的详情**：计划页右栏选了节点之后的样子。
 *
 * 从上到下：在哪（标题串）→ 头（状态 · 标题 · 类 · 状态那一行 · 原因）→ 位置（负责人 · 阶段 ·［切到会话］· 没人接着 ⇒ 交接说明）→
 * 要做成什么样（md；话里提到的节点照 pb 给的位置换成标题链接）→ 里面 → 连着的 · 连着它的（按这一类的人话分组）→
 * 管的文件 → 状态（值 · 依据它的 · 还没执行的 sets）→ agent 站在这个节点看到的（折着，展开才问）→ 跟 agent 说时用的编号。
 *
 * 凡出现一格，一律写成「编号 标题」（后端拼好的 `labels`）。只排版：成品都是后端给的。
 */
import { button, setDisabled } from "../kit/button";
import { icon } from "../kit/icon";
import { foldCaret } from "../kit/fold";
import { toast, failToast } from "../kit/toast";
import { copyText } from "../copy-table";
import { saidOf } from "../kit/detail";
import { writeClipboard } from "../clipboard";
import { renderMarkdown } from "../render";
import type { Origin } from "../ipc/origin";
import type { NewSessionSpec } from "../new-session";
import { fetchCellView, PLAN_EDGE_KINDS, type PlanBlock, type PlanCell, type PlanEdgeKind, type PlanRef, type PlanSlice, type PlanWho } from "../plan-reads";
import { ancestry, blockRoots, cellIndex, kindSlot, labelOf, refSpans } from "./plan-model";
import { phaseBadge, setKindColor, STATUS_ICON } from "./plan-bits";
import s from "./plan-cell.module.css";

export interface CellDetailHost {
  select(id: string | null): void;
  who(w: PlanWho): HTMLElement;
  origin(): Origin;
  workspace(): string;
  switchTo(sid: string): void;
  /** 头右那一颗：做完了 ⇒［退回…］；没做完 ⇒［说给负责的…］（同一个框）。 */
  returnCell(slice: PlanSlice, cell: PlanCell): void;
  /** ［新会话接手…］：开全产品那一个新建会话框（`takeoverSpec`）。 */
  takeover(b: PlanBlock): void;
  /** ［问图…］：开问图那个框（`plan-ask.ts`）。 */
  ask(slice: PlanSlice, cell: PlanCell): void;
  /** 那台离线那一句（计划页知道那台断了 ⇒ ［问图…］灰、悬停摆它）；连着 / 不知道 ⇒ `null`。缺 ⇒ 当连着。 */
  askOff?(): string | null;
}

/** 正文里占位的两头（私用区的两个字，渲 md 时原样过）。 */
const REF_OPEN = String.fromCharCode(0xe000);
const REF_CLOSE = String.fromCharCode(0xe001);
const REF_SPLIT = new RegExp(`${REF_OPEN}(\\d+)${REF_CLOSE}`);

/** 正文多于这么多行 ⇒ 先折起。 */
const FOLD_LINES = 14;

export class CellDetail {
  /** 问过的 agent 视角：`仓库 \u0000 节点 \u0000 rev` ⇒ 那一段 / 没有。同一格再开不重问（rev 变了才重问）。 */
  private views = new Map<string, { text: string } | { none: string }>();
  /** 展开着 agent 视角的那一格。 */
  private viewOpen: string | null = null;
  /** 正文展开全部的那一格。 */
  private bodyOpen: string | null = null;

  constructor(private readonly host: CellDetailHost) {}

  render(slice: PlanSlice, cell: PlanCell, rev = ""): HTMLElement {
    const box = document.createElement("div");
    box.className = `${s.cd} plan-cell`;
    box.append(this.crumb(slice, cell), this.head(slice, cell));
    const pos = this.position(slice, cell);
    if (pos) box.appendChild(pos);
    const hand = this.handoff(slice, cell);
    if (hand) box.appendChild(hand);
    if (cell.body) box.appendChild(this.design(slice, cell));
    if (cell.children.length > 0) box.appendChild(this.inside(slice, cell));
    const edges = this.edges(slice, cell);
    if (edges) box.appendChild(edges);
    const rings = this.rings(slice, cell);
    if (rings) box.appendChild(rings);
    if (cell.files.length > 0) box.appendChild(this.files(slice, cell));
    box.appendChild(this.state(slice, cell));
    box.appendChild(this.agentView(slice, cell, rev));
    box.appendChild(this.idLine(cell));
    return box;
  }

  // ── 在哪 · 头 · 位置 ──

  private crumb(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const c = document.createElement("div");
    c.className = s.cdCrumb;
    const top = link(slice.name, () => this.host.select(null));
    c.appendChild(top);
    for (const a of ancestry(slice, cell.id)) {
      c.append(icon("caretRight", "compact"), link(labelOf(slice, a.id), () => this.host.select(a.id)));
    }
    return c;
  }

  private head(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const h = document.createElement("div");
    h.className = s.cdHead;
    const look = cell.look;
    const st = document.createElement("span");
    st.className = s.cdSt;
    st.dataset.look = look;
    st.appendChild(icon(STATUS_ICON[look]));
    const t = document.createElement("span");
    t.className = s.cdTitle;
    t.dataset.look = look;
    // 「编号 标题」是后端拼的那一份；标题里提到的格照位置换链接，前面那一截（编号）原样。
    const label = labelOf(slice, cell.id);
    const title = cell.title ?? "";
    if (title && label.endsWith(title)) t.append(document.createTextNode(label.slice(0, label.length - title.length)), ...this.refText(slice, title, cell.refs.title));
    else t.textContent = label;
    h.append(st, t);
    if (cell.kind) h.appendChild(kindChip(slice, cell.kind));
    const badge = document.createElement("span");
    badge.className = s.cdBadge;
    badge.dataset.look = look;
    badge.textContent = cell.stateLine ?? "";
    h.appendChild(badge);
    if (cell.whyShort) h.appendChild(span(cell.whyShort.text, s.cdWhy));
    if (cell.covers.includes("project")) h.appendChild(span(copyText("plan.cell.coversAll"), s.cdWhy));
    const gap = document.createElement("span");
    gap.className = s.cdGap;
    const ask = button({ label: copyText("plan.ask.open"), icon: "chat", size: "compact", onClick: () => this.host.ask(slice, cell) });
    setDisabled(ask, this.host.askOff?.() ?? null);
    h.append(
      gap,
      button({ label: look === "done" ? copyText("plan.review.return") : copyText("plan.review.tell"), icon: "send", size: "compact", onClick: () => this.host.returnCell(slice, cell) }),
      ask,
    );
    return h;
  }

  /**
   * 划出的根节点给接手（后端那一格 `takeover`：`open` · `no_line`）⇒ pb 给的交接说明（不带认领码；句末 pb 写了认领码找谁要）＋
   * ［复制］＋［新会话接手…］；给不了 ⇒ pb 说的为什么。本页不替人敲 `pb reissue`。
   */
  private handoff(slice: PlanSlice, cell: PlanCell): HTMLElement | null {
    const b = blockRoots(slice).get(cell.id) ?? null;
    if (!b || (b.takeover !== "open" && b.takeover !== "no_line")) return null;
    const box = handoffBox(b);
    if (b.takeover === "open") box.appendChild(takeoverCell(b, () => this.host.takeover(b)));
    return box;
  }

  /** 位置行：划出的根节点 ⇒ 那个权力范围的负责人 ＋ 阶段；别的节点 ⇒ 负责它的。都带［切到会话］。 */
  private position(slice: PlanSlice, cell: PlanCell): HTMLElement | null {
    const b = blockRoots(slice).get(cell.id) ?? null;
    const who = b?.owner ?? cell.owner;
    if (!who && !b) return null;
    const row = document.createElement("div");
    row.className = s.cdPos;
    if (b) row.appendChild(span(copyText("plan.cell.blockRoot", { block: labelOf(slice, cell.id) })));
    else row.appendChild(span(copyText("plan.cell.grower")));
    if (who) row.appendChild(this.host.who(who));
    if (b?.phase) row.appendChild(phaseBadge(b.phase));
    const sid = who?.sid ?? null;
    if (sid) row.appendChild(link(copyText("plan.cell.toSession"), () => this.host.switchTo(sid)));
    return row;
  }

  // ── 要做成什么样 · 里面 · 边 ──

  private design(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const sec = section(copyText("plan.cell.design"));
    const body = cell.body ?? "";
    const prose = document.createElement("div");
    prose.className = `${s.cdProse} plan-prose`;
    this.fillMarkdown(prose, slice, body, cell.refs.body);
    sec.appendChild(prose);
    const lines = body.split("\n").length;
    if (lines > FOLD_LINES && this.bodyOpen !== cell.id) {
      prose.dataset.folded = "true";
      const more = link(copyText("plan.cell.expand"), () => {
        this.bodyOpen = cell.id;
        delete prose.dataset.folded;
        more.remove();
      });
      sec.appendChild(more);
    }
    return sec;
  }

  /** md 里提到的格：先把 pb 给的那几段换成占位，渲完再把占位换成标题链接（代码里的 pb 本就不算）。 */
  private fillMarkdown(el: HTMLElement, slice: PlanSlice, body: string, refs: readonly PlanRef[]): void {
    const sorted: PlanRef[] = [];
    const md = refSpans(body, refs)
      .map((p) => {
        if (typeof p === "string") return p;
        sorted.push(p);
        return `${REF_OPEN}${sorted.length - 1}${REF_CLOSE}`;
      })
      .join("");
    el.innerHTML = renderMarkdown(md);
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    const hits: Text[] = [];
    while (walker.nextNode()) if ((walker.currentNode.nodeValue ?? "").includes(REF_OPEN)) hits.push(walker.currentNode as Text);
    for (const t of hits) {
      const parts = (t.nodeValue ?? "").split(REF_SPLIT);
      const frag = document.createDocumentFragment();
      parts.forEach((p, j) => {
        if (j % 2 === 0) {
          if (p) frag.appendChild(document.createTextNode(p));
        } else {
          const r = sorted[Number(p)];
          frag.append(...this.refLinks(slice, r));
        }
      });
      t.replaceWith(frag);
    }
  }

  /** 一段话（标题）照位置换链接。 */
  refText(slice: PlanSlice, text: string, refs: readonly PlanRef[]): Node[] {
    return refSpans(text, refs).flatMap((p) => (typeof p === "string" ? [document.createTextNode(p)] : this.refLinks(slice, p)));
  }

  private refLinks(slice: PlanSlice, r: PlanRef): Node[] {
    const byId = cellIndex(slice);
    const out: Node[] = [];
    r.ids.forEach((id, k) => {
      if (k > 0) out.push(document.createTextNode(copyText("plan.page.listSep")));
      const c = byId.get(id);
      const a = document.createElement("button");
      a.type = "button";
      a.className = `${s.cdRef} plan-ref`;
      a.textContent = labelOf(slice, id);
      if (c) a.addEventListener("click", () => this.host.select(id));
      out.push(a);
    });
    return out;
  }

  private inside(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const sec = section(copyText("plan.cell.inside"));
    const byId = cellIndex(slice);
    for (const k of cell.children) {
      const c = byId.get(k);
      if (c) sec.appendChild(this.cellLine(slice, c));
    }
    return sec;
  }

  /** 一行：状态图标 · 标题（链接）· 类的词 · 没签收时原因短词。 */
  private cellLine(slice: PlanSlice, c: PlanCell): HTMLElement {
    const row = document.createElement("div");
    row.className = s.cdLine;
    setKindColor(row, kindSlot(slice, c.kind));
    const look = c.look;
    const st = document.createElement("span");
    st.className = s.cdSt;
    st.dataset.look = look;
    st.appendChild(icon(STATUS_ICON[look], "compact"));
    row.append(st, link(labelOf(slice, c.id), () => this.host.select(c.id)));
    if (c.kind) row.appendChild(span(c.kind, s.cdKw));
    if (c.whyShort) row.appendChild(span(c.whyShort.text, s.cdMuted));
    return row;
  }

  /** 连着的 · 连着它的：出边用这一格那一类的词，指着它的用对方那一类的词；空组不出。 */
  private edges(slice: PlanSlice, cell: PlanCell): HTMLElement | null {
    const byId = cellIndex(slice);
    const wordOf = (kind: string | null, e: PlanEdgeKind): string | null => slice.kinds.find((k) => k.name === kind)?.edgeWords[e] ?? null;
    const groups = new Map<string, PlanCell[]>();
    const add = (label: string, id: string): void => {
      const c = byId.get(id);
      if (!c) return;
      const g = groups.get(label) ?? [];
      if (g.includes(c)) return;
      g.push(c);
      groups.set(label, g);
    };
    for (const e of PLAN_EDGE_KINDS) for (const id of cell.edges[e]) add(outLabel(e, wordOf(cell.kind, e)), id);
    for (const e of PLAN_EDGE_KINDS) for (const id of cell.pointedBy[e]) add(inLabel(e, wordOf(byId.get(id)?.kind ?? null, e)), id);
    if (groups.size === 0) return null;
    const sec = section(copyText("plan.cell.edges"));
    for (const [label, cells] of groups) {
      const g = document.createElement("div");
      g.className = s.cdGroup;
      g.appendChild(span(copyText("plan.edge.group", { word: label, n: cells.length }), s.cdGroupHead));
      for (const c of cells) g.appendChild(this.cellLine(slice, c));
      sec.appendChild(g);
    }
    return sec;
  }

  /**
   * 圈：covers 是格身上的一份名单（不是边），照 pb 列在格的面板上 —— 圈住它的（`coveredBy`，pb 算的）· 它圈住的（`covers`，写的）；
   * 写了 `project` ⇒ 那一项写「管全部」。两样都空不出。
   */
  private rings(slice: PlanSlice, cell: PlanCell): HTMLElement | null {
    const byId = cellIndex(slice);
    const inside = cell.coveredBy.map((id) => byId.get(id)).filter((c): c is PlanCell => c !== undefined);
    const outs = cell.covers.filter((id) => id !== "project").map((id) => byId.get(id)).filter((c): c is PlanCell => c !== undefined);
    const all = cell.covers.includes("project");
    if (inside.length === 0 && outs.length === 0 && !all) return null;
    const sec = section(copyText("plan.cell.rings"));
    const group = (word: string, cells: PlanCell[], extra?: HTMLElement): void => {
      const g = document.createElement("div");
      g.className = s.cdGroup;
      g.appendChild(span(copyText("plan.edge.group", { word, n: cells.length + (extra ? 1 : 0) }), s.cdGroupHead));
      if (extra) g.appendChild(extra);
      for (const c of cells) g.appendChild(this.cellLine(slice, c));
      sec.appendChild(g);
    };
    if (inside.length > 0) group(copyText("plan.ring.in"), inside);
    if (outs.length > 0 || all) group(copyText("plan.ring.out"), outs, all ? span(copyText("plan.cell.coversAll"), s.cdMuted) : undefined);
    return sec;
  }

  // ── 文件 · 状态 · agent 看到的 · 编号 ──

  private files(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const sec = section(copyText("plan.cell.files"));
    const tbl = document.createElement("table");
    tbl.className = s.cdTbl;
    const head = tbl.createTHead().insertRow();
    for (const h of [copyText("plan.file.path"), copyText("plan.file.state"), copyText("plan.file.note"), copyText("plan.file.also")]) {
      const th = document.createElement("th");
      th.textContent = h;
      head.appendChild(th);
    }
    const body = tbl.createTBody();
    for (const f of cell.files) {
      const tr = body.insertRow();
      const p = tr.insertCell();
      p.className = s.cdMono;
      p.append(icon("file", "compact"), document.createTextNode(f.path));
      const st = tr.insertCell();
      const b = span(f.checkText ?? copyText("plan.page.none"), s.cdFileState);
      b.dataset.state = f.stateCode === "ok" ? "ok" : f.stateCode === "empty" ? "warn" : "bad";
      st.appendChild(b);
      tr.insertCell().textContent = f.note ?? "";
      const also = tr.insertCell();
      if (f.alsoBy.length === 0) also.textContent = copyText("plan.page.none");
      else f.alsoBy.forEach((id, i) => {
        if (i > 0) also.appendChild(document.createTextNode(copyText("plan.page.listSep")));
        also.appendChild(link(labelOf(slice, id), () => this.host.select(id)));
      });
    }
    sec.appendChild(tbl);
    return sec;
  }

  /**
   * 状态：记着的值与笔数（后端写好的那一行，同 `pb render`）· 依据它的（「编号 标题」链接）·
   * 指着它、还没执行的 sets（虚线：来处 · 值（去一笔写 `-值`）· pb 说的为什么）。什么都没记 ⇒ 一句为什么没有。
   */
  private state(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const sec = section(copyText("plan.cell.state"));
    if (cell.stateLine) sec.appendChild(span(cell.stateLine, s.cdStateLine));
    if (cell.evidencedBy.length > 0) {
      const row = document.createElement("div");
      row.className = s.cdSetHead;
      row.appendChild(span(copyText("plan.state.evidencedBy"), s.cdMuted));
      cell.evidencedBy.forEach((id, i) => {
        if (i > 0) row.appendChild(document.createTextNode(copyText("plan.page.listSep")));
        row.appendChild(link(labelOf(slice, id), () => this.host.select(id)));
      });
      sec.appendChild(row);
    }
    const pending = slice.pendingSets.filter((p) => p.target === cell.id);
    for (const p of pending) {
      const row = document.createElement("div");
      row.className = s.cdSet;
      row.dataset.pending = "true";
      const h = document.createElement("div");
      h.className = s.cdSetHead;
      h.appendChild(span(copyText("plan.state.pending"), s.cdCounts));
      const src = p.source;
      if (src) h.appendChild(link(labelOf(slice, src), () => this.host.select(src)));
      if (p.value) h.appendChild(span(p.op === "remove" ? copyText("plan.state.removeValue", { value: p.value }) : p.value));
      row.appendChild(h);
      if (p.blockedReason) row.appendChild(span(p.blockedReason, s.cdSetWhy));
      sec.appendChild(row);
    }
    if (cell.quiet) sec.appendChild(span(cell.quiet, s.cdMuted));
    return sec;
  }

  private agentView(slice: PlanSlice, cell: PlanCell, rev: string): HTMLElement {
    const box = document.createElement("div");
    box.className = s.cdFold;
    const key = `${slice.name}\u0000${cell.id}\u0000${rev}`;
    const open = this.viewOpen === cell.id;
    const h = document.createElement("button");
    h.type = "button";
    h.className = `${s.cdFoldHead} plan-agent-view`;
    h.setAttribute("aria-expanded", String(open));
    h.append(foldCaret(), span(copyText("plan.cell.agentView")), span(copyText("plan.cell.agentViewHint"), s.cdFoldHint));
    box.appendChild(h);
    const fill = (): void => {
      const got = this.views.get(key);
      box.querySelector("pre, [data-view-none]")?.remove();
      if (!got) return;
      if ("text" in got) {
        const pre = document.createElement("pre");
        pre.className = s.cdPre;
        pre.textContent = got.text;
        box.appendChild(pre);
      } else {
        const none = span(got.none, s.cdMuted);
        none.dataset.viewNone = "";
        box.appendChild(none);
      }
    };
    h.addEventListener("click", () => {
      const now = this.viewOpen !== cell.id;
      this.viewOpen = now ? cell.id : null;
      h.setAttribute("aria-expanded", String(now));
      if (!now) {
        box.querySelector("pre, [data-view-none]")?.remove();
        return;
      }
      if (this.views.has(key)) return fill();
      fetchCellView(this.host.origin(), this.host.workspace(), slice.name, cell.id).then(
        (text) => {
          this.views.set(key, { text });
          if (this.viewOpen === cell.id) fill();
        },
        (e: unknown) => {
          this.views.set(key, { none: saidOf(e) });
          if (this.viewOpen === cell.id) fill();
        },
      );
    });
    if (open) fill();
    return box;
  }

  private idLine(cell: PlanCell): HTMLElement {
    const row = document.createElement("div");
    row.className = s.cdId;
    const id = span(cell.id, s.cdMono);
    const copy = button({
      label: copyText("plan.cell.copyId"),
      kind: "icon",
      icon: "copy",
      size: "compact",
      hint: copyText("plan.cell.copyId"),
      onClick: () => void writeClipboard(cell.id).then(() => toast(copyText("plan.cell.copied", { id: cell.id }), "", { level: "success" }), (e: unknown) => failToast(copyText("detail.act.failed"), e, { level: "error" })),
    });
    row.append(span(copyText("plan.cell.idLine")), id, copy);
    return row;
  }
}

/**
 * 交接说明那一块（划出的根节点没人接着 · 新登记的仓库还没人认领 project）：pb 给的那一句原样（码的位置写着 `<认领码>`，
 * 句末 pb 写了码找谁要）＋［复制］；给不了 ⇒ pb 说的为什么。复制的是那一句本身，交给另开的会话、认领码找上一级要。
 */
export function handoffBox(b: Pick<PlanBlock, "handoff" | "handoffError">): HTMLElement {
  const box = document.createElement("div");
  box.className = `${s.cdHandoff} plan-handoff`;
  box.appendChild(span(copyText("plan.handoff.title"), s.cdSecHead));
  const said = b.handoff;
  if (said === null) {
    box.appendChild(handoffNone(b));
    return box;
  }
  const row = document.createElement("div");
  row.className = s.cdHandoffRow;
  row.appendChild(span(said, s.cdMono));
  row.appendChild(
    button({
      label: copyText("plan.handoff.copy"),
      kind: "icon",
      icon: "copy",
      size: "compact",
      hint: copyText("plan.handoff.copy"),
      onClick: () => void writeClipboard(said).then(() => toast(copyText("plan.handoff.copied"), "", { level: "success" }), (e: unknown) => failToast(copyText("detail.act.failed"), e, { level: "error" })),
    }),
  );
  box.appendChild(row);
  return box;
}

/** 交接说明给不了：pb 说的为什么（交接说明那一块与概览那一格同一句）。 */
function handoffNone(b: Pick<PlanBlock, "handoffError">): HTMLElement {
  return span(copyText("plan.handoff.none", { reason: b.handoffError ?? "" }), s.cdMuted);
}

/** ［新会话接手…］那颗按钮。 */
function takeoverButton(open: () => void): HTMLButtonElement {
  return button({ label: copyText("plan.takeover.open"), icon: "plus", size: "compact", onClick: () => open() });
}

/**
 * 「接手」那一格（概览权力范围那一行末 · 节点详情交接说明底下）：照后端那一格画，界面不看负责人自己判。
 * `open` ⇒［新会话接手…］；`no_line` ⇒ pb 说的为什么；其余（负责人在跑 · 说不清 · 要并回）⇒ 空。
 */
export function takeoverCell(b: Pick<PlanBlock, "takeover" | "handoffError">, open: () => void): HTMLElement {
  const box = document.createElement("span");
  if (b.takeover === "open") box.appendChild(takeoverButton(open));
  else if (b.takeover === "no_line") box.appendChild(handoffNone(b));
  return box;
}

/** 还没建的权力范围（`unbuiltBlocks` 那一列本身就是这一判）：灰着的按钮，悬停说为什么。 */
export function unbuiltTakeoverCell(): HTMLElement {
  const box = document.createElement("span");
  const x = takeoverButton(() => {});
  setDisabled(x, copyText("plan.takeover.unbuilt"));
  box.appendChild(x);
  return box;
}

/** 接手那一下开的框：机器锁那台，目录锁在工作区根，第一句预填 pb 给的交接说明（拿不到 ⇒ 不预填）。 */
export function takeoverSpec(origin: Origin, workspace: string, b: Pick<PlanBlock, "handoff">): NewSessionSpec {
  const spec: NewSessionSpec = { origin, lockMachine: true, cwd: workspace, lockCwd: true };
  if (b.handoff !== null) spec.prompt = b.handoff;
  return spec;
}

/** 出边组名：with / to 用这一类的词（「它实现」「它指向」）；after / replaces 是固定的说法。 */
function outLabel(e: PlanEdgeKind, word: string | null): string {
  if (e === "after") return copyText("plan.edge.afterOut");
  if (e === "replaces") return copyText("plan.edge.replacesOut");
  if (word === null) return e === "with" ? copyText("plan.edge.withOut") : copyText("plan.edge.toOut");
  return copyText("plan.edge.out", { word });
}

/** 指着它的组名：with / to 用对方那一类的词（「实现它的」「测它的」）。 */
function inLabel(e: PlanEdgeKind, word: string | null): string {
  if (e === "after") return copyText("plan.edge.afterIn");
  if (e === "replaces") return copyText("plan.edge.replacesIn");
  if (word === null) return e === "with" ? copyText("plan.edge.withIn") : copyText("plan.edge.toIn");
  return copyText("plan.edge.in", { word });
}

function kindChip(slice: PlanSlice, kind: string): HTMLElement {
  const k = span(kind, s.cdKind);
  setKindColor(k, kindSlot(slice, kind));
  return k;
}

function span(t: string, cls?: string): HTMLElement {
  const e = document.createElement("span");
  if (cls) e.className = cls;
  e.textContent = t;
  return e;
}

function link(t: string, go: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = s.cdLink;
  b.textContent = t;
  b.addEventListener("click", go);
  return b;
}

function section(title: string): HTMLElement {
  const sec = document.createElement("div");
  sec.className = s.cdSec;
  const h = document.createElement("div");
  h.className = s.cdSecHead;
  h.appendChild(document.createTextNode(title));
  sec.appendChild(h);
  return sec;
}
