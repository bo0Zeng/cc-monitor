/**
 * 命令面板（`Ctrl+K`）：上方居中的模态面板（kit `panelDialog`，宽 600、距顶 90px），一个框搜命令、会话与机器。
 *
 * - 空输入时分组：需手动（有才出）· 当前会话 · 机器（每台一行，Enter 只把名字填进框）· 打开 · 窗口 · 账号。
 *   有输入时：会话在前、命令在后（子串匹配标题 · 项目 · 机器 · 关键词）；打的字恰好是一台 ⇒ 那台一块排最前
 *   （头 · 这台能做的几样 · 这台的会话），块里列过的会话块下面不再重复。
 * - 会话行：状态点 ＋ 机器徽标（只出一次）＋ 项目 ＋ 标题 ＋ 在等什么；右侧它的数字键（1–9 内）。
 * - 不可用的项：灰，第二行写为什么。
 * - 写动作不列：结束 · 删除 · 恢复这类有后果的动作在标签页右键里（底栏一句）。这是裁定（U2），明确不做，不是漏做。
 * - 键盘：↑↓ 走 · Enter 执行并关 · Shift+Enter 做那一行的第二样（会话：在终端里打开）· Esc 关 · Tab 不出框；输入法组字时这几个键归输入法。
 *
 * 只排版：哪几条、可不可用、为什么由调用方（`main.ts`）给，机器那一块由 {@link MachineSource} 给；`filterCommands` · `sections` 是纯函数。
 */
import { imeComposing } from "../keybindings/ime";
import { panelDialog, type PanelHandle } from "../kit/dialog";
import { icon, type IconName } from "../kit/icon";
import { kbd, tag } from "../kit/badge";
import { statusDot, type DotState, type MachineDotState } from "../kit/status-dot";
import { copyText } from "../copy-table";
import s from "./command-bar.module.css";

/** 分组（空输入时按这个顺序列；有输入时只分「会话」「命令」，打中一台另排最前一块）。 */
export type CommandGroup = "needs" | "current" | "machine" | "open" | "window" | "account";

/** 会话行要画的那几样（标签页栏同一份事实）。 */
export interface SessionFace {
  /** 还没收到这条会话的脸 ⇒ `null`（`statusDot` 不画）。 */
  dot: DotState | null;
  dotLabel: string | null;
  /** 远端那台的名字；本机 ⇒ `null`（不画徽标）。 */
  machine: string | null;
  project: string | null;
  title: string;
  /** 在等你什么（`等批准`）；不在等 ⇒ `null`。 */
  waiting: string | null;
  /** 在哪台（机器名，本机 `<local>`）：打中一台时按它挑出那台的会话。 */
  origin?: string;
}

/** 「机器」那一块的头（不可选）：状态点 ＋ 名字 ＋ 那台成品的词 ＋ 问题行那一句 ＋ 右侧（系统 / 出处）。 */
export interface MachineHeadFace {
  dot: MachineDotState | null;
  name: string;
  word: string;
  said: string;
  right: string;
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
  /** 标题后那一段灰字（`ssh nas · 登录 shell`）。 */
  sub?: string;
  /** 可用时的第二行提醒（`终端照开`）。 */
  note?: string;
  /** 行首的状态点（「机器」那一段每台一行）。 */
  dot?: MachineDotState | null;
  /** 给了 ⇒ Enter 不执行、不关框，只把这串填进框（机器行 ⇒ 进那台的一块）。 */
  fill?: string;
  /** Shift+Enter 做的那一样（会话行：在终端里打开）。 */
  alt?: () => void;
  /** 「机器」那一块的头（给了就按头画、不可选）。 */
  machine?: MachineHeadFace;
  run: () => void;
}

/** 打中一台时排最前的那一块：几段 · 默认选中哪一条（`id`）· 块里已列的会话（块下面不再重复）。 */
export interface MachineBlock {
  sections: { title: string; items: Command[] }[];
  select: string | null;
  claimed: ReadonlySet<string>;
}

/** 面板问后端的那一份（`machine-commands.ts`）：空输入那一段每台一行 · 打的字打中一台时那一块。 */
export interface MachineSource {
  rows(): Promise<Command[]>;
  block(query: string, sessions: readonly Command[]): Promise<MachineBlock | null>;
}

