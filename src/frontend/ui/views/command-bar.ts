/**
 * 命令面板（`Ctrl+K`）：上方居中的模态面板（kit `panelDialog`，宽 600、距顶 90px），一个框搜命令与会话。
 *
 * - 空输入时分组：需要你（有才出）· 当前会话 · 打开 · 窗口 · 账号。有输入时：会话在前、命令在后（子串匹配标题 · 项目 · 机器 · 关键词）。
 * - 会话行：状态点 ＋ 机器徽标（只出一次）＋ 项目 ＋ 标题 ＋ 在等什么；右侧它的数字键（1–9 内）。
 * - 不可用的项：灰，第二行写为什么。
 * - 写动作不列：结束 · 删除 · 恢复这类有后果的动作在标签页右键里（底栏一句）。这是裁定（U2），明确不做，不是漏做。
 * - 键盘：↑↓ 走 · Enter 执行并关 · Esc 关 · Tab 不出框；输入法组字时这几个键归输入法。
 *
 * 只排版：哪几条、可不可用、为什么由调用方（`main.ts`）给；`filterCommands` 是纯函数。
 */
import { imeComposing } from "../keybindings/ime";
import { panelDialog, type PanelHandle } from "../kit/dialog";
import { icon, type IconName } from "../kit/icon";
import { kbd, tag } from "../kit/badge";
import { statusDot, type DotState } from "../kit/status-dot";
import { copyText } from "../copy-table";
import s from "./command-bar.module.css";

/** 分组（空输入时按这个顺序列；有输入时只分「会话」「命令」）。 */
export type CommandGroup = "needs" | "current" | "open" | "window" | "account";

/** 会话行要画的那几样（标签页栏同一份事实）。 */
export interface SessionFace {
  dot: DotState;
  dotLabel: string;
  /** 远端那台的名字；本机 ⇒ `null`（不画徽标）。 */
  machine: string | null;
  project: string | null;
  title: string;
  /** 在等你什么（`等批准`）；不在等 ⇒ `null`。 */
  waiting: string | null;
}

export interface Command {
  id: string;
  /** 展示名（也是主匹配字段）。 */
  title: string;
  /** 附加匹配关键词（空格分隔，不展示）。 */
  keywords?: string;
  /** 右侧：当前键位（会话行是它的数字键）。 */
  hint?: string;
  icon?: IconName;
  /** 命令归哪一组；会话行不给。 */
  group?: CommandGroup;
  /** 会话行（给了就按会话画、按会话排）。 */
  session?: SessionFace;
  /** 不可用：第二行写为什么，点了不做事。 */
  disabled?: string;
  run: () => void;
}

/**
 * 子串过滤 ＋ 排序。大小写不敏感。空 query → 原序返回全部。
 * 排序档：标题前缀命中(0) > 标题子串命中(1) > 仅 keywords 命中(2)；同档保原序（稳定）。纯函数。
 */
export function filterCommands(cmds: Command[], query: string): Command[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...cmds];
  const rank = (c: Command): number => {
    const title = c.title.toLowerCase();
    if (title.startsWith(q)) return 0;
    if (title.includes(q)) return 1;
    if ((c.keywords ?? "").toLowerCase().includes(q)) return 2;
    return 3; // 不命中
  };
  return cmds
    .map((c, i) => ({ c, i, r: rank(c) }))
    .filter((x) => x.r < 3)
    .sort((a, b) => a.r - b.r || a.i - b.i)
    .map((x) => x.c);
}

const GROUP_ORDER: readonly CommandGroup[] = ["needs", "current", "open", "window", "account"];

function groupTitle(g: CommandGroup | "sessions" | "commands"): string {
  switch (g) {
    case "needs":
      return copyText("commandBar.group.needs");
    case "current":
      return copyText("commandBar.group.current");
    case "open":
      return copyText("commandBar.group.open");
    case "window":
      return copyText("commandBar.group.window");
    case "account":
      return copyText("commandBar.group.account");
    case "sessions":
      return copyText("commandBar.group.sessions");
    case "commands":
      return copyText("commandBar.group.commands");
  }
}

/** 这一刻要列的段：空输入按分组（会话只列在等你的）；有输入时会话一段、命令一段。纯函数。 */
export function sections(cmds: Command[], query: string): { title: string; items: Command[] }[] {
  if (!query.trim()) {
    return GROUP_ORDER.map((g) => ({
      title: groupTitle(g),
      items: cmds.filter((c) => (g === "needs" ? c.session?.waiting != null : !c.session && (c.group ?? "open") === g)),
    })).filter((sec) => sec.items.length > 0);
  }
  const hit = filterCommands(cmds, query);
  return [
    { title: groupTitle("sessions"), items: hit.filter((c) => c.session) },
    { title: groupTitle("commands"), items: hit.filter((c) => !c.session) },
  ].filter((sec) => sec.items.length > 0);
}

export class CommandBarView {
  private input!: HTMLInputElement;
  private listEl!: HTMLElement;
  private handle: PanelHandle | null = null;
  /** 本次打开时的全表（打开那一刻快照：打字中途会话增减不让列表跳）。 */
  private allCommands: Command[] = [];
  /** 当前列着的、能被选中的那几条（与列表 DOM 同步）。 */
  private shown: Command[] = [];
  private rows: HTMLElement[] = [];
  private selected = 0;

  constructor(private listCommands: () => Command[]) {}

  isVisible(): boolean {
    return this.handle !== null;
  }

