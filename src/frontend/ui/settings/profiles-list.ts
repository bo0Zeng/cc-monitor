/**
 * 「别名」那一行里的清单：配置文件（`~/.cc-monitor/profiles.toml`）按「基于」排成一棵树。
 *
 * - 每行只写这一条自己那几项（后端写好的 `said`），继承来的不重复写；点一行出合并表（项 · 值 · 来自哪一段，被盖掉的划线）
 *   与这台后端算的「等于」那一行，「假设在这个目录敲」可改。
 * - 新建 / 改：先选「基于」谁（下拉只列不成圈的，后端给），每一格先显示继承值，点「改」才写进这一条；「等于」按未存的表单问后端。
 *   改一条被别人基于的：存之前列出会连带谁（后端算的改前改后）。删一条被别人基于的：三个选择，推荐改成基于它的父。
 * - 手改过 · 写错 · 保存冲突 · 终端函数 · 第一次升级 · 空态：照后端读回的那一份画，原话一律后端给。
 *
 * 本文件一个 ccm 选项都不认、不合并：五问见 `../profiles-reads`。构造零 I/O：第一次 `load()` 才发第一条。
 */
import { chan, type Sub } from "../../../comms/inward/chan";
import { openPath } from "@tauri-apps/plugin-opener";
import { copyText } from "../copy-table";
import { writeClipboard } from "../clipboard";
import { homeShort } from "../kit/path";
import { select as kitSelect } from "../kit/select";
import { toast, undoToast, failToast } from "../kit/toast";
import { confirmDialog, type ConfirmFn } from "../kit/dialog";
import { sayWithDetail } from "../kit/detail";
import type { Origin } from "../generated/Origin";
import type { ClashWins } from "../alias-reads";
import { changedItems, changedStream } from "../changed-stream";
import {
  ProfilesStale,
  profileBases,
  profileImpact,
  readProfiles,
  resolveProfile,
  writeProfiles,
  type Affected,
  type ProfileForm,
  type ProfileOp,
  type ProfileRow,
  type ProfilesBook,
  type Resolved,
  type SlotId,
} from "../profiles-reads";
import { sayFailure } from "../kit/detail";

/** 启动文件里块外与清单同名的函数（接入那一格读到的）。 */
export interface ProfileClash {
  name: string;
  path: string;
  line: number;
  wins: ClashWins;
}

export interface ProfilesListSpec {
  origin: () => Origin;
  /** 本机才有「打开配置文件」（系统默认编辑器）。 */
  local: boolean;
  /** 清单读回之后，头部那一行（`N 条 · 规则住 … · 改了下次起会话就生效`）。 */
  onHead: (text: string) => void;
  confirm?: ConfirmFn;
}

export interface ProfilesList {
  element: HTMLElement;
  load(): Promise<void>;
  /** 接入那一格读到的同名函数（每次读回之后给一次）。 */
  setClashes(c: readonly ProfileClash[]): void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function button(label: string, cls: string, onClick: () => void): HTMLButtonElement {
  const b = el("button", cls, label);
  b.type = "button";
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    onClick();
  });
  return b;
}

const link = (label: string, onClick: () => void, danger = false): HTMLButtonElement =>
  button(label, danger ? "cfg-link cfg-link-danger" : "cfg-link", onClick);

/** 窄窗（合并表落到那一行下面）的分界。 */
const WIDE = "(min-width: 900px)";

/** 树里一行：那一段 ＋ 前面的缩进与竖线。 */
interface TreeLine {
  p: ProfileRow;
  ind: string;
}

/** 按「基于」排成树（父在前、子缩进；同一层按文件里的顺序）。「基于」的不在 / 绕成圈的那几段当根放在后面。 */
export function treeOf(rows: readonly ProfileRow[]): TreeLine[] {
  const names = new Set(rows.map((r) => r.name));
  const kids = (n: string): ProfileRow[] => rows.filter((r) => r.from === n);
  const out: TreeLine[] = [];
  const seen = new Set<string>();
  const walk = (p: ProfileRow, lead: string, last: boolean, depth: number): void => {
    if (seen.has(p.name)) return;
    seen.add(p.name);
    out.push({ p, ind: depth === 0 ? "" : `${lead}${last ? "└ " : "├ "}` });
    const ks = kids(p.name);
    ks.forEach((k, i) => walk(k, depth === 0 ? "" : `${lead}${last ? "  " : "│ "}`, i === ks.length - 1, depth + 1));
  };
  for (const r of rows) if (r.from === null || !names.has(r.from)) walk(r, "", true, 0);
  for (const r of rows) if (!seen.has(r.name)) walk(r, "", true, 0);
  return out;
}

/** 表单开着时的样子。 */
interface FormState {
  /** 正在改的那一段原来的名字（新增 ⇒ `null`）。 */
  was: string | null;
  form: ProfileForm;
  /** 一开始的样子（Esc 时比较有没有改过）。 */
  initial: string;
  el: HTMLElement;
  /** 表单顶那一句（这一段写错的原话 · 保存冲突）。 */
  top: string | null;
  stale: boolean;
}

const emptyForm = (name = "", from: string | null = null): ProfileForm => ({
  name,
  from,
  account: null,
  tmux: null,
  cwdIf: null,
  cwd: null,
  agent: null,
  args: null,
  launcher: null,
  tmuxSize: null,
  detach: false,
  busRegister: false,
  busNote: null,
});

/** 一格写了没有（开关写 `true` 才算写了）。 */
function own(f: ProfileForm, s: SlotId): boolean {
  const v = f[s];
  return typeof v === "boolean" ? v : v !== null;
}

/** 回到继承：把这一格清掉。 */
function clearSlot(f: ProfileForm, s: SlotId): ProfileForm {
  const n = { ...f };
  if (s === "detach" || s === "busRegister") n[s] = false;
  else n[s] = null;
  return n;
}