/** 产品里那一份：用到时才载（面板本身不依赖起会话 · 文件窗口那几处）。 */
const productMachines: MachineSource = {
  rows: async () => {
    const m = await import("../machine-commands");
    return m.machineSource(m.machineActions).rows();
  },
  block: async (q, sessions) => {
    const m = await import("../machine-commands");
    return m.machineSource(m.machineActions).block(q, sessions);
  },
};

/** 能不能被选中：灰着的、机器块的头都不行。 */
function selectable(c: Command): boolean {
  return !c.disabled && !c.machine;
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

const GROUP_ORDER: readonly CommandGroup[] = ["needs", "current", "machine", "open", "window", "account"];

/** 每一段的标题（骨架字，不随数据变）。 */
const GROUP_TITLE: Record<CommandGroup | "sessions" | "commands", () => string> = {
  needs: () => copyText("beSession.needs.unknown"),
  current: () => copyText("commandBar.group.current"),
  machine: () => copyText("detail.label.machine"),
  open: () => copyText("mobTerms.row.attach"),
  window: () => copyText("commandBar.group.window"),
  account: () => copyText("beProfile.slot.account"),
  sessions: () => copyText("mobDrawer.item.sessions"),
  commands: () => copyText("detail.label.command"),
};

function groupTitle(g: CommandGroup | "sessions" | "commands"): string {
  return GROUP_TITLE[g]();
}

/**
 * 这一刻要列的段：空输入按分组（会话只列在等你的）；有输入时会话一段、命令一段（机器行只在空输入时列）；
 * 给了打中的那一块 ⇒ 它排最前，块里列过的不再重复。纯函数。
 */
export function sections(cmds: Command[], query: string, block: MachineBlock | null = null): { title: string; items: Command[] }[] {
  if (!query.trim()) {
    return GROUP_ORDER.map((g) => ({
      title: groupTitle(g),
      items: cmds.filter((c) => (g === "needs" ? c.session?.waiting != null : !c.session && (c.group ?? "open") === g)),
    })).filter((sec) => sec.items.length > 0);
  }
  const hit = filterCommands(
    cmds.filter((c) => c.group !== "machine" && !block?.claimed.has(c.id)),
    query,
  );
  return [
    ...(block?.sections ?? []),
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
  /** 此刻框里那串打中的那一块（回包到了才有）。 */
  private block: MachineBlock | null = null;
  /** 问机器那一块的第几问：迟到的回包（框里的字已经变了）丢掉。 */
  private asked = 0;

  constructor(
    private listCommands: () => Command[],
    private machines: MachineSource | null = productMachines,
  ) {}

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
    head.append(this.input, kbd(copyText("kit.dock.escKey")));

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
    this.block = null;
    this.handle = panelDialog({
      label: copyText("mobKeys.action.palette"),
      size: "palette",
      content: [head, this.listEl, foot],
      first: this.input,
      onClose: () => {
        this.handle = null;
      },
    });
    this.applyFilter();
    const handle = this.handle;
    this.machines?.rows().then(
      (rows) => {
        if (this.handle !== handle) return;
        this.allCommands = [...this.allCommands, ...rows];
        if (!this.input.value.trim()) this.render(null);
      },
      (e: unknown) => console.warn("command-bar machine rows:", e),
    );
  }

  close(): void {
    this.handle?.close();
  }

  /** 改字：先照手上的画，再问打没打中一台（迟到的丢掉）。 */
  private applyFilter(): void {
    const query = this.input.value;
    const n = ++this.asked;
    if (!query.trim() || !this.machines) {
      this.block = null;
      this.render(null);
      return;
    }
    this.render(null);
    const handle = this.handle;
    this.machines.block(query, this.allCommands.filter((c) => c.session)).then(
      (b) => {
        if (n !== this.asked || this.handle !== handle) return;
        this.block = b;
        this.render(b?.select ?? null);
      },
      (e: unknown) => console.warn("command-bar machine block:", e),
    );
  }

  private render(selectId: string | null): void {
    const secs = sections(this.allCommands, this.input.value, this.input.value.trim() ? this.block : null);
    const prev = this.shown[this.selected]?.id ?? null;
    this.listEl.replaceChildren();
    this.shown = [];
    this.rows = [];
    if (secs.length === 0) {
      const empty = document.createElement("div");
      empty.className = s.cbEmpty;
      empty.textContent = copyText("history.list.noMatch");
      this.listEl.appendChild(empty);
      return;
    }
    for (const sec of secs) {
      if (sec.title) {
        const h = document.createElement("div");
        h.className = s.cbGroup;
        h.textContent = sec.title;
        this.listEl.appendChild(h);
      }
      for (const cmd of sec.items) {
        const row = this.row(cmd, this.shown.length);
        this.shown.push(cmd);
        this.rows.push(row);
        this.listEl.appendChild(row);
      }
    }
    // 选中哪一条：块指定的 → 回包到之前选着的那条（还在的话；不让选中跳）→ 第一条能选的。
    const at = (id: string | null): number => (id === null ? -1 : this.shown.findIndex((c) => c.id === id && selectable(c)));
    const keep = selectId === null ? at(prev) : -1;
    this.selected = [at(selectId), keep, this.shown.findIndex(selectable)].find((i) => i >= 0) ?? 0;
    this.mark();
  }

  private row(cmd: Command, index: number): HTMLElement {
    const item = document.createElement("div");
    item.className = cmd.machine ? `${s.cbItem} ${s.cbMachineHead}` : s.cbItem;
    item.dataset.role = cmd.machine ? "command-machine-head" : "command-item";
    item.setAttribute("role", "option");
    if (!selectable(cmd)) item.setAttribute("aria-disabled", "true");
    const main = document.createElement("div");
    main.className = s.cbMain;
    if (cmd.machine) {
      const f = cmd.machine;
      main.appendChild(statusDot(f.dot, f.word || null, "compact"));
      const text = document.createElement("span");
      text.className = s.cbText;
      const line = document.createElement("span");
      const name = document.createElement("span");
      name.className = s.cbMachineName;
      name.textContent = f.name;
      line.appendChild(name);
      if (f.word) {
        const w = document.createElement("span");
        w.className = s.cbSub;
        w.textContent = ` ${f.word}`;
        line.appendChild(w);
      }
      text.appendChild(line);
      if (f.said) {
        const why = document.createElement("span");
        why.className = s.cbWhy;
        why.textContent = f.said;
        text.appendChild(why);
      }
      main.appendChild(text);
      item.appendChild(main);
      if (f.right) {
        const r = document.createElement("span");
        r.className = s.cbRight;
        r.textContent = f.right;
        item.appendChild(r);
      }
      return item;
    }
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
      if (cmd.alt) {
        const a = document.createElement("span");
        a.className = s.cbAlt;
        a.textContent = copyText("commandBar.machine.altHint");
        main.appendChild(a);
      }
    } else {
      if (cmd.dot !== undefined) main.appendChild(statusDot(cmd.dot, cmd.sub ?? null, "compact"));
      else main.appendChild(icon(cmd.icon ?? "command"));
      const text = document.createElement("span");
      text.className = s.cbText;
      const line = document.createElement("span");
      const t = document.createElement("span");
      t.className = s.cbTitle;
      t.textContent = cmd.title;
      line.appendChild(t);
      if (cmd.sub) {
        const sub = document.createElement("span");
        sub.className = s.cbSub;
        sub.textContent = ` ${cmd.sub}`;
        line.appendChild(sub);
      }
      text.appendChild(line);
      const second = cmd.disabled ?? cmd.note;
      if (second) {
        const why = document.createElement("span");
        why.className = s.cbWhy;
        why.textContent = second;
        text.appendChild(why);
      }
      main.appendChild(text);
    }
    item.appendChild(main);
    if (cmd.hint) item.appendChild(kbd(cmd.hint));
    if (cmd.fill !== undefined) item.appendChild(icon("caretRight"));
    item.addEventListener("mousedown", (e) => {
      e.preventDefault(); // 别让输入框失焦
      this.run(index, false);
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
      if (selectable(this.shown[i])) break;
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
      this.run(this.selected, e.shiftKey);
    }
  }

  private run(index: number, alt: boolean): void {
    const cmd = this.shown[index];
    if (!cmd || !selectable(cmd)) return;
    if (cmd.fill !== undefined) {
      this.input.value = cmd.fill;
      this.applyFilter();
      return;
    }
    const act = alt && cmd.alt ? cmd.alt : cmd.run;
    this.close(); // 先关（出弹层栈），再执行 —— 命令可能自己开一层（历史）
    try {
      act();
    } catch (e) {
      console.warn("command-bar run failed:", e);
    }
  }
}
