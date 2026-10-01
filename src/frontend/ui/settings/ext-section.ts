/**
 * 设置顶层页「扩展」：跨机器的 skill 与 MCP，一张表 ＋ 一个抽屉。
 *
 * 交互链路：看到（一行一个条目，每台机器一个点；有备注的行带一个记号）→ 点开（右侧抽屉：备注 ＋ 每台一行，展开成它的各处）
 * → 机器那一行的「装到…」或某一处的「卸载」→ 在确认卡上确认（装到哪在卡上选）。
 *
 * # 本文件只画、只问
 *
 * - 每格的点、那台上的各处（全局一行 ＋ 每个装着它的项目一行）与各自的态和「卸载」、「装到…」从哪台拿哪一版、
 *   能装到哪几处（不能选的为什么）、建议的那一处 —— 全是后端 `ext-list` 答的，这里照着画，不比较、不推断。
 * - 装：确认卡由本机后端当枢纽拼（`ext-hub-preview`，带用户在卡上选的那一处）；确认时交回卡上的记号（`ext-hub-apply`）；
 *   看过之后变了 ⇒ 后端答 `stale`，卡上说一句、给「重看」。卸：问被卸的那一台（`ext-uninstall-*`）。
 * - 备注：自带扩展的内置备注照画；用户写的经本机后端记进它的目录（`ext-note-set`），写完让本机后端同步一趟各台。
 * - 要加钩子的那一个（自带的 cc-bus）：抽屉里每台一行钩子状态与要加的内容，问那台后端（`hooks-diag`，抽屉打开时问一次），
 *   界面不读那份配置文件、也不写它。
 * - 做完之后重读那张表（远端那台先让本机后端对它同步一趟），点自己变；不轮询。
 */