/** 第一次「改」那一格时的起始值。 */
function startSlot(f: ProfileForm, s: SlotId, accounts: readonly string[]): ProfileForm {
  const n = { ...f };
  switch (s) {
    case "account":
      n.account = accounts.length ? { kind: "account", name: accounts[0] } : { kind: "base" };
      break;
    case "tmux":
      n.tmux = { mode: "auto", name: "" };
      break;
    case "cwdIf":
      n.cwdIf = [{ at: "", to: "" }];
      break;
    case "detach":
    case "busRegister":
      n[s] = true;
      break;
    default:
      n[s] = "";
  }
  return n;
}

const SLOT_LABEL: Record<SlotId, () => string> = {
  account: () => copyText("beProfile.slot.account"),
  tmux: () => copyText("beProfile.slot.tmux"),
  cwdIf: () => copyText("beProfile.slot.cwdIf"),
  cwd: () => copyText("beProfile.slot.cwd"),
  agent: () => copyText("beProfile.slot.agent"),
  args: () => copyText("beProfile.slot.args"),
  launcher: () => copyText("beProfile.slot.launcher"),
  tmuxSize: () => copyText("beProfile.slot.tmuxSize"),
  detach: () => copyText("beProfile.slot.detach"),
  busRegister: () => copyText("beProfile.slot.busRegister"),
  busNote: () => copyText("beProfile.slot.busNote"),
};

/**
 * 「在哪起」那三种 tmux 取名**撞名时会怎样**（下拉的选项只有名字，没有一句说撞了会怎样）。
 *
 * 规则不在这里：取名与退让住后端 `control/ccm/plan.rs::build`。`stepsAside` 与后端逐条对拍
 * （`tests/frontend/ui/settings/machine-aliases-naming.vitest.ts` 读后端原文，两向相等），说明里点出退让的后缀（-2）⇔ 它为真。
 */
// ⚠ `text` 是取文函数：模块顶层调 `copyText` 会让打包器把本模块挪进主窗口也要的共享块。
export const TMUX_NAMING: Record<"auto" | "fixed" | "base", { stepsAside: boolean; text: () => string }> = {
  auto: { stepsAside: true, text: () => copyText("machineAliases.tmuxNaming.auto") },
  fixed: { stepsAside: false, text: () => copyText("machineAliases.tmuxNaming.named") },
  base: { stepsAside: true, text: () => copyText("machineAliases.tmuxNaming.base") },
};

/** 表单上直接露出来的几格；其余收在「更多」里。 */
const MAIN_SLOTS: readonly SlotId[] = ["account", "tmux", "cwdIf", "cwd"];
const MORE_SLOTS: readonly SlotId[] = ["agent", "args", "launcher", "tmuxSize", "detach", "busRegister", "busNote"];

