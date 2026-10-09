/**
 * **一格的详情**（设计稿 planned-build 02）：计划页右栏选了格之后的样子。
 *
 * 从上到下：在哪（标题串）→ 头（状态 · 标题 · 类 · 状态徽标 · 原因）→ 位置（接手 · 阶段 ·［切到会话］）→ 条（不做了）→
 * 要做成什么样（md；话里提到的格照 pb 给的位置换成标题链接）→ 里面 → 连着的 · 连着它的（按这一类的人话分组）→
 * 管的文件 → 收下 → agent 站在这一格看到的（折着，展开才问）→ 跟 agent 说时用的编号。
 *
 * 编号只在最底下那一行与 agent 看到的那一段里出现；别处一律写标题。只排版：成品都是后端给的。
 */
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { foldCaret } from "../kit/fold";
import { toast, failToast } from "../kit/toast";
import { copyText } from "../copy-table";
import { writeClipboard } from "../clipboard";
import { renderMarkdown } from "../render";
import type { Origin } from "../ipc/origin";
import { fetchCellView, PlanMiss, PLAN_EDGE_KINDS, type PlanCell, type PlanEdgeKind, type PlanRef, type PlanSlice, type PlanWho } from "../plan-reads";
import { ancestry, blockRoots, cellIndex, kindSlot, statusLook } from "./plan-model";
import { phaseBadge, setKindColor, STATUS_ICON } from "./plan-bits";
import s from "./plan-cell.module.css";

export interface CellDetailHost {
  select(id: string | null): void;
  who(w: PlanWho, signer?: boolean): HTMLElement;
  origin(): Origin;
  workspace(): string;
  switchTo(sid: string): void;
}

/** 正文里占位的两头（私用区的两个字，渲 md 时原样过）。 */
const REF_OPEN = String.fromCharCode(0xe000);
const REF_CLOSE = String.fromCharCode(0xe001);
const REF_SPLIT = new RegExp(`${REF_OPEN}(\\d+)${REF_CLOSE}`);

/** 正文多于这么多行 ⇒ 先折起。 */
const FOLD_LINES = 14;

