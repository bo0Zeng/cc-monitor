/**
 * F84（#57）：键盘命令栏（⌘K/Ctrl-K 命令面板）。**加分项**，守只读铁律 + 北极星。
 *
 * body-level overlay（照 HistoryView 范式）：输入框 + 子串过滤命令列表 + 方向键选 + 回车执行 + Esc/点背景关。
 * **首刀只列只读命令**（开 overlay / 窗口操作 / 导航——切会话/切 tab）；resume/new-session/attach/kill/delete 等
 * **写/驱动动作首刀排除**（守北极星，延后须 danger + 二次确认）。命令列表由 main.ts 组装后注入（`listCommands`），
 * 复用既有 view.open()/dispatcher 目标 + F91 `snapshotSessions()` 喂「切到会话…」。
 *
 * `filterCommands` 纯函数抽出可测；无 fuzzy（子串够用、与既有过滤一致）。全 textContent，无 innerHTML。
 */
import { dispatcher } from "../keybindings/registry";

export interface Command {
  id: string;
  /** 展示名（也是主匹配字段）。 */
  title: string;
  /** 附加匹配关键词（空格分隔，不展示）。 */
  keywords?: string;
  /** 右侧提示（该命令对应的快捷键，如 `H`）——把冗余入口变成快捷键教学卡（业务二审 gap#3）。 */
  hint?: string;
  /** 执行体（只读命令：开 overlay / 导航 / 窗口操作）。 */
  run: () => void;
}

/**
 * 子串过滤 + 排序。大小写不敏感。空 query → 原序返回全部。
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

export class CommandBarView {
  private root: HTMLElement;
  private input!: HTMLInputElement;
  private listEl!: HTMLElement;
  private isOpen = false;
  /** 本次 open 的命令全表（open 时快照一次，避免每次击键重建 + 打字中途会话增减致列表跳动）。 */
  private allCommands: Command[] = [];
  /** 当前过滤结果（与列表 DOM 同步）。 */
  private filtered: Command[] = [];
  /** 选中项在 filtered 中的下标。 */
  private selected = 0;

  constructor(private listCommands: () => Command[]) {
    this.root = this.build();
  }

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "command-bar";
    // 点背景（非 box 内）关闭
    root.addEventListener("mousedown", (e) => {
      if (e.target === root) this.close();
    });

    const box = document.createElement("div");
    box.className = "command-bar-box";

    this.input = document.createElement("input");
    this.input.className = "command-bar-input";
    this.input.type = "text";
    this.input.placeholder = "输入命令 / 会话名…（↑↓ 选择，回车执行，Esc 关闭）";
    this.input.setAttribute("aria-label", "命令栏");
    this.input.addEventListener("input", () => this.applyFilter());
    this.input.addEventListener("keydown", (e) => this.onInputKeydown(e));
    box.appendChild(this.input);

    this.listEl = document.createElement("div");
    this.listEl.className = "command-bar-list";
    box.appendChild(this.listEl);

    root.appendChild(box);
    return root;
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  handleEsc(): void {
    this.close();
  }

  toggle(): void {
    if (this.isOpen) this.close();
    else this.open();
  }

  open(): void {
    if (this.isOpen) return;
    document.body.appendChild(this.root);
    this.isOpen = true;
    dispatcher.pushOverlay(this);
    this.input.value = "";
    this.allCommands = this.listCommands(); // 本次 open 快照一次
    this.applyFilter(); // 渲染全表
    this.input.focus();
  }

  close(): void {
    if (!this.isOpen) return;
    this.root.remove();
    this.isOpen = false;
    dispatcher.popOverlay(this);
  }

  private applyFilter(): void {
    this.filtered = filterCommands(this.allCommands, this.input.value);
    this.selected = 0;
    this.renderList();
  }

  private renderList(): void {
    this.listEl.replaceChildren();
    if (this.filtered.length === 0) {
      const empty = document.createElement("div");
      empty.className = "command-bar-empty";
      empty.textContent = "无匹配命令";
      this.listEl.appendChild(empty);
      return;
    }
    this.filtered.forEach((cmd, i) => {
      const item = document.createElement("div");
      item.className = "command-bar-item";
      if (i === this.selected) item.classList.add("selected");
      const titleEl = document.createElement("span");
      titleEl.className = "command-bar-item-title";
      titleEl.textContent = cmd.title;
      item.appendChild(titleEl);
      if (cmd.hint) {
        const hintEl = document.createElement("span");
        hintEl.className = "command-bar-item-hint";
        hintEl.textContent = cmd.hint; // 该命令的快捷键（教学式发现）
        item.appendChild(hintEl);
      }
      item.addEventListener("mousedown", (e) => {
        e.preventDefault(); // 别让输入框失焦
        this.run(i);
      });
      this.listEl.appendChild(item);
    });
  }

  private moveSelection(delta: number): void {
    if (this.filtered.length === 0) return;
    const n = this.filtered.length;
    this.selected = (this.selected + delta + n) % n;
    // 只更新高亮 + 滚动到可见，不整表重建
    const items = this.listEl.querySelectorAll<HTMLElement>(".command-bar-item");
    items.forEach((el, i) => el.classList.toggle("selected", i === this.selected));
    // 可选调用：jsdom（测试环境）无 scrollIntoView，真实 webview 有。
    items[this.selected]?.scrollIntoView?.({ block: "nearest" });
  }

  private onInputKeydown(e: KeyboardEvent): void {
    // Ctrl+K 再按关闭：dispatcher 的 app.open-command-bar 在输入框聚焦时被可编辑目标守卫拦掉
    // （registry.ts 只放行 overlay.close），故 toggle 的关分支键盘不可达——在此本地兜住。
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
    // Esc 交给 dispatcher overlay 栈（overlay.close → handleEsc），不在此处理。
  }

  private run(index: number): void {
    const cmd = this.filtered[index];
    if (!cmd) return; // 空列表 / 越界 → no-op
    this.close(); // 先关命令栏（popOverlay），再执行——命令可能自己 pushOverlay（如开历史）
    try {
      cmd.run();
    } catch (e) {
      console.warn("command-bar run failed:", e); // run 回调异常不逸出 keydown 处理器
    }
  }
}