export function buildProfilesList(opts: ProfilesListSpec): ProfilesList {
  const root = el("div", "prof");
  root.dataset.role = "profiles";
  const notes = el("div", "prof-notes");
  const head = el("div", "cfg-list-head prof-head");
  const headL = el("div", "prof-head-l");
  headL.append(el("span", "cfg-list-title", copyText("profilesPage.list.title")), el("span", "cfg-hint", copyText("profilesPage.list.hint")));
  const headR = el("div", "prof-head-r");
  const openBtn = link(copyText("profilesPage.list.open"), () => void onOpenFile());
  const addBtn = button(copyText("profilesPage.list.add"), "settings-btn", () => void openForm(null));
  headR.append(...(opts.local ? [openBtn] : []), addBtn);
  head.append(headL, headR);
  const split = el("div", "prof-split");
  const left = el("div", "prof-left");
  const right = el("div", "prof-right");
  split.append(left, right);
  const status = el("div", "cfg-hint prof-status");
  root.append(notes, head, status, split);

  let book: ProfilesBook | null = null;
  let selected: string | null = null;
  let at = "~";
  let resolved: Resolved | null = null;
  let form: FormState | null = null;
  /** 删了、还在 8 秒撤销期里的那几条（清单里先不画；到点才真交 remove）。 */
  const pendingRemove = new Set<string>();
  let removing: { name: string; kids: string[]; pick: "reparent" | "cascade"; impact: Affected[] | null; open: boolean } | null = null;
  let impact: { names: string[]; rows: Affected[]; open: boolean } | null = null;
  /** 「上次 cc-monitor 写过之后有人改过」那一格（后端判、后端写好时刻）。开着表单时来的那一份不换清单，只换这一格。 */
  let editedAt: string | null = null;
  /** 开着表单 / 删的选择时盘上变了：关掉之后重读。 */
  let behind = false;
  let clashes: readonly ProfileClash[] = [];
  let sub: Sub | null = null;
  let seq = 0;

  const short = (p: string): string => homeShort(p, book?.home ?? null);
  const find = (n: string): ProfileRow | undefined => book?.profiles.find((p) => p.name === n);
  const kidsOf = (n: string): string[] => (book?.profiles ?? []).filter((p) => p.from === n).map((p) => p.name);

  // ── 读 ──

  const reread = async (): Promise<void> => {
    try {
      book = await readProfiles(opts.origin());
      status.textContent = "";
    } catch (e) {
      sayFailure(status, copyText("profilesPage.read.failed"), e);
      return render();
    }
    editedAt = book.editedAt;
    behind = false;
    opts.onHead(
      book.exists
        ? copyText("profilesPage.head.line", { n: String(book.profiles.length), path: short(book.path) })
        : copyText("profilesPage.head.none"),
    );
    if (selected && !find(selected)) selected = null;
    render();
    if (selected) void askResolve();
  };

  const subscribe = async (): Promise<void> => {
    if (sub) return;
    // 订上那一刻的第一格 `seen` 不算变（读回那一下刚做过）；之后的 `seen`（断了又接上）· 帧 · `gap` 都算。
    let first = true;
    sub = await chan.subscribe(opts.origin(), changedStream("profiles"), null, 8, (items) => {
      const fresh = first ? items.filter((it, i) => !(i === 0 && it.t === "seen")) : items;
      first = false;
      const { cells, all, frames } = changedItems(fresh);
      if (frames > 0) sub?.want(frames);
      if (!all && cells.length === 0) return;
      // 开着表单 / 删的选择时不换清单（不拽人）：只问一次后端那一格、亮那一句；关掉之后重读。
      if (form || removing) {
        behind = true;
        void readProfiles(opts.origin())
          .then((b) => {
            editedAt = b.editedAt;
            renderNotes();
          })
          .catch(() => undefined);
      } else void reread();
    });
  };

  // ── 合并表 ──

  const askResolve = async (): Promise<void> => {
    if (!selected) return;
    const mine = ++seq;
    try {
      const r = await resolveProfile(opts.origin(), selected, null, at.trim() || null);
      if (mine === seq) {
        resolved = r;
        renderPanel();
      }
    } catch (e) {
      if (mine === seq) {
        resolved = { chain: [], rows: [], line: null, lineError: null, problem: e instanceof Error ? e.message : String(e) };
        renderPanel();
      }
    }
  };

  const select = (n: string | null): void => {
    selected = selected === n ? null : n;
    resolved = null;
    render();
    if (selected) void askResolve();
  };

  const panel = (): HTMLElement => {
    const box = el("div", "prof-box");
    box.dataset.role = "merge";
    const p = selected ? find(selected) : undefined;
    if (!p) return box;
    const chain = resolved?.chain ?? [];
    box.appendChild(
      el("div", "prof-box-title", chain.length ? copyText("profilesPage.merge.title", { name: p.name, chain: chain.join(copyText("beProfile.chain.arrow")) }) : copyText("profilesPage.merge.titleBare", { name: p.name })),
    );
    if (!resolved) {
      box.appendChild(el("div", "cfg-hint", copyText("profilesPage.merge.asking")));
      return box;
    }
    if (resolved.problem) {
      box.appendChild(el("div", "prof-err", resolved.problem));
    } else {
      const tab = el("table", "prof-mtab");
      const hr = el("tr", "");
      hr.append(el("th", "", copyText("profilesPage.merge.colItem")), el("th", "", copyText("profilesPage.merge.colValue")), el("th", "", copyText("profilesPage.merge.colFrom")));
      tab.appendChild(hr);
      for (const r of resolved.rows) {
        const tr = el("tr", r.overriddenBy ? "over" : "");
        const k = el("td", "k");
        k.append(el("span", "", r.label), el("small", "prof-key", r.key));
        const v = el("td", "v", r.said);
        if (r.overriddenBy) v.title = copyText("profilesPage.merge.overridden", { by: r.overriddenBy });
        const f = el("td", "");
        f.appendChild(el("span", r.from === p.name ? "prof-layer me" : "prof-layer", r.from));
        tr.append(k, v, f);
        tab.appendChild(tr);
      }
      box.appendChild(tab);
    }
    const atRow = el("label", "prof-at");
    const atIn = el("input", "settings-input prof-at-in");
    atIn.type = "text";
    atIn.value = at;
    atIn.dataset.role = "at";
    atIn.addEventListener("change", () => {
      at = atIn.value;
      void askResolve();
    });
    atRow.append(el("span", "", copyText("profilesPage.merge.at")), atIn);
    box.appendChild(atRow);
    box.appendChild(el("div", "cfg-hint", copyText("profilesPage.merge.equals", { name: p.name })));
    const line = el("pre", "prof-line", resolved.line ?? resolved.lineError ?? resolved.problem ?? "");
    line.dataset.role = "equals";
    if (!resolved.line) line.classList.add("prof-line-bad");
    box.appendChild(line);
    box.appendChild(el("div", "cfg-hint", copyText("profilesPage.merge.hint", { name: p.name })));
    if (p.kind === "function") {
      const fn = el("div", "prof-fn");
      fn.dataset.role = "function-note";
      fn.append(el("div", "prof-fn-title", copyText("profilesPage.function.title", { name: p.name })), el("div", "cfg-hint", [p.functionWhy, p.functionLine].filter(Boolean).join(copyText("profilesPage.list.sep"))));
      box.appendChild(fn);
    }
    return box;
  };

  const renderPanel = (): void => {
    const old = root.querySelector("[data-role=merge]");
    const fresh = panel();
    if (old) old.replaceWith(fresh);
    else render();
  };

  // ── 树 ──

  const hitNames = (): Set<string> => {
    if (removing) return new Set(removing.kids);
    return new Set(impact?.names ?? []);
  };

  const treeRow = (t: TreeLine, ix: number, lines: TreeLine[]): HTMLElement => {
    const { p } = t;
    const r = el("div", "prof-trow");
    r.dataset.name = p.name;
    r.tabIndex = selected === p.name || (selected === null && ix === 0) ? 0 : -1;
    if (selected === p.name || form?.was === p.name || removing?.name === p.name) r.classList.add("sel");
    if (hitNames().has(p.name)) r.classList.add("hit");
    const tn = el("span", "prof-tn");
    if (t.ind) tn.appendChild(el("span", "prof-ind", t.ind));
    tn.appendChild(document.createTextNode(p.name));
    if (p.kind === "function") {
      const c = el("span", "prof-chip fn", copyText("profilesPage.chip.function"));
      c.title = p.functionWhy ?? "";
      tn.appendChild(c);
    }
    if (!p.usable) tn.appendChild(el("span", "prof-chip bad", copyText("profilesPage.chip.unusable")));
    const ts = el("span", p.usable ? "prof-ts" : "prof-ts prof-ts-bad", p.usable ? p.said : (p.problem?.message ?? ""));
    const ta = el("span", "prof-ta");
    ta.append(
      link(copyText("profilesPage.row.edit"), () => void openForm(p.name)),
      link(copyText("profilesPage.row.delete"), () => void startRemove(p.name), true),
    );
    r.append(tn, ts, ta);
    r.addEventListener("click", () => select(p.name));
    r.addEventListener("keydown", (ev) => {
      if (ev.isComposing || form) return;
      const at = lines.findIndex((l) => l.p.name === p.name);
      const go = (i: number): void => {
        const n = lines[i]?.p.name;
        if (!n) return;
        selected = n;
        resolved = null;
        render();
        void askResolve();
        (root.querySelector(`.prof-trow[data-name="${CSS.escape(n)}"]`) as HTMLElement | null)?.focus();
      };
      if (ev.key === "ArrowDown") go(at + 1);
      else if (ev.key === "ArrowUp") go(at - 1);
      else if (ev.key === "Enter") select(p.name);
      else if (ev.key === "e") void openForm(p.name);
      else if (ev.key === "Delete") void startRemove(p.name);
      else if (ev.key === "Escape" && selected) select(selected);
      else return;
      ev.preventDefault();
    });
    return r;
  };

  const render = (): void => {
    left.replaceChildren();
    right.replaceChildren();
    renderNotes();
    // 列表头只在有清单时挂上（它是 flex 盒，`hidden` 盖不住）。
    if (book?.exists && !book.fileProblem) {
      if (!head.isConnected) notes.after(head);
    } else head.remove();
    if (!book) return;
    if (book.fileProblem) {
      const card = el("div", "prof-card prof-card-bad");
      card.dataset.role = "file-problem";
      const fp = book.fileProblem;
      const said = fp.line !== null ? copyText("profilesPage.file.at", { line: String(fp.line), why: fp.message }) : fp.message;
      const err = el("div", "prof-err", said);
      if (fp.detail !== null) sayWithDetail(err, said, fp.detail);
      card.append(el("div", "prof-card-title", copyText("profilesPage.file.broken")), err);
      if (opts.local) card.appendChild(button(copyText("profilesPage.list.open"), "settings-btn", () => void onOpenFile()));
      left.appendChild(card);
      return;
    }
    if (!book.exists) {
      left.appendChild(emptyState(book));
      return;
    }
    const tree = el("div", "prof-tree");
    tree.dataset.role = "tree";
    const lines = treeOf(book.profiles.filter((p) => !pendingRemove.has(p.name)));
    const wide = window.matchMedia?.(WIDE).matches ?? true;
    lines.forEach((t, i) => {
      tree.appendChild(treeRow(t, i, lines));
      if (form?.was === t.p.name) tree.appendChild(form.el);
      if (!wide && selected === t.p.name && !form) tree.appendChild(panel());
    });
    if (!lines.length) tree.appendChild(el("div", "cfg-hint prof-empty", copyText("profilesPage.list.empty")));
    if (form && form.was === null) tree.appendChild(form.el);
    left.appendChild(tree);
    split.classList.toggle("prof-split-on", wide && selected !== null && !form);
    if (wide && selected && !form) right.appendChild(panel());
  };

  const emptyState = (b: ProfilesBook): HTMLElement => {
    const box = el("div", "prof-empty-state");
    box.dataset.role = "seed";
    box.appendChild(el("div", "cfg-hint", copyText("profilesPage.seed.hint")));
    const tree = el("div", "prof-tree");
    for (const t of treeOf(b.seed)) {
      const r = el("div", "prof-trow");
      const tn = el("span", "prof-tn");
      if (t.ind) tn.appendChild(el("span", "prof-ind", t.ind));
      tn.appendChild(document.createTextNode(t.p.name));
      if (t.p.kind === "function") tn.appendChild(el("span", "prof-chip fn", copyText("profilesPage.chip.function")));
      r.append(tn, el("span", "prof-ts", t.p.said), el("span", "prof-ta"));
      tree.appendChild(r);
    }
    const acts = el("div", "cfg-acts");
    acts.append(
      button(copyText("profilesPage.seed.create"), "settings-btn settings-btn-primary", () => void init(true)),
      button(copyText("profilesPage.seed.blank"), "settings-btn", () => void init(false)),
      el("span", "cfg-hint", copyText("profilesPage.seed.where", { path: short(b.path) })),
    );
    box.append(tree, acts);
    return box;
  };

  // ── 页首那几句 ──

  const renderNotes = (): void => {
    notes.replaceChildren();
    if (!book) return;
    if (book.migrated) {
      const m = book.migrated;
      const n = el("div", "cfg-note prof-note-ok");
      n.dataset.role = "migrated";
      n.appendChild(el("span", "", copyText("profilesPage.migrated.line", { n: String(m.count), path: short(m.path) })));
      if (opts.local) n.appendChild(link(copyText("profilesPage.edited.look"), () => void onOpenFile()));
      n.appendChild(link(copyText("profilesPage.migrated.ack"), () => void ackMigrated()));
      for (const s of m.skipped) n.appendChild(el("div", "prof-sub", s));
      notes.appendChild(n);
    }
    if (clashes.length) notes.appendChild(clashCard());
    if (editedAt !== null) {
      const n = el("div", "cfg-note");
      n.dataset.role = "edited-elsewhere";
      n.appendChild(el("span", "", form || removing ? copyText("profilesPage.edited.lineOpen", { time: editedAt }) : copyText("profilesPage.edited.line", { time: editedAt })));
      if (opts.local) n.appendChild(link(copyText("profilesPage.edited.look"), () => void onOpenFile()));
      notes.appendChild(n);
    }
    if (impact && impact.names.length && form) notes.appendChild(impactNote(form.form.name || form.was || "", impact));
    if (removing) notes.appendChild(removeChoices());
  };

  const impactTable = (rows: readonly Affected[]): HTMLElement => {
    const tab = el("table", "prof-mtab");
    tab.dataset.role = "impact";
    const hr = el("tr", "");
    hr.append(el("th", "", copyText("profilesPage.impact.colName")), el("th", "", copyText("profilesPage.impact.colBefore")), el("th", "", copyText("profilesPage.impact.colAfter")));
    tab.appendChild(hr);
    for (const a of rows) {
      if (a.problem) {
        const tr = el("tr", "");
        const td = el("td", "v prof-err", a.problem);
        td.colSpan = 2;
        tr.append(el("td", "k", a.name), td);
        tab.appendChild(tr);
        continue;
      }
      for (const c of a.changes) {
        const tr = el("tr", "");
        tr.append(el("td", "k", a.name), el("td", "v", c.before || copyText("profilesPage.impact.none")), el("td", "v", c.after || copyText("profilesPage.impact.none")));
        tab.appendChild(tr);
      }
    }
    return tab;
  };

  const impactNote = (name: string, i: { names: string[]; rows: Affected[]; open: boolean }): HTMLElement => {
    const box = el("div", "");
    const n = el("div", "cfg-note cfg-note-warn");
    n.dataset.role = "impact-note";
    n.append(
      el("span", "", copyText("profilesPage.impact.line", { name, n: String(i.names.length), names: i.names.join(copyText("profilesPage.list.sep")) })),
      link((i.open ? copyText("profilesPage.impact.hide") : copyText("profilesPage.impact.show")), () => {
        i.open = !i.open;
        renderNotes();
      }),
    );
    box.appendChild(n);
    if (i.open) box.appendChild(impactTable(i.rows));
    return box;
  };

  const clashCard = (): HTMLElement => {
    const card = el("div", "prof-card prof-card-warn");
    card.dataset.role = "clash";
    // 「待办」里同名那一件［去定…］带 `clash` 锚点跳到这里。
    card.dataset.anchor = "clash";
    const names = [...new Set(clashes.map((c) => c.name))];
    const paths = [...new Set(clashes.map((c) => c.path))];
    const lines = clashes.map((c) => String(c.line)).join(copyText("profilesPage.list.sep"));
    const wins = new Set(clashes.map((c) => c.wins));
    const now =
      wins.size > 1
        ? copyText("machineAliases.clash.mixed")
        : clashes[0].wins === "yours"
          ? copyText("machineAliases.clash.nowYours")
          : clashes[0].wins === "list"
            ? copyText("machineAliases.clash.nowList")
            : copyText("machineAliases.clash.nowUnclear");
    card.appendChild(
      el("div", "prof-card-title", copyText("profilesPage.clash.title", { names: names.join(copyText("profilesPage.list.sep")), path: paths.length === 1 ? short(paths[0]) : copyText("machineAliases.clash.manyFiles"), lines, now })),
    );
    const opt = (title: string, hint: string, rec: boolean, tail?: HTMLElement): HTMLElement => {
      const o = el("div", rec ? "prof-opt rec" : "prof-opt");
      const h = el("div", "cfg-hint", hint);
      if (tail) h.append(" ", tail);
      o.append(el("b", "", title), h);
      return o;
    };
    const copyLines = link(copyText("profilesPage.clash.copyLines"), () => {
      void writeClipboard(clashes.map((c) => `${short(c.path)}:${c.line}`).join("\n")).then(
        () => toast(copyText("profilesPage.clash.copied"), "", { level: "info" }),
        (e: unknown) => failToast(copyText("detail.act.failed"), e, { level: "error" }),
      );
    });
    const opts3 = el("div", "prof-opts");
    opts3.append(
      opt(copyText("profilesPage.clash.useList"), copyText("profilesPage.clash.useListHint", { names: names.join(copyText("profilesPage.list.sep")) }), true, copyLines),
      opt(copyText("profilesPage.clash.keepBoth"), copyText("profilesPage.clash.keepBothHint"), false),
      opt(copyText("profilesPage.clash.rename"), copyText("profilesPage.clash.renameHint", { name: names[0] }), false, link(copyText("profilesPage.row.edit"), () => void openForm(names[0]))),
    );
    card.appendChild(opts3);
    return card;
  };

  // ── 删 ──

  const startRemove = async (name: string): Promise<void> => {
    const kids = kidsOf(name);
    if (!kids.length) {
      // 撤得回 ⇒ 直接删 ＋ 8 秒撤销（撤销 ＝ 不提交）。
      pendingRemove.add(name);
      render();
      undoToast(
        copyText("profilesPage.remove.done", { name }),
        () => {
          pendingRemove.delete(name);
          render();
        },
        () => void commit([{ op: "remove", name }]).finally(() => pendingRemove.delete(name)),
      );
      return;
    }
    removing = { name, kids, pick: "reparent", impact: null, open: false };
    render();
    try {
      const r = await profileImpact(opts.origin(), [{ op: "remove", name, children: "reparent" }]);
      if (removing?.name === name) {
        removing.impact = r;
        renderNotes();
      }
    } catch {
      // 看不到改前改后不挡删：三个选择照给。
    }
  };

  const removeChoices = (): HTMLElement => {
    const r = removing!;
    const parent = find(r.name)?.from ?? null;
    const box = el("div", "prof-remove");
    box.dataset.role = "remove";
    box.appendChild(el("div", "cfg-note", copyText("profilesPage.remove.ask", { name: r.name, n: String(r.kids.length), kids: r.kids.join(copyText("profilesPage.list.sep")) })));
    const choice = (pick: "reparent" | "cascade" | "cancel", title: string, hint: string): HTMLElement => {
      const o = el("div", pick !== "cancel" && r.pick === pick ? "prof-opt rec picked" : "prof-opt");
      o.tabIndex = 0;
      o.dataset.pick = pick;
      o.append(el("b", "", title), el("div", "cfg-hint", hint));
      o.addEventListener("click", () => {
        if (pick === "cancel") return cancelRemove();
        r.pick = pick;
        renderNotes();
      });
      return o;
    };
    const lost = (find(r.name)?.own ?? []).map((o) => SLOT_LABEL[o.slot]()).join(copyText("profilesPage.list.sep"));
    const reparentTitle = parent ? copyText("profilesPage.remove.reparent", { parent }) : copyText("profilesPage.remove.reparentRoot");
    const reparentHint = copyText("profilesPage.remove.reparentHint", { name: r.name, items: lost || copyText("profilesPage.impact.none") });
    const showImpact = link((r.open ? copyText("profilesPage.impact.hide") : copyText("profilesPage.impact.show")), () => {
      r.open = !r.open;
      renderNotes();
    });
    const rp = choice("reparent", reparentTitle, reparentHint);
    rp.querySelector(".cfg-hint")?.append(" ", showImpact);
    const opts3 = el("div", "prof-opts");
    opts3.append(
      rp,
      choice("cascade", copyText("profilesPage.remove.cascade"), copyText("profilesPage.remove.cascadeHint", { kids: r.kids.join(copyText("profilesPage.list.sep")), n: String(r.kids.length + 1) })),
      choice("cancel", copyText("profilesPage.remove.cancel"), copyText("profilesPage.remove.cancelHint")),
    );
    box.appendChild(opts3);
    if (r.open && r.impact) box.appendChild(impactTable(r.impact));
    const acts = el("div", "cfg-acts");
    const go = button(
      r.pick === "reparent"
        ? parent
          ? copyText("profilesPage.remove.goReparent", { parent })
          : copyText("profilesPage.remove.goReparentRoot")
        : copyText("profilesPage.remove.goCascade", { n: String(r.kids.length + 1) }),
      "settings-btn settings-btn-primary",
      () => void commit([{ op: "remove", name: r.name, children: r.pick }]).then((said) => said === null && cancelRemove()),
    );
    acts.append(go, button(copyText("profilesPage.remove.cancel"), "settings-btn", () => cancelRemove()));
    box.appendChild(acts);
    box.addEventListener("keydown", (ev) => {
      if (ev.key === "Escape") cancelRemove();
    });
    return box;
  };

  const cancelRemove = (): void => {
    removing = null;
    if (behind && !form) void reread();
    else render();
  };

  // ── 写 ──

  /** 存一批改动（带读回时的指纹）。成了 ⇒ 记下修改时间、重读，回 `null`；被别处改过且 `throwStale` ⇒ 抛 [`ProfilesStale`]；
   *  别的 ⇒ 重读之后状态行说那一句、回那一句。 */
  const commit = async (changes: ProfileOp[], throwStale = false): Promise<string | null> => {
    try {
      const done = await writeProfiles(opts.origin(), changes, book?.fingerprint ?? null);
      if (done.reload) toast(copyText("profilesPage.save.done"), done.reload, { level: "info" });
    } catch (e) {
      if (e instanceof ProfilesStale && throwStale) throw e;
      const said = e instanceof Error ? e.message : String(e);
      await reread();
      status.textContent = said;
      return said;
    }
    await reread();
    return null;
  };

  const init = async (seed: boolean): Promise<void> => {
    await commit([{ op: "init", seed }]);
  };

  const ackMigrated = async (): Promise<void> => {
    await commit([{ op: "ackMigrated" }]);
  };

  const onOpenFile = async (): Promise<void> => {
    if (!book) return;
    try {
      await openPath(book.path);
    } catch (e) {
      failToast(copyText("profilesPage.open.failed"), e, { fact: book.path });
    }
  };

  // ── 表单 ──

  const openForm = async (was: string | null): Promise<void> => {
    if (form && !(await leaveForm())) return;
    removing = null;
    const p = was ? find(was) : undefined;
    const f: ProfileForm = p ? structuredClone(p.form) : emptyForm("", book?.profiles.find((x) => x.from === null)?.name ?? null);
    form = { was, form: f, initial: JSON.stringify(f), el: el("div", ""), top: p && !p.usable ? (p.problem?.message ?? null) : null, stale: false };
    selected = null;
    impact = null;
    paintForm();
    render();
    void refreshForm();
    (form.el.querySelector("[data-role=name]") as HTMLInputElement | null)?.focus();
  };

  const leaveForm = async (): Promise<boolean> => {
    if (!form) return true;
    if (JSON.stringify(form.form) !== form.initial) {
      const yes = await (opts.confirm ?? confirmDialog)({ title: copyText("profilesPage.form.discardTitle"), action: copyText("profilesPage.form.discard"), body: copyText("profilesPage.form.discardBody") });
      if (!yes) return false;
    }
    form = null;
    impact = null;
    if (behind && !removing) void reread();
    else render();
    return true;
  };

  let formResolved: Resolved | null = null;
  let bases: { name: string; said: string; selectable: boolean }[] = [];
  let formSeq = 0;

  /** 表单变了：问后端「等于」·「基于」能选哪几条 · 连带谁（各问各的，最后一问的算）。 */
  const refreshForm = async (): Promise<void> => {
    const f = form;
    if (!f) return;
    const mine = ++formSeq;
    const name = f.form.name.trim() || f.was || "";
    const tasks: Promise<void>[] = [];
    tasks.push(
      profileBases(opts.origin(), name).then(
        (b) => {
          if (mine === formSeq) bases = b;
        },
        () => undefined,
      ),
    );
    if (name) {
      tasks.push(
        resolveProfile(opts.origin(), f.was ?? name, f.form, at.trim() || null).then(
          (r) => {
            if (mine === formSeq) formResolved = r;
          },
          (e: unknown) => {
            if (mine === formSeq) formResolved = { chain: [], rows: [], line: null, lineError: null, problem: e instanceof Error ? e.message : String(e) };
          },
        ),
      );
    } else formResolved = null;
    if (f.was && kidsOf(f.was).length) {
      tasks.push(
        profileImpact(opts.origin(), [{ op: "set", was: f.was, form: f.form }]).then(
          (rows) => {
            if (mine === formSeq) impact = { names: rows.map((a) => a.name), rows, open: impact?.open ?? false };
          },
          () => undefined,
        ),
      );
    }
    await Promise.all(tasks);
    if (mine !== formSeq || form !== f) return;
    paintForm();
    renderNotes();
    for (const r of root.querySelectorAll<HTMLElement>(".prof-trow")) r.classList.toggle("hit", hitNames().has(r.dataset.name ?? ""));
  };

  const changed = (next: ProfileForm): void => {
    if (!form) return;
    form.form = next;
    paintForm();
    void refreshForm();
  };

  /** 这一格继承来的是什么（合并表里不是这一段写的、没被盖掉的那几行）。 */
  const inherited = (s: SlotId): string => {
    const me = form?.form.name.trim() || form?.was || "";
    const rows = (formResolved?.rows ?? []).filter((r) => r.slot === s && r.from !== me && (r.overriddenBy === null || r.overriddenBy === me));
    if (!rows.length) return copyText("profilesPage.form.inheritNone");
    const from = rows[rows.length - 1].from;
    return copyText("profilesPage.form.inherit", { value: rows.map((r) => r.said).join(copyText("profilesPage.list.sep")), from });
  };

  const text = (value: string, placeholder: string, onChange: (v: string) => void, role?: string): HTMLInputElement => {
    const i = el("input", "settings-input");
    i.type = "text";
    i.value = value;
    i.placeholder = placeholder;
    if (role) i.dataset.role = role;
    i.addEventListener("change", () => onChange(i.value));
    return i;
  };

  const slotEditor = (s: SlotId, f: ProfileForm, accounts: readonly string[]): HTMLElement => {
    const cell = el("div", "prof-fc");
    cell.dataset.slot = s;
    if (s === "account") {
      const seg = el("div", "prof-seg");
      const chip = (label: string, on: boolean, pick: () => ProfileForm): HTMLButtonElement => {
        const b = button(label, "prof-segb", () => changed(pick()));
        b.setAttribute("aria-pressed", String(on));
        return b;
      };
      seg.appendChild(chip(copyText("profilesPage.form.inheritChip"), f.account === null, () => ({ ...f, account: null })));
      const named = f.account?.kind === "account" ? f.account.name : null;
      const names = named && !accounts.includes(named) ? [...accounts, named] : accounts;
      for (const a of names) seg.appendChild(chip(a, named === a, () => ({ ...f, account: { kind: "account", name: a } })));
      seg.appendChild(chip(copyText("beProfile.val.base"), f.account?.kind === "base", () => ({ ...f, account: { kind: "base" } })));
      cell.appendChild(seg);
      if (f.account === null) cell.appendChild(el("div", "prof-inh", inherited(s)));
      return cell;
    }
    if (!own(f, s)) {
      const row = el("div", "prof-inhrow");
      row.append(
        el("span", "prof-inh", inherited(s)),
        link(s === "cwdIf" ? copyText("profilesPage.form.addCase") : copyText("profilesPage.form.change"), () => changed(startSlot(f, s, accounts))),
      );
      cell.appendChild(row);
      return cell;
    }
    const back = link(copyText("profilesPage.form.back"), () => changed(clearSlot(f, s)));
    if (s === "tmux" && f.tmux) {
      const t = f.tmux;
      const sel = el("select", "settings-input");
      for (const [v, label] of [
        ["auto", copyText("beProfile.val.tmuxAuto")],
        ["fixed", copyText("profilesPage.form.tmuxNamed")],
        ["base", copyText("profilesPage.form.tmuxBase")],
      ] as const) {
        const o = el("option", "", label);
        o.value = v;
        o.title = TMUX_NAMING[v].text();
        sel.appendChild(o);
      }
      sel.value = t.mode;
      sel.dataset.role = "tmux";
      sel.addEventListener("change", () => changed({ ...f, tmux: { ...t, mode: sel.value as typeof t.mode } }));
      const row = el("div", "prof-inhrow");
      row.append(sel);
      if (t.mode !== "auto") row.append(text(t.name, copyText("profilesPage.form.tmuxName"), (v) => changed({ ...f, tmux: { ...t, name: v } })));
      row.append(back);
      const naming = el("div", "cfg-hint", TMUX_NAMING[t.mode].text());
      naming.dataset.role = "tmux-naming";
      cell.append(row, naming);
      // 这台没有 tmux（那台后端判的，与起新会话框同一个判法）⇒ 格不藏，旁边说一句。
      if (book?.tmux === false) {
        const none = el("div", "cfg-hint", copyText("profilesPage.form.noTmux"));
        none.dataset.role = "no-tmux";
        cell.appendChild(none);
      }
      return cell;
    }
    if (s === "cwdIf" && f.cwdIf) {
      const cases = f.cwdIf;
      cases.forEach((c, i) => {
        const row = el("div", "prof-inhrow");
        const put = (patch: Partial<typeof c>): ProfileForm => ({ ...f, cwdIf: cases.map((x, j) => (j === i ? { ...x, ...patch } : x)) });
        row.append(
          el("span", "", copyText("profilesPage.form.caseIn")),
          text(c.at, copyText("profilesPage.form.caseAt"), (v) => changed(put({ at: v }))),
          el("span", "", copyText("profilesPage.form.caseArrow")),
          text(c.to, copyText("profilesPage.form.caseTo"), (v) => changed(put({ to: v }))),
          link(copyText("profilesPage.form.caseDrop"), () => changed(cases.length > 1 ? { ...f, cwdIf: cases.filter((_, j) => j !== i) } : clearSlot(f, s))),
        );
        cell.appendChild(row);
      });
      const more = el("div", "prof-inhrow");
      more.append(link(copyText("profilesPage.form.addCase"), () => changed({ ...f, cwdIf: [...cases, { at: "", to: "" }] })), back);
      cell.appendChild(more);
      return cell;
    }
    if (s === "detach" || s === "busRegister") {
      const row = el("div", "prof-inhrow");
      row.append(el("span", "", copyText("beProfile.val.on")), back);
      cell.appendChild(row);
      return cell;
    }
    const row = el("div", "prof-inhrow");
    const v = f[s as "cwd" | "agent" | "args" | "launcher" | "tmuxSize" | "busNote"] ?? "";
    row.append(text(v, SLOT_LABEL[s](), (nv) => changed({ ...f, [s]: nv })), back);
    cell.appendChild(row);
    return cell;
  };

  const paintForm = (): void => {
    const st = form;
    if (!st) return;
    const f = st.form;
    const box = el("div", "cfg-form prof-form");
    box.dataset.role = "profile-form";
    box.appendChild(el("div", "cfg-form-title", st.was ? copyText("profilesPage.form.titleEdit", { name: st.was }) : copyText("profilesPage.form.titleNew")));
    if (st.top || st.stale) {
      const top = el("div", "prof-form-top");
      top.dataset.role = "form-top";
      top.appendChild(el("span", "", st.stale ? copyText("profilesPage.form.stale") : (st.top ?? "")));
      if (st.stale)
        top.appendChild(
          link(copyText("profilesPage.form.reread"), () => {
            st.stale = false;
            void reread().then(() => {
              paintForm();
              void refreshForm();
            });
          }),
        );
      box.appendChild(top);
    }
    const grid = el("div", "cfg-fg prof-fg");
    const field = (label: string, ...ctl: HTMLElement[]): void => {
      const c = el("div", "prof-fc");
      c.append(...ctl);
      grid.append(el("div", "cfg-fl", label), c);
    };
    const nameIn = text(f.name, copyText("profilesPage.form.nameHint"), (v) => changed({ ...f, name: v }), "name");
    field(copyText("profilesPage.form.name"), nameIn, el("div", "cfg-hint", copyText("profilesPage.form.nameHelp")));
    // 「基于」下拉：名字 ＋ 小字摘要（后端给的那一句）；自己那一条灰着说为什么。
    const fromSel = kitSelect({
      label: copyText("profilesPage.form.from"),
      options: [
        { value: "", label: copyText("profilesPage.form.fromNone") },
        ...bases.map((b) => ({
          value: b.name,
          label: b.name,
          note: b.selectable ? b.said : undefined,
          enabled: b.selectable,
          why: b.selectable ? undefined : copyText("profilesPage.form.fromSelf"),
        })),
        ...(f.from && !bases.some((b) => b.name === f.from) ? [{ value: f.from, label: f.from }] : []),
      ],
      value: f.from ?? "",
      onChange: (v) => changed({ ...f, from: v || null }),
    });
    fromSel.el.dataset.role = "from";
    field(copyText("profilesPage.form.from"), fromSel.el, el("div", "cfg-hint", copyText("profilesPage.form.fromHelp")));
    const accounts = book?.accounts ?? [];
    for (const s of MAIN_SLOTS) field(SLOT_LABEL[s](), slotEditor(s, f, accounts));
    const more = el("details", "prof-more");
    if (MORE_SLOTS.some((s) => own(f, s))) more.open = true;
    more.appendChild(el("summary", "", copyText("profilesPage.form.more")));
    const moreGrid = el("div", "cfg-fg prof-fg");
    for (const s of MORE_SLOTS) moreGrid.append(el("div", "cfg-fl", SLOT_LABEL[s]()), slotEditor(s, f, accounts));
    more.appendChild(moreGrid);
    const prevMore = st.el.querySelector("details.prof-more") as HTMLDetailsElement | null;
    if (prevMore) more.open = prevMore.open;
    field("", more);
    box.appendChild(grid);
    const name = f.name.trim() || st.was || "";
    box.appendChild(el("div", "cfg-hint", copyText("profilesPage.merge.equals", { name: name || copyText("profilesPage.form.unnamed") })));
    const line = el("pre", "prof-line", formResolved?.line ?? formResolved?.lineError ?? formResolved?.problem ?? "");
    line.dataset.role = "form-equals";
    if (!formResolved?.line) line.classList.add("prof-line-bad");
    box.appendChild(line);
    if (formResolved?.chain.length) box.appendChild(el("div", "cfg-hint", copyText("profilesPage.form.chain", { name, chain: formResolved.chain.join(copyText("beProfile.chain.arrow")) })));
    const acts = el("div", "cfg-acts");
    const save = button(copyText("profilesPage.form.save"), "settings-btn settings-btn-primary", () => void onSave());
    save.dataset.role = "save";
    const kids = st.was ? kidsOf(st.was) : [];
    const renamed = st.was !== null && f.name.trim() !== "" && f.name.trim() !== st.was;
    const hint = renamed && kids.length
      ? copyText("profilesPage.form.renameHint", { n: String(kids.length), name: f.name.trim() })
      : impact && impact.names.length
        ? copyText("profilesPage.form.impactHint", { n: String(impact.names.length) })
        : st.was
          ? copyText("profilesPage.form.saveHintEdit", { name: st.was })
          : copyText("profilesPage.form.saveHintNew", { name: name || copyText("profilesPage.form.unnamed") });
    acts.append(save, button(copyText("profilesPage.form.cancel"), "settings-btn", () => void leaveForm()), el("span", "cfg-hint", hint));
    box.appendChild(acts);
    box.addEventListener("keydown", (ev) => {
      if (ev.isComposing) return;
      if (ev.key === "Escape") {
        ev.preventDefault();
        void leaveForm();
      } else if (ev.key === "Enter" && (ev.ctrlKey || ev.metaKey)) {
        ev.preventDefault();
        void onSave();
      }
    });
    const focused = document.activeElement instanceof HTMLElement && st.el.contains(document.activeElement) ? document.activeElement.dataset.role : undefined;
    st.el.replaceChildren(box);
    if (focused) (box.querySelector(`[data-role="${focused}"]`) as HTMLElement | null)?.focus();
  };

  const onSave = async (): Promise<void> => {
    const st = form;
    if (!st) return;
    try {
      const said = await commit([{ op: "set", was: st.was, form: st.form }], true);
      if (said === null) {
        form = null;
        impact = null;
        selected = st.form.name.trim() || null;
        render();
        void askResolve();
      } else if (form === st) {
        st.top = said;
        status.textContent = "";
        render();
        paintForm();
      }
    } catch (e) {
      if (e instanceof ProfilesStale && form === st) {
        st.stale = true;
        paintForm();
      }
    }
  };

  return {
    element: root,
    async load() {
      await Promise.all([reread(), subscribe().catch(() => undefined)]);
    },
    setClashes(c) {
      clashes = c;
      renderNotes();
    },
  };
}