import { copyText } from "../copy-table";
import { LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { commands } from "../ipc/commands";
import { syncAssets } from "../assets-sync-reads";
import { openFileWindow } from "../file-window";
import { resolveRemoteConfigByOrigin } from "../remote-config";
import { buildPasteBlock } from "../paste-block";
import { describeState, fetchHooksReport, type HooksReport } from "../cc-bus-hooks-reads";
import {
  ExtRefused,
  extHubApply,
  extHubPreview,
  extList,
  extNoteSet,
  extUninstallApply,
  extUninstallPreview,
  type ExtAsk,
  type ExtBring,
  type ExtCard,
  type ExtCell,
  type ExtList,
  type ExtLoc,
  type ExtMachine,
  type ExtPlace,
  type ExtRow,
  type ExtTarget,
  type ExtUninstallCard,
} from "../ext-reads";

type KindFilter = "all" | "skill" | "mcp";

/** 点的样式类（逐字写出：CSS 对账按字面认类名）。 */
const DOT_CLASS: Record<ExtCell["state"], string> = {
  same: "ext-dot is-same",
  differs: "ext-dot is-differs",
  missing: "ext-dot is-missing",
  project: "ext-dot is-project",
};

/** 钩子那一行的语气类（逐字写出，同上）。 */
const TONE_CLASS: Record<"ok" | "bad" | "unknown", string> = {
  ok: "ext-hook-state is-ok",
  bad: "ext-hook-state is-bad",
  unknown: "ext-hook-state is-unknown",
};

/** 一格的点（态 ⇒ 字形；字形住文案表）。 */
export function dotOf(state: ExtCell["state"]): string {
  switch (state) {
    case "same":
      return copyText("extPage.dot.same");
    case "differs":
      return copyText("extPage.dot.differs");
    case "missing":
      return copyText("extPage.dot.missing");
    case "project":
      return copyText("extPage.dot.project");
  }
}

/** 一格 / 一处给人看的那句话（悬停 · 抽屉里那一行）。 */
export function stateText(state: ExtCell["state"], places: readonly ExtPlace[]): string {
  switch (state) {
    case "same":
      return copyText("extPage.state.same");
    case "differs":
      return copyText("extPage.state.differs");
    case "missing":
      return copyText("extPage.state.missing");
    case "project":
      return copyText("extPage.state.project", {
        dirs: places.flatMap((p) => (p.at.level === "project" ? [p.at.dir] : [])).join(copyText("extPage.list.sep")),
      });
  }
}

/** 一处叫什么：全局 · 某个项目。 */
export function locText(at: ExtLoc): string {
  return at.level === "user" ? copyText("extPage.loc.user") : copyText("extPage.loc.project", { dir: at.dir });
}

/** 两处是不是同一处（卡上那一项选中没有）。 */
function sameLoc(a: ExtLoc, b: ExtLoc): boolean {
  return a.level === "user" ? b.level === "user" : b.level === "project" && a.dir === b.dir;
}

function kindText(r: ExtRow): string {
  return r.kind === "skill" ? copyText("extPage.kind.skill") : copyText("extPage.kind.mcp");
}

function machineName(m: ExtMachine): string {
  return m.here ? copyText("extPage.machine.here") : m.name;
}

/** 这台在通道上叫什么（本机后端那一台 = `LOCAL_ORIGIN`）。 */
function originOf(m: ExtMachine): Origin {
  return m.here || m.key === null ? LOCAL_ORIGIN : m.key;
}

/** 抽屉里一台机器那一行的键（临时状态按它记）。 */
function machineKey(m: ExtMachine): string {
  return m.here ? "" : (m.key ?? m.name);
}

/** 这一行过不过筛（名字 / 简介里含搜的字，种类对得上）。 */
export function passes(r: ExtRow, query: string, kind: KindFilter): boolean {
  if (kind !== "all" && r.kind !== kind) return false;
  const q = query.trim().toLowerCase();
  return q === "" || r.name.toLowerCase().includes(q) || (r.about ?? "").toLowerCase().includes(q);
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function button(text: string, onClick: () => void, primary = false): HTMLButtonElement {
  const b = el("button", primary ? "settings-btn settings-btn-primary" : "settings-btn", text);
  b.type = "button";
  b.addEventListener("click", onClick);
  return b;
}

/** 出错那一句说的是哪个动作。 */
type ExtVerb = "install" | "uninstall";

/**
 * 一问没成 ⇒ 给人看的那一句：「装到 / 从…卸载 <那台> 失败：」＋ 那台后端说的那句本身（码不上屏，`stale` 只决定给不给「重看」）。
 */
function said(e: unknown, verb: ExtVerb, m: ExtMachine): { text: string; stale: boolean } {
  const why = e instanceof Error ? e.message : String(e);
  const machine = machineName(m);
  const text = verb === "install" ? copyText("extPage.error.install", { machine, said: why }) : copyText("extPage.error.uninstall", { machine, said: why });
  return { text, stale: e instanceof ExtRefused && e.code === "stale" };
}

/** 「装到…」那张卡要记住的：交给枢纽的那几格 · 能选的各处 · 来源叫什么。 */
interface BringCard {
  kind: "bring";
  ask: ExtAsk;
  targets: ExtTarget[];
  fromName: string;
  card: ExtCard | null;
}

/** 抽屉里一台机器那一行的临时状态：打开的确认卡 · 在路上 · 那一句出错的话（整句，带动作与机器）· 做完那一句。 */
interface Slot {
  card?: BringCard | { kind: "remove"; at: ExtLoc; card: ExtUninstallCard | null };
  busy?: boolean;
  error?: { text: string; stale: boolean; again?: () => void };
  done?: string;
}

/** 一台的钩子状态：在问 · 问到了 · 问不出来。 */
type HookSlot = { kind: "loading" } | { kind: "report"; report: HooksReport } | { kind: "failed"; said: string };

export class ExtSection {
  readonly element: HTMLElement;
  private readonly status: HTMLElement;
  private readonly table: HTMLElement;
  private readonly drawer: HTMLElement;
  private list: ExtList | null = null;
  private query = "";
  private kind: KindFilter = "all";
  private openKey: string | null = null;
  private slots = new Map<string, Slot>();
  private hooks = new Map<string, HookSlot>();
  /** 备注正在改（那一段草稿；`null` = 没在改）· 写备注那一问没成时那一句。 */
  private noteDraft: string | null = null;
  private noteError: string | null = null;
  private seq = 0;

  constructor() {
    this.element = el("div", "ext-page");
    const main = el("div", "ext-main");
    const bar = el("div", "ext-toolbar");
    const search = el("input", "ext-search");
    search.type = "search";
    search.placeholder = copyText("extPage.search.placeholder");
    search.setAttribute("aria-label", copyText("extPage.search.aria"));
    search.addEventListener("input", () => {
      this.query = search.value;
      this.renderTable();
    });
    bar.appendChild(search);
    const filter = el("div", "ext-filter");
    const kinds: [KindFilter, string][] = [
      ["all", copyText("extPage.filter.all")],
      ["skill", copyText("extPage.filter.skill")],
      ["mcp", copyText("extPage.filter.mcp")],
    ];
    for (const [k, label] of kinds) {
      const b = button(label, () => {
        this.kind = k;
        for (const c of filter.children) c.classList.toggle("is-active", c === b);
        this.renderTable();
      });
      b.classList.toggle("is-active", k === "all");
      filter.appendChild(b);
    }
    bar.appendChild(filter);
    main.appendChild(bar);
    this.status = el("div", "settings-hint ext-status");
    main.appendChild(this.status);
    this.table = el("div", "ext-table");
    main.appendChild(this.table);
    this.element.appendChild(main);
    this.drawer = el("aside", "ext-drawer");
    this.drawer.hidden = true;
    this.element.appendChild(this.drawer);
  }

  /** 这一页变可见：先画本机后端手上那一份（算「来看了一次」），再让它同步一趟各台、重读。 */
  loadNow(): void {
    void this.reload(true, null);
  }

  /** 重读那张表；`sync` = 先让本机后端对那台（`LOCAL_ORIGIN` = 各台）同步一趟。 */
  private async reload(visit: boolean, sync: Origin | null): Promise<void> {
    const my = ++this.seq;
    if (!this.list) this.status.textContent = copyText("extPage.status.loading");
    try {
      const synced = sync === null ? "" : await this.syncOnce(sync);
      const list = await extList(visit);
      if (my !== this.seq) return;
      this.list = list;
      const problems = list.problems.length > 0 ? copyText("extPage.status.problems", { list: list.problems.join(copyText("extPage.list.sep")) }) : "";
      this.status.textContent = [problems, synced].filter((x) => x !== "").join(" ");
      this.renderTable();
      this.renderDrawer();
      if (visit) void this.reload(false, LOCAL_ORIGIN);
    } catch (e) {
      if (my !== this.seq) return;
      this.status.textContent = copyText("extPage.status.failed", { e: e instanceof Error ? e.message : String(e) });
    }
  }

  /** 让本机后端对那台（`LOCAL_ORIGIN` = 各台）同步一趟；没对上的那几台说出来（表照画手上那一份）。 */
  private async syncOnce(on: Origin): Promise<string> {
    try {
      const s = await syncAssets(on);
      return s.synced
        .flatMap((x) => (x.error === null ? [] : [copyText("extPage.status.syncRow", { machine: x.origin, e: x.error })]))
        .join(" ");
    } catch (e) {
      return copyText("extPage.status.syncFailed", { e: e instanceof Error ? e.message : String(e) });
    }
  }

  private renderTable(): void {
    this.table.replaceChildren();
    const list = this.list;
    if (!list) return;
    const rows = list.rows.filter((r) => passes(r, this.query, this.kind));
    if (rows.length === 0) {
      const said = list.rows.length === 0 ? copyText("extPage.table.empty") : copyText("extPage.table.noMatch");
      this.table.appendChild(el("div", "settings-hint", said));
      return;
    }
    for (const r of rows) {
      const line = el("button", "ext-row");
      line.type = "button";
      line.dataset.key = `${r.kind}/${r.name}`;
      const name = el("span", "ext-name", r.name);
      if (r.new) name.appendChild(el("span", "ext-new", copyText("extPage.row.new")));
      if (r.builtin !== null || r.note !== null) {
        const mark = el("span", "ext-note-mark", copyText("extPage.row.noteMark"));
        mark.title = copyText("extPage.row.noteMarkTitle");
        name.appendChild(mark);
      }
      line.appendChild(name);
      line.appendChild(el("span", "ext-kind", kindText(r)));
      line.appendChild(el("span", "ext-about", r.about ?? ""));
      const dots = el("span", "ext-dots");
      r.cells.forEach((c, i) => {
        const m = list.machines[i];
        const d = el("span", DOT_CLASS[c.state], dotOf(c.state));
        d.title = copyText("extPage.dot.title", { machine: machineName(m), state: stateText(c.state, c.places) });
        dots.appendChild(d);
      });
      line.appendChild(dots);
      line.addEventListener("click", () => this.openRow(r));
      this.table.appendChild(line);
    }
  }

  /** 点开一行：抽屉换成它；要加钩子的那一个顺手问各台一次钩子状态。 */
  private openRow(r: ExtRow): void {
    this.openKey = `${r.kind}/${r.name}`;
    this.slots.clear();
    this.hooks.clear();
    this.noteDraft = null;
    this.noteError = null;
    this.renderDrawer();
    if (r.builtin?.hooks && this.list) for (const m of this.list.machines) if (m.reachable) void this.readHooks(m);
  }

  /** 问那台后端它的钩子装了没、要加的内容是什么（只读）。 */
  private async readHooks(m: ExtMachine): Promise<void> {
    const key = machineKey(m);
    this.hooks.set(key, { kind: "loading" });
    this.renderDrawer();
    try {
      const report = await fetchHooksReport(originOf(m));
      this.hooks.set(key, { kind: "report", report });
    } catch (e) {
      this.hooks.set(key, { kind: "failed", said: e instanceof Error ? e.message : String(e) });
    }
    this.renderDrawer();
  }

  private current(): ExtRow | null {
    return this.list?.rows.find((r) => `${r.kind}/${r.name}` === this.openKey) ?? null;
  }

  private renderDrawer(): void {
    const r = this.current();
    const list = this.list;
    this.drawer.hidden = r === null;
    this.drawer.replaceChildren();
    if (!r || !list) return;
    const head = el("div", "ext-drawer-head");
    head.appendChild(el("strong", "", r.name));
    head.appendChild(el("span", "ext-kind", kindText(r)));
    head.appendChild(
      button(copyText("extPage.drawer.close"), () => {
        this.openKey = null;
        this.renderDrawer();
      }),
    );
    this.drawer.appendChild(head);
    this.drawer.appendChild(this.notesOf(r));
    if (r.builtin?.hooks) this.drawer.appendChild(this.hooksOf(list));
    if (r.about) this.drawer.appendChild(el("p", "ext-drawer-about", r.about));
    if (r.detail.length > 0) {
      const more = el("details", "ext-detail");
      more.appendChild(el("summary", "", copyText("extPage.drawer.detail")));
      for (const d of r.detail) more.appendChild(el("div", "ext-detail-line", copyText("extPage.drawer.detailLine", { label: d.label, value: d.value })));
      this.drawer.appendChild(more);
    }
    r.cells.forEach((c, i) => this.drawer.appendChild(this.machineRow(r, c, list.machines[i])));
  }

  /** 抽屉顶部的备注：内置的（自带的扩展）照画；用户的可以写 / 改 / 清（清 = 存一段空的）。 */
  private notesOf(r: ExtRow): HTMLElement {
    const box = el("div", "ext-notes");
    if (r.builtin) box.appendChild(el("p", "ext-note-builtin", r.builtin.note));
    if (this.noteDraft !== null) {
      const input = el("textarea", "settings-input ext-note-input");
      input.rows = 3;
      input.value = this.noteDraft;
      input.setAttribute("aria-label", copyText("extPage.note.aria"));
      input.addEventListener("input", () => {
        this.noteDraft = input.value;
      });
      box.appendChild(input);
      const save = button(copyText("extPage.note.save"), () => void this.saveNote(r), true);
      const cancel = button(copyText("extPage.card.cancel"), () => {
        this.noteDraft = null;
        this.noteError = null;
        this.renderDrawer();
      });
      box.appendChild(el("div", "ext-card-buttons")).append(save, cancel);
    } else {
      if (r.note !== null) box.appendChild(el("p", "ext-note-user", r.note));
      box.appendChild(
        button(r.note === null ? copyText("extPage.note.write") : copyText("extPage.note.edit"), () => {
          this.noteDraft = r.note ?? "";
          this.renderDrawer();
        }),
      );
    }
    if (this.noteError !== null) box.appendChild(el("div", "ext-error", this.noteError));
    return box;
  }

  /** 写备注：记进本机后端的目录，再让它同步一趟各台、重读。 */
  private async saveNote(r: ExtRow): Promise<void> {
    const text = this.noteDraft ?? "";
    try {
      await extNoteSet(r.kind, r.name, text);
      this.noteDraft = null;
      this.noteError = null;
      await this.reload(false, LOCAL_ORIGIN);
    } catch (e) {
      this.noteError = copyText("extPage.note.failed", { said: e instanceof Error ? e.message : String(e) });
      this.renderDrawer();
    }
  }

  /** 每台一行：钩子装了没有（那台后端读它自己那份配置判）＋ 要加的内容（那台的 cc-bus 装着才有）。 */
  private hooksOf(list: ExtList): HTMLElement {
    const box = el("div", "ext-hooks");
    box.appendChild(el("div", "settings-label", copyText("extPage.hooks.title")));
    for (const m of list.machines) {
      const line = el("div", "ext-hook");
      line.appendChild(el("span", "ext-machine-name", machineName(m)));
      const h = this.hooks.get(machineKey(m));
      if (!m.reachable) line.appendChild(el("div", "settings-hint", copyText("extPage.hooks.offline")));
      else if (!h || h.kind === "loading") line.appendChild(el("div", "settings-hint", copyText("extPage.hooks.loading")));
      else if (h.kind === "failed") line.appendChild(el("div", "ext-error", copyText("extPage.hooks.failed", { said: h.said })));
      else this.hookReport(line, h.report);
      box.appendChild(line);
    }
    return box;
  }

  private hookReport(line: HTMLElement, rep: HooksReport): void {
    if (!rep.supported) {
      line.appendChild(el("div", "settings-hint ext-hook-unsupported", copyText("extPage.hooks.unsupported")));
      return;
    }
    for (const [label, st] of [
      [copyText("extPage.hooks.sessionStart"), rep.diagnosis.session_start],
      [copyText("extPage.hooks.stop"), rep.diagnosis.stop],
    ] as const) {
      const d = describeState(st);
      const s = el("div", TONE_CLASS[d.tone], copyText("extPage.hooks.stateLine", { hook: label, state: d.text }));
      s.dataset.kind = st.kind;
      line.appendChild(s);
    }
    if (rep.diagnosis.note) line.appendChild(el("div", "settings-hint", rep.diagnosis.note));
    const snippet = rep.snippet;
    if (snippet === null) {
      line.appendChild(el("div", "settings-hint ext-hook-install-first", copyText("extPage.hooks.installFirst")));
      return;
    }
    const more = el("details", "ext-hook-snippet");
    more.appendChild(el("summary", "", copyText("extPage.hooks.copy")));
    const paste = buildPasteBlock({
      text: () => snippet,
      target: copyText("extPage.hooks.target", { source: rep.source }),
      mergeNote: copyText("ccBusHooks.snippet.merge"),
      activation: copyText("ccBusHooks.snippet.activation"),
      multiline: true,
      rows: 10,
    });
    paste.refresh();
    more.appendChild(paste.element);
    line.appendChild(more);
  }

  private machineRow(r: ExtRow, c: ExtCell, m: ExtMachine): HTMLElement {
    const key = machineKey(m);
    const slot = this.slots.get(key) ?? {};
    const line = el("div", "ext-machine");
    const top = el("div", "ext-machine-top");
    top.appendChild(el("span", DOT_CLASS[c.state], dotOf(c.state)));
    top.appendChild(el("span", "ext-machine-name", machineName(m)));
    top.appendChild(el("span", "ext-machine-state", stateText(c.state, c.places)));
    const bring = c.bring;
    if (bring) {
      const b = button(copyText("extPage.button.bring"), () => void this.openBring(r, m, bring));
      b.disabled = slot.busy === true || slot.card !== undefined;
      top.appendChild(b);
    }
    line.appendChild(top);
    if (!bring && c.note) line.appendChild(el("div", "settings-hint", c.note));
    const places = el("div", "ext-places");
    for (const p of c.places) places.appendChild(this.placeRow(r, m, p, slot));
    line.appendChild(places);
    if (slot.done) line.appendChild(el("div", "settings-hint ext-done", slot.done));
    if (slot.error) {
      const err = el("div", "ext-error", slot.error.text);
      const again = slot.error.again;
      if (slot.error.stale && again) err.appendChild(button(copyText("extPage.card.again"), again));
      line.appendChild(err);
    }
    if (slot.card) line.appendChild(this.cardOf(r, m, key, slot));
    return line;
  }

  /** 那台上的一处：在哪 · 态 · 「卸载」（有才给）· skill 在远端有目录的 ⇒「在文件窗口里打开」。 */
  private placeRow(r: ExtRow, m: ExtMachine, p: ExtPlace, slot: Slot): HTMLElement {
    const row = el("div", "ext-place");
    row.appendChild(el("span", "ext-place-at", locText(p.at)));
    row.appendChild(el("span", "ext-machine-state", stateText(p.state, [])));
    if (p.uninstall) {
      const b = button(copyText("extPage.button.uninstall"), () => void this.openRemove(r, m, p.at));
      b.disabled = slot.busy === true || slot.card !== undefined;
      row.appendChild(b);
    }
    if (r.kind === "skill" && p.dir !== null && !m.here && m.key !== null) {
      const dir = p.dir;
      const host = m.key;
      row.appendChild(
        button(copyText("extPage.button.openFiles"), () => {
          void resolveRemoteConfigByOrigin(host).then((cfg) => {
            if (cfg) void openFileWindow(cfg, { dir });
            else this.setSlot(machineKey(m), { error: { text: copyText("extPage.error.noHostConfig", { machine: machineName(m) }), stale: false } });
          });
        }),
      );
    }
    if (p.note) row.appendChild(el("span", "settings-hint", p.note));
    return row;
  }

  private setSlot(key: string, s: Slot): void {
    this.slots.set(key, s);
    this.renderDrawer();
  }

  /** 点了某一处的「卸载」：先问被卸的那一台要一张卡。 */
  private async openRemove(r: ExtRow, m: ExtMachine, at: ExtLoc): Promise<void> {
    const key = machineKey(m);
    this.setSlot(key, { busy: true, card: { kind: "remove", at, card: null } });
    try {
      const card = await extUninstallPreview(originOf(m), r.kind, r.name, at);
      this.setSlot(key, { card: { kind: "remove", at, card } });
    } catch (e) {
      this.setSlot(key, { error: { ...said(e, "uninstall", m), again: () => void this.openRemove(r, m, at) } });
    }
  }

  /** 点了「装到…」：先按后端建议的那一处问一张卡（本机后端当枢纽）。 */
  private async openBring(r: ExtRow, m: ExtMachine, bring: ExtBring): Promise<void> {
    const ask: ExtAsk = { kind: r.kind, name: r.name, from: bring.from, to: m.here ? null : m.key, scope: bring.scope };
    await this.previewBring(machineKey(m), m, { kind: "bring", ask, targets: bring.targets, fromName: bring.fromName, card: null });
  }

  private async previewBring(key: string, m: ExtMachine, b: BringCard): Promise<void> {
    this.setSlot(key, { busy: true, card: { ...b, card: null } });
    try {
      const card = await extHubPreview(b.ask);
      this.setSlot(key, { card: { ...b, card } });
    } catch (e) {
      // 卡没拼成也留着「装到哪」：换一处再看。
      this.setSlot(key, { card: { ...b, card: null }, error: { ...said(e, "install", m), again: () => void this.previewBring(key, m, b) } });
    }
  }

  private cardOf(r: ExtRow, m: ExtMachine, key: string, slot: Slot): HTMLElement {
    const box = el("div", "ext-card");
    const c = slot.card;
    const cancel = button(copyText("extPage.card.cancel"), () => this.setSlot(key, {}));
    if (!c) return box;
    if (c.kind === "remove") {
      const card = c.card;
      if (card === null) {
        box.appendChild(el("div", "settings-hint", copyText("extPage.card.loading")));
        return box;
      }
      box.appendChild(el("div", "ext-card-title", copyText("extPage.card.removeTitle", { machine: machineName(m), path: card.path })));
      box.appendChild(el("div", "ext-card-said", card.said));
      const files = el("ul", "ext-card-files");
      for (const f of card.files) files.appendChild(el("li", "", f));
      box.appendChild(files);
      const ok = button(
        copyText("extPage.card.confirmRemove"),
        () =>
          void this.run(r, key, m, "uninstall", async () => {
            const done = await extUninstallApply(originOf(m), card, c.at);
            return done.note ?? copyText("extPage.done.removed", { n: String(done.changed.length) });
          }),
        true,
      );
      ok.disabled = slot.busy === true;
      box.appendChild(el("div", "ext-card-buttons")).append(ok, cancel);
      return box;
    }
    box.appendChild(el("div", "ext-card-title", copyText("extPage.card.bringTitle", { from: c.fromName, to: machineName(m) })));
    box.appendChild(this.targetsOf(key, m, c, slot));
    const card = c.card;
    if (card === null) {
      if (!slot.error) box.appendChild(el("div", "settings-hint", copyText("extPage.card.loading")));
      box.appendChild(el("div", "ext-card-buttons")).append(cancel);
      return box;
    }
    box.appendChild(el("div", "ext-card-path", card.path));
    const files = el("ul", "ext-card-files");
    for (const f of card.writes) files.appendChild(el("li", "", f));
    box.appendChild(files);
    if (card.unchanged) box.appendChild(el("div", "settings-hint", copyText("extPage.card.unchanged")));
    if (card.config !== null) box.appendChild(el("pre", "ext-card-config", card.config));
    const fill: Record<string, Record<string, string>> = {};
    for (const s of card.slots) {
      const row = el("label", "ext-card-field", copyText("extPage.card.slot", { field: s.field, key: s.key }));
      const input = el("input", "ext-card-secret");
      input.type = "password";
      input.autocomplete = "off";
      input.placeholder = s.kept ? copyText("extPage.card.slotKept") : copyText("extPage.card.slotEmpty");
      input.addEventListener("input", () => {
        (fill[s.field] ??= {})[s.key] = input.value;
      });
      row.appendChild(input);
      box.appendChild(row);
    }
    for (const s of card.suspects) box.appendChild(el("div", "ext-card-suspect", s));
    if (card.stop) box.appendChild(el("div", "ext-error", card.stop));
    const ok = button(
      copyText("extPage.card.confirm"),
      () =>
        void this.run(r, key, m, "install", async () => {
          const done = await extHubApply(c.ask, card, fill);
          const said = done.note ?? copyText("extPage.done.written", { n: String(done.changed.length) });
          // 自带的那一个装到本机：装出来的命令在这台跑不跑得起来（本机 ccm 够不够新）是 monitor 那一侧的事实，另问一次。
          const warn = r.builtin !== null && m.here ? await commands.cc_bus_ccm_precheck() : null;
          return warn ? `${said} ${copyText("extPage.done.warn", { said: warn })}` : said;
        }),
      true,
    );
    ok.disabled = slot.busy === true || card.stop !== null || card.unchanged;
    box.appendChild(el("div", "ext-card-buttons")).append(ok, cancel);
    return box;
  }

  /** 卡上「装到哪」：后端给的各处，不能选的照列、置灰、旁注为什么；换一处就按那一处重看一张卡。 */
  private targetsOf(key: string, m: ExtMachine, c: BringCard, slot: Slot): HTMLElement {
    const box = el("fieldset", "ext-targets");
    box.appendChild(el("legend", "", copyText("extPage.card.where")));
    const group = `ext-target-${key || "here"}`;
    for (const t of c.targets) {
      const row = el("label", t.ok ? "ext-target" : "ext-target is-off");
      const radio = el("input", "");
      radio.type = "radio";
      radio.name = group;
      radio.checked = sameLoc(t.at, c.ask.scope.to);
      radio.disabled = !t.ok || slot.busy === true;
      radio.addEventListener("change", () => {
        if (radio.checked) void this.previewBring(key, m, { ...c, ask: { ...c.ask, scope: { from: c.ask.scope.from, to: t.at } } });
      });
      row.appendChild(radio);
      row.appendChild(el("span", "", locText(t.at)));
      if (t.note) row.appendChild(el("span", "ext-target-note", t.note));
      box.appendChild(row);
    }
    return box;
  }

  /** 确认之后：做 → 成了就重读那张表（那台先同步一趟），点自己变；没成就在那一行说。 */
  private async run(r: ExtRow, key: string, m: ExtMachine, verb: ExtVerb, go: () => Promise<string>): Promise<void> {
    const keep = this.slots.get(key) ?? {};
    this.setSlot(key, { ...keep, busy: true, error: undefined });
    try {
      const done = await go();
      this.setSlot(key, { done });
      await this.reload(false, m.here ? null : originOf(m));
      if (r.builtin?.hooks) void this.readHooks(m);
    } catch (e) {
      const card = keep.card;
      const again = card?.kind === "bring" ? () => void this.previewBring(key, m, card) : card?.kind === "remove" ? () => void this.openRemove(r, m, card.at) : undefined;
      this.setSlot(key, { ...keep, busy: false, error: { ...said(e, verb, m), again } });
    }
  }
}