export class CellDetail {
  /** 问过的 agent 视角：`片 \u0000 格 \u0000 rev` ⇒ 那一段 / 没有。同一格再开不重问（rev 变了才重问）。 */
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
    const bar = this.droppedBar(slice, cell);
    if (bar) box.appendChild(bar);
    if (cell.body) box.appendChild(this.design(slice, cell));
    if (cell.children.length > 0) box.appendChild(this.inside(slice, cell));
    const edges = this.edges(slice, cell);
    if (edges) box.appendChild(edges);
    if (cell.files.length > 0) box.appendChild(this.files(slice, cell));
    box.appendChild(this.signs(slice, cell));
    if (cell.hasView) box.appendChild(this.agentView(slice, cell, rev));
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
      c.append(icon("caretRight", "compact"), link(a.title ?? a.id, () => this.host.select(a.id)));
    }
    return c;
  }

  private head(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const h = document.createElement("div");
    h.className = s.cdHead;
    const look = statusLook(cell);
    const st = document.createElement("span");
    st.className = s.cdSt;
    st.dataset.look = look;
    st.appendChild(icon(STATUS_ICON[look]));
    const t = document.createElement("span");
    t.className = s.cdTitle;
    t.dataset.look = look;
    t.append(...this.refText(slice, cell.title ?? cell.id, cell.refs.title));
    h.append(st, t);
    if (cell.kind) h.appendChild(kindChip(slice, cell.kind));
    const badge = document.createElement("span");
    badge.className = s.cdBadge;
    badge.dataset.look = look;
    badge.textContent = cell.status ?? "";
    h.appendChild(badge);
    if (look === "open" && cell.why) h.appendChild(span(cell.why, s.cdWhy));
    return h;
  }

  /** 位置行：块根格 ⇒ 那一块的接手 ＋ 阶段；别的格 ⇒ 在长它的。都带［切到会话］。 */
  private position(slice: PlanSlice, cell: PlanCell): HTMLElement | null {
    const b = blockRoots(slice).get(cell.id) ?? null;
    const who = b?.owner ?? cell.owner;
    if (!who && !b) return null;
    const row = document.createElement("div");
    row.className = s.cdPos;
    if (b) row.appendChild(span(copyText("plan.cell.blockRoot", { block: cell.title ?? cell.id })));
    else row.appendChild(span(copyText("plan.cell.grower")));
    if (who) row.appendChild(this.host.who(who));
    if (b?.phase) row.appendChild(phaseBadge(b.phase));
    const sid = who?.sid ?? null;
    if (sid) row.appendChild(link(copyText("plan.cell.toSession"), () => this.host.switchTo(sid)));
    return row;
  }

  private droppedBar(slice: PlanSlice, cell: PlanCell): HTMLElement | null {
    if (statusLook(cell) !== "dropped") return null;
    const by = cell.pointedBy.replaces[0];
    const bar = document.createElement("div");
    bar.className = s.cdBar;
    if (by) {
      const t = cellIndex(slice).get(by);
      bar.appendChild(span([copyText("plan.bar.droppedBare"), copyText("plan.bar.dropped", { title: t?.title ?? by })].join(copyText("kit.text.sep"))));
    } else bar.appendChild(span(copyText("plan.bar.droppedBare")));
    return bar;
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
    const chars = Array.from(body);
    const sorted = [...refs].filter((r) => r.start >= 0 && r.end <= chars.length && r.start < r.end).sort((a, b) => a.start - b.start);
    let md = "";
    let at = 0;
    sorted.forEach((r, i) => {
      if (r.start < at) return;
      md += chars.slice(at, r.start).join("") + `${REF_OPEN}${i}${REF_CLOSE}`;
      at = r.end;
    });
    md += chars.slice(at).join("");
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

  /** 一段话（标题 / 签收理由）照位置换链接。 */
  refText(slice: PlanSlice, text: string, refs: readonly PlanRef[]): Node[] {
    const chars = Array.from(text);
    const out: Node[] = [];
    let at = 0;
    for (const r of [...refs].sort((a, b) => a.start - b.start)) {
      if (r.start < at || r.end > chars.length || r.start >= r.end) continue;
      if (r.start > at) out.push(document.createTextNode(chars.slice(at, r.start).join("")));
      out.push(...this.refLinks(slice, r));
      at = r.end;
    }
    if (at < chars.length) out.push(document.createTextNode(chars.slice(at).join("")));
    return out;
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
      a.textContent = c?.title ?? id;
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

  /** 一行：状态图标 · 标题（链接）· 类的词 · 没做完时原因。 */
  private cellLine(slice: PlanSlice, c: PlanCell): HTMLElement {
    const row = document.createElement("div");
    row.className = s.cdLine;
    setKindColor(row, kindSlot(slice, c.kind));
    const look = statusLook(c);
    const st = document.createElement("span");
    st.className = s.cdSt;
    st.dataset.look = look;
    st.appendChild(icon(STATUS_ICON[look], "compact"));
    row.append(st, link(c.title ?? c.id, () => this.host.select(c.id)));
    if (c.kind) row.appendChild(span(c.kind, s.cdKw));
    if (look === "open" && c.why) row.appendChild(span(c.why, s.cdMuted));
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

  // ── 文件 · 收下 · agent 看到的 · 编号 ──

  private files(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const sec = section(copyText("plan.cell.files"));
    const byId = cellIndex(slice);
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
      const b = span(f.state ?? "", s.cdFileState);
      b.dataset.state = f.stateCode === "ok" ? "ok" : f.stateCode === "empty" ? "warn" : "bad";
      st.appendChild(b);
      tr.insertCell().textContent = f.note ?? "";
      const also = tr.insertCell();
      if (f.alsoBy.length === 0) also.textContent = copyText("plan.page.none");
      else f.alsoBy.forEach((id, i) => {
        if (i > 0) also.appendChild(document.createTextNode(copyText("plan.page.listSep")));
        also.appendChild(link(byId.get(id)?.title ?? id, () => this.host.select(id)));
      });
    }
    sec.appendChild(tbl);
    return sec;
  }

  private signs(slice: PlanSlice, cell: PlanCell): HTMLElement {
    const n = cell.signs.length;
    const sec = section(copyText("plan.cell.signs"), n > 1 ? copyText("plan.sign.count", { n }) : undefined);
    if (n === 0) {
      const line = cell.children.length > 0 ? copyText("plan.sign.inner") : (cell.why ?? copyText("plan.sign.none"));
      sec.appendChild(span(line, s.cdMuted));
      return sec;
    }
    [...cell.signs].reverse().forEach((g, i) => {
      const row = document.createElement("div");
      row.className = s.cdSign;
      if (i > 0) row.dataset.old = "true";
      const h = document.createElement("div");
      h.className = s.cdSignHead;
      if (i === 0) h.appendChild(span(copyText("plan.sign.counts"), s.cdCounts));
      h.appendChild(span(g.atText ?? ""));
      if (g.by) h.appendChild(this.host.who(g.by, true));
      const r = document.createElement("div");
      r.className = s.cdSignWhy;
      r.append(...this.refText(slice, g.reason ?? "", g.refs));
      row.append(h, r);
      sec.appendChild(row);
    });
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
          this.views.set(key, { none: e instanceof PlanMiss ? e.said : String(e) });
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
      onClick: () => void writeClipboard(cell.id).then(() => toast(copyText("plan.cell.copied", { id: cell.id }), ""), (e: unknown) => failToast(copyText("detail.act.failed"), e, { level: "error" })),
    });
    row.append(span(copyText("plan.cell.idLine")), id, copy);
    return row;
  }
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

function section(title: string, aside?: string): HTMLElement {
  const sec = document.createElement("div");
  sec.className = s.cdSec;
  const h = document.createElement("div");
  h.className = s.cdSecHead;
  h.appendChild(document.createTextNode(title));
  if (aside) h.appendChild(span(aside, s.cdAside));
  sec.appendChild(h);
  return sec;
}
