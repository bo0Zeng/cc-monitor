/**
 * 快捷键一览（`?` · 命令面板「快捷键一览」）：kit 面板形 `sheet`（640 宽）。
 *
 * 顶上标题 ＋ 过滤框（按名字或按键找）；表三列（动作 · 当前键 · 何时生效），按「会话之间 · 当前会话 · 窗口与应用 · 未设键」分组；
 * 底栏一句单键规则 ＋［改快捷键…］（直达设置那一节）［关闭］。Esc / Enter 关。键位一律按当前键位现拼。
 */
import { dispatcher, KeybindingDispatcher } from "../keybindings/registry";
import { findAction, type ActionId, type Scope } from "../keybindings/actions";
import { panelDialog, type PanelHandle } from "../kit/dialog";
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { kbd } from "../kit/badge";
import { copyText } from "../copy-table";
import s from "./keys-overview.module.css";

interface Row {
  /** 一行写几个动作的就给那一行自己的名字；单个动作 ⇒ 用动作名。 */
  label?: string;
  ids: readonly ActionId[];
  /** `1 – 9` 这种连着的一串：只画头尾。 */
  range?: boolean;
}

function groups(): { title: string; rows: Row[] }[] {
  return [
    {
      title: copyText("keys.group.between"),
      rows: [
        { label: copyText("keys.row.jump"), ids: ["tab.jump-1", "tab.jump-9"], range: true },
        { label: copyText("keys.row.nextPrev"), ids: ["tab.next", "tab.prev"] },
        { ids: ["needs.next"] },
        { ids: ["app.open-command-bar"] },
      ],
    },
    {
      title: copyText("keys.group.current"),
      rows: [
        { ids: ["session.find"] },
        { label: copyText("keys.row.turns"), ids: ["session.prev-turn", "session.next-turn"] },
        { ids: ["session.to-bottom"] },
        { ids: ["session.toggle-process"] },
        { ids: ["panel.toggle-terminal"] },
        { label: copyText("keys.row.panels"), ids: ["panel.toggle-tasks", "panel.toggle-agents"] },
        { label: copyText("keys.row.front"), ids: ["terminal.bring-front"] },
        { ids: ["tab.open-cwd"] },
        { ids: ["tab.pop-out"] },
        { ids: ["tab.close-archived"] },
        { ids: ["tab.context-menu"] },
      ],
    },
    {
      title: copyText("keys.group.app"),
      rows: [
        { label: copyText("keys.row.historySettings"), ids: ["app.toggle-history", "app.open-settings"] },
        { ids: ["app.keys"] },
        { ids: ["app.toggle-fullscreen"] },
        { ids: ["app.minimize"] },
        { label: copyText("keys.row.zoom"), ids: ["app.zoom-in", "app.zoom-out", "app.zoom-reset"] },
        { ids: ["overlay.close"] },
        { ids: ["app.undo"] },
      ],
    },
    {
      title: copyText("keys.group.unbound"),
      rows: [
        { ids: ["behavior.toggle-auto-follow"] },
        { ids: ["behavior.toggle-bring-monitor"] },
        { ids: ["app.open-cc-bus"] },
        { ids: ["account.switch-default"] },
        { ids: ["app.toggle-tab-bar"] },
      ],
    },
  ];
}

function scopeText(scope: Scope, chord: string | null): string {
  const modified = chord !== null && chord.split("+").some((p) => p === "Ctrl" || p === "Alt" || p === "Meta");
  switch (scope === "bare" && modified ? "any" : scope) {
    case "any":
      return copyText("keys.scope.any");
    case "idle":
      return copyText("keys.scope.idle");
    case "main":
      return copyText("keys.scope.main");
    case "nav":
      return copyText("keys.scope.nav");
    case "bare":
      return copyText("keys.scope.bare");
  }
}