  toggle(): void {
    if (this.handle) this.close();
    else this.open();
  }

  open(): void {
    if (this.handle) return;
    const head = document.createElement("div");
    head.className = s.cbHead;
    head.appendChild(icon("search"));
    this.input = document.createElement("input");
    this.input.className = s.cbInput;
    this.input.dataset.role = "command-input";
    this.input.type = "text";
    this.input.placeholder = copyText("commandBar.build.placeholder");
    this.input.setAttribute("aria-label", copyText("commandBar.build.placeholder"));
    this.input.addEventListener("input", () => this.applyFilter());
    this.input.addEventListener("keydown", (e) => this.onInputKeydown(e));
    head.append(this.input, kbd(copyText("commandBar.key.esc")));

    this.listEl = document.createElement("div");
    this.listEl.className = s.cbList;
    this.listEl.setAttribute("role", "listbox");

    const foot = document.createElement("div");
    foot.className = s.cbFoot;
    const keys = document.createElement("span");
    keys.textContent = copyText("commandBar.foot.keys");
    const noWrites = document.createElement("span");
    noWrites.textContent = copyText("commandBar.foot.noWrites");
    foot.append(keys, noWrites);

    this.allCommands = this.listCommands();
    this.handle = panelDialog({
      label: copyText("commandBar.build.ariaLabel"),
      size: "palette",
      content: [head, this.listEl, foot],
      first: this.input,
      onClose: () => {
        this.handle = null;
      },
    });
    this.applyFilter();
  }

  close(): void {
    this.handle?.close();
  }

  private applyFilter(): void {
    const secs = sections(this.allCommands, this.input.value);
    this.listEl.replaceChildren();
    this.shown = [];
    this.rows = [];
    if (secs.length === 0) {
      const empty = document.createElement("div");
      empty.className = s.cbEmpty;
      empty.textContent = copyText("commandBar.renderList.none");
      this.listEl.appendChild(empty);
      return;
    }
    for (const sec of secs) {
      const h = document.createElement("div");
      h.className = s.cbGroup;
      h.textContent = sec.title;
      this.listEl.appendChild(h);
      for (const cmd of sec.items) {
        const row = this.row(cmd, this.shown.length);
        this.shown.push(cmd);
        this.rows.push(row);
        this.listEl.appendChild(row);
      }
    }
    this.selected = Math.max(0, this.shown.findIndex((c) => !c.disabled));
    this.mark();
  }

  private row(cmd: Command, index: number): HTMLElement {
    const item = document.createElement("div");
    item.className = s.cbItem;
    item.dataset.role = "command-item";
    item.setAttribute("role", "option");
    if (cmd.disabled) item.setAttribute("aria-disabled", "true");
    const main = document.createElement("div");
    main.className = s.cbMain;
    if (cmd.session) {
      const f = cmd.session;
      main.appendChild(statusDot(f.dot, f.dotLabel, "compact"));
      if (f.machine) main.appendChild(tag(f.machine));
      if (f.project) {
        const p = document.createElement("span");
        p.className = s.cbProject;
        p.textContent = f.project;
        main.appendChild(p);
      }
      const t = document.createElement("span");
      t.className = f.project ? s.cbSub : s.cbTitle;
      t.textContent = f.title;
      main.appendChild(t);
      if (f.waiting) {
        const w = document.createElement("span");
        w.className = s.cbWaiting;
        w.textContent = copyText("commandBar.session.waiting", { what: f.waiting });
        main.appendChild(w);
      }
    } else {
      main.appendChild(icon(cmd.icon ?? "command"));
      const text = document.createElement("span");
      text.className = s.cbText;
      const t = document.createElement("span");
      t.className = s.cbTitle;
      t.textContent = cmd.title;
      text.appendChild(t);
      if (cmd.disabled) {
        const why = document.createElement("span");
        why.className = s.cbWhy;
        why.textContent = cmd.disabled;
        text.appendChild(why);
      }
      main.appendChild(text);
    }
    item.appendChild(main);
    if (cmd.hint) item.appendChild(kbd(cmd.hint));
    item.addEventListener("mousedown", (e) => {
      e.preventDefault(); // 别让输入框失焦
      this.run(index);
    });
    return item;
  }

  private mark(): void {
    this.rows.forEach((el, i) => el.setAttribute("aria-selected", String(i === this.selected)));
    this.rows[this.selected]?.scrollIntoView?.({ block: "nearest" });
  }

  private moveSelection(delta: number): void {
    const n = this.shown.length;
    if (n === 0) return;
    let i = this.selected;
    for (let k = 0; k < n; k++) {
      i = (i + delta + n) % n;
      if (!this.shown[i].disabled) break;
    }
    this.selected = i;
    this.mark();
  }

  private onInputKeydown(e: KeyboardEvent): void {
    if (imeComposing(e)) return; // 组字中的 Enter / 方向键归输入法
    // 开着时再按 Ctrl+K 关：模态压栈后快捷键只放行 Esc，这一键由框自己接。
    if (e.ctrlKey && e.code === "KeyK") {
      e.preventDefault();
      this.close();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      this.moveSelection(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      this.moveSelection(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      this.run(this.selected);
    }
  }

  private run(index: number): void {
    const cmd = this.shown[index];
    if (!cmd || cmd.disabled) return;
    this.close(); // 先关（出弹层栈），再执行 —— 命令可能自己开一层（历史）
    try {
      cmd.run();
    } catch (e) {
      console.warn("command-bar run failed:", e);
    }
  }
}
