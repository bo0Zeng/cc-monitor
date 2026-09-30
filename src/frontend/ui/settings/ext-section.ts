/**
 * 设置顶层页「扩展」：跨机器的 skill 与 MCP，一张表 ＋ 一个抽屉。
 *
 * 交互链路：看到（一行一个条目，每台机器一个点）→ 点开（右侧抽屉，每台一行）→ 点那台后面唯一那个按钮 → 在确认卡上确认。
 *
 * # 本文件只画、只问
 *
 * - 每格的态、那一格的按钮做什么、从哪台拿哪一版、没有按钮时为什么 —— 全是后端 `ext-list` 答的，这里照着画点和字，
 *   不比较、不推断（线上本来就没有摘要可比）。
 * - 装：确认卡由本机后端当枢纽拼（`ext-hub-preview`），确认时交回卡上的记号（`ext-hub-apply`）；看过之后变了 ⇒ 后端答 `stale`，
 *   卡上说一句、给「重看」。卸：问被卸的那一台（`ext-uninstall-*`）。
 * - 做完之后重读那张表（远端那台先让本机后端对它同步一趟），那一格的点自己变；不轮询。
 */
import { copyText } from "../copy-table";
import { LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { syncAssets } from "../assets-sync-reads";
import { openFileWindow } from "../file-window";
import { resolveRemoteConfigByOrigin } from "../remote-config";
import {
  ExtRefused,
  extHubApply,
  extHubPreview,
  extList,
  extUninstallApply,
  extUninstallPreview,
  type ExtAction,
  type ExtBring,
  type ExtCard,
  type ExtCell,
  type ExtList,
  type ExtLoc,
  type ExtMachine,
  type ExtRow,
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

/** 一格给人看的那句话（悬停 · 抽屉里那一行）。 */
export function stateText(state: ExtCell["state"], places: readonly ExtLoc[]): string {
  switch (state) {
    case "same":
      return copyText("extPage.state.same");
    case "differs":
      return copyText("extPage.state.differs");
    case "missing":
      return copyText("extPage.state.missing");
    case "project":
      return copyText("extPage.state.project", {
        dirs: places.flatMap((p) => (p.level === "project" ? [p.dir] : [])).join(copyText("extPage.list.sep")),
      });
  }
}

/** 那个按钮上的字。 */
export function buttonText(a: ExtAction): string {
  switch (a.verb) {
    case "install":
      return copyText("extPage.button.install");
    case "replace":
      return copyText("extPage.button.replace", { machine: nameOfKey(a.from, a.fromName) });
    case "uninstall":
      return copyText("extPage.button.uninstall");
  }
}

/** 枢纽认的键 ⇒ 给人看的名字（`null` = 本机后端这一台）。 */
function nameOfKey(key: string | null, fallback: string): string {
  return key === null ? copyText("extPage.machine.here") : fallback;
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

/** 抽屉里一台机器那一行的临时状态：打开的确认卡 · 在路上 · 那一句出错的话。 */
interface Slot {
  card?: { kind: "bring"; bring: ExtBring; card: ExtCard | null } | { kind: "remove"; at: ExtLoc; card: ExtUninstallCard | null };
  busy?: boolean;
  error?: { text: string; stale: boolean };
  done?: string;
}

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
      line.addEventListener("click", () => {
        this.openKey = `${r.kind}/${r.name}`;
        this.slots.clear();
        this.renderDrawer();
      });
      this.table.appendChild(line);
    }
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
    if (r.about) this.drawer.appendChild(el("p", "ext-drawer-about", r.about));
    if (r.detail.length > 0) {
      const more = el("details", "ext-detail");
      more.appendChild(el("summary", "", copyText("extPage.drawer.detail")));
      for (const d of r.detail) more.appendChild(el("div", "ext-detail-line", copyText("extPage.drawer.detailLine", { label: d.label, value: d.value })));
      this.drawer.appendChild(more);
    }
    r.cells.forEach((c, i) => this.drawer.appendChild(this.machineRow(r, c, list.machines[i])));
  }

  private machineRow(r: ExtRow, c: ExtCell, m: ExtMachine): HTMLElement {
    const key = `${m.here ? "" : (m.key ?? m.name)}`;
    const slot = this.slots.get(key) ?? {};
    const line = el("div", "ext-machine");
    const top = el("div", "ext-machine-top");
    top.appendChild(el("span", DOT_CLASS[c.state], dotOf(c.state)));
    top.appendChild(el("span", "ext-machine-name", machineName(m)));
    top.appendChild(el("span", "ext-machine-state", stateText(c.state, c.places)));
    const act = c.action;
    if (act) {
      const b = button(buttonText(act), () => void this.openCard(r, m, act));
      b.disabled = slot.busy === true || slot.card !== undefined;
      top.appendChild(b);
    }
    if (r.kind === "skill" && c.dir !== null && !m.here && m.key !== null) {
      const dir = c.dir;
      const host = m.key;
      top.appendChild(
        button(copyText("extPage.button.openFiles"), () => {
          void resolveRemoteConfigByOrigin(host).then((cfg) => {
            if (cfg) void openFileWindow(cfg, { dir });
            else this.setSlot(key, { error: { text: copyText("extPage.error.noHostConfig", { machine: machineName(m) }), stale: false } });
          });
        }),
      );
    }
    line.appendChild(top);
    if (!act && c.note) line.appendChild(el("div", "settings-hint", c.note));
    if (slot.done) line.appendChild(el("div", "settings-hint ext-done", slot.done));
    if (slot.error) {
      const err = el("div", "ext-error", copyText("extPage.error.lead", { machine: machineName(m), said: slot.error.text }));
      if (slot.error.stale && act) err.appendChild(button(copyText("extPage.card.again"), () => void this.openCard(r, m, act)));
      line.appendChild(err);
    }
    if (slot.card) line.appendChild(this.cardOf(m, key, slot));
    return line;
  }

  private setSlot(key: string, s: Slot): void {
    this.slots.set(key, s);
    this.renderDrawer();
  }

  /** 点了那一格的按钮：先问一张确认卡（装 = 本机后端当枢纽；卸 = 那一台）。 */
  private async openCard(r: ExtRow, m: ExtMachine, act: ExtAction): Promise<void> {
    const key = `${m.here ? "" : (m.key ?? m.name)}`;
    if (act.verb === "uninstall") {
      this.setSlot(key, { busy: true, card: { kind: "remove", at: act.at, card: null } });
      try {
        const card = await extUninstallPreview(originOf(m), r.kind, r.name, act.at);
        this.setSlot(key, { card: { kind: "remove", at: act.at, card } });
      } catch (e) {
        this.setSlot(key, { error: this.said(e) });
      }
      return;
    }
    const bring: ExtBring = { kind: r.kind, name: r.name, from: act.from, to: m.here ? null : m.key, scope: act.scope };
    await this.previewBring(key, bring);
  }

  private async previewBring(key: string, bring: ExtBring): Promise<void> {
    this.setSlot(key, { busy: true, card: { kind: "bring", bring, card: null } });
    try {
      const card = await extHubPreview(bring);
      this.setSlot(key, { card: { kind: "bring", bring, card } });
    } catch (e) {
      this.setSlot(key, { error: this.said(e) });
    }
  }

  private said(e: unknown): { text: string; stale: boolean } {
    return e instanceof ExtRefused ? { text: e.message, stale: e.code === "stale" } : { text: e instanceof Error ? e.message : String(e), stale: false };
  }

  private cardOf(m: ExtMachine, key: string, slot: Slot): HTMLElement {
    const box = el("div", "ext-card");
    const c = slot.card;
    if (!c || c.card === null) {
      box.appendChild(el("div", "settings-hint", copyText("extPage.card.loading")));
      return box;
    }
    const cancel = button(copyText("extPage.card.cancel"), () => this.setSlot(key, {}));
    if (c.kind === "remove") {
      const card = c.card;
      box.appendChild(el("div", "ext-card-title", copyText("extPage.card.removeTitle", { machine: machineName(m), path: card.path })));
      box.appendChild(el("div", "ext-card-said", card.said));
      const files = el("ul", "ext-card-files");
      for (const f of card.files) files.appendChild(el("li", "", f));
      box.appendChild(files);
      const ok = button(
        copyText("extPage.card.confirmRemove"),
        () =>
          void this.run(key, m, async () => {
            const done = await extUninstallApply(originOf(m), card, c.at);
            return done.note ?? copyText("extPage.done.removed", { n: String(done.changed.length) });
          }),
        true,
      );
      ok.disabled = slot.busy === true;
      box.appendChild(el("div", "ext-card-buttons")).append(ok, cancel);
      return box;
    }
    const card = c.card;
    const from = this.list?.machines.find((x) => (c.bring.from === null ? x.here : x.key === c.bring.from));
    box.appendChild(
      el(
        "div",
        "ext-card-title",
        copyText("extPage.card.bringTitle", { from: from ? machineName(from) : copyText("extPage.machine.here"), to: machineName(m) }),
      ),
    );
    if (c.bring.scope.to.level === "project") {
      const pick = el("select", "ext-card-project");
      pick.setAttribute("aria-label", copyText("extPage.card.projectAria"));
      const now = c.bring.scope.to.dir;
      for (const d of m.projects) {
        const o = el("option", "", d);
        o.value = d;
        o.selected = d === now;
        pick.appendChild(o);
      }
      pick.addEventListener("change", () => {
        void this.previewBring(key, { ...c.bring, scope: { from: c.bring.scope.from, to: { level: "project", dir: pick.value } } });
      });
      const row = el("label", "ext-card-field", copyText("extPage.card.project"));
      row.appendChild(pick);
      box.appendChild(row);
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
        void this.run(key, m, async () => {
          const done = await extHubApply(c.bring, card, fill);
          return done.note ?? copyText("extPage.done.written", { n: String(done.changed.length) });
        }),
      true,
    );
    ok.disabled = slot.busy === true || card.stop !== null || card.unchanged;
    box.appendChild(el("div", "ext-card-buttons")).append(ok, cancel);
    return box;
  }

  /** 确认之后：做 → 成了就重读那张表（那台先同步一趟），那一格的点自己变；没成就在那一行说。 */
  private async run(key: string, m: ExtMachine, go: () => Promise<string>): Promise<void> {
    const keep = this.slots.get(key) ?? {};
    this.setSlot(key, { ...keep, busy: true, error: undefined });
    try {
      const said = await go();
      this.setSlot(key, { done: said });
      await this.reload(false, m.here ? null : originOf(m));
    } catch (e) {
      this.setSlot(key, { ...keep, busy: false, error: this.said(e) });
    }
  }
}