/** 一行此刻的键（现拼）：`[[键…]…]`，每个动作一组；没绑 ⇒ 空组。 */
function chordsOf(row: Row): string[][] {
  if (row.range) {
    const [from, to] = row.ids.map((id) => dispatcher.effectiveChord(id));
    if (!from || !to) return [[]];
    return [[copyText("keys.key.range", { from: KeybindingDispatcher.prettyChord(from), to: KeybindingDispatcher.prettyChord(to) })]];
  }
  return row.ids.map((id) => {
    const c = dispatcher.effectiveChord(id);
    const a = findAction(id);
    const out = c ? [KeybindingDispatcher.prettyChord(c)] : [];
    if (a?.also) out.push(copyText("keys.key.menu"));
    return out;
  });
}

export class KeysOverview {
  private handle: PanelHandle | null = null;
  private list!: HTMLElement;
  private filter!: HTMLInputElement;

  constructor(private readonly host: { editKeys: () => void }) {}

  get isOpen(): boolean {
    return this.handle !== null;
  }

  toggle(): void {
    if (this.handle) this.handle.close();
    else this.open();
  }

  open(): void {
    if (this.handle) return;
    const head = document.createElement("div");
    head.className = s.koHead;
    const h = document.createElement("h2");
    h.className = s.koTitle;
    h.textContent = copyText("keys.title.main");
    const box = document.createElement("label");
    box.className = s.koFilter;
    box.appendChild(icon("search", "compact"));
    this.filter = document.createElement("input");
    this.filter.type = "search";
    this.filter.dataset.role = "keys-filter";
    this.filter.placeholder = copyText("keys.search.placeholder");
    this.filter.setAttribute("aria-label", copyText("keys.search.placeholder"));
    this.filter.addEventListener("input", () => this.render());
    box.appendChild(this.filter);
    head.append(h, box);

    this.list = document.createElement("div");
    this.list.className = s.koList;

    const foot = document.createElement("div");
    foot.className = s.koFoot;
    const note = document.createElement("span");
    note.className = s.koNote;
    note.textContent = copyText("keys.foot.bare");
    const edit = button({
      label: copyText("keys.edit.open"),
      onClick: () => {
        this.handle?.close();
        this.host.editKeys();
      },
    });
    const done = button({ label: copyText("keys.close.label"), kind: "primary", onClick: () => this.handle?.close() });
    foot.append(note, edit, done);

    const panel = [head, this.list, foot];
    for (const el of panel) {
      el.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && !e.isComposing && e.target !== edit) {
          e.preventDefault();
          this.handle?.close();
        }
      });
    }
    this.render();
    this.handle = panelDialog({
      label: copyText("keys.title.main"),
      size: "sheet",
      content: panel,
      first: done,
      onClose: () => {
        this.handle = null;
      },
    });
  }

  private render(): void {
    const q = this.filter.value.trim().toLowerCase();
    this.list.replaceChildren();
    for (const g of groups()) {
      const rows = g.rows
        .map((r) => {
          const first = findAction(r.ids[0]);
          const label = r.label ?? first?.label ?? r.ids[0];
          const chords = chordsOf(r);
          return { label, chords, scope: first ? scopeText(first.scope, dispatcher.effectiveChord(r.ids[0])) : "" };
        })
        .filter((r) => !q || r.label.toLowerCase().includes(q) || r.chords.flat().some((c) => c.toLowerCase().includes(q)));
      if (rows.length === 0) continue;
      const head = document.createElement("div");
      head.className = s.koGroup;
      head.textContent = g.title;
      this.list.appendChild(head);
      for (const r of rows) {
        const row = document.createElement("div");
        row.className = s.koRow;
        row.dataset.role = "keys-row";
        const name = document.createElement("span");
        name.textContent = r.label;
        const keys = document.createElement("span");
        keys.className = s.koKeys;
        const all = r.chords.filter((c) => c.length > 0);
        if (all.length === 0) {
          const none = document.createElement("span");
          none.className = s.koNone;
          none.textContent = copyText("registry.prettyChord.unbound");
          keys.appendChild(none);
        } else {
          for (const c of all.flat()) keys.appendChild(kbd(c));
        }
        const scope = document.createElement("span");
        scope.className = s.koScope;
        scope.textContent = all.length === 0 ? "" : r.scope;
        row.append(name, keys, scope);
        this.list.appendChild(row);
      }
    }
  }
}
