/**
 * 设置「数据位置」里界面这一侧自己的那一块：本地存储（localStorage 的 `cc-monitor.` 键）＋ 卸载说明。
 * 数据位置的每一行（本机后端家里那几样 · monitor 数据目录里界面自己那几样 · 网页引擎目录）不在这里：
 * 「文件与数据」页照那台后端 `data-report` 的 `own` 画（`own-block.ts`），本机那一问由本机后端连界面那几样一起答。
 * 纯展示，不做删除 / 清空（用户要的是路径）；
 * `data-section.vitest.ts` 钉着：效应面两向相等（只会打开 / 显示、localStorage 零写）。
 * 卸载不清 `~/.cc-monitor/`。
 */

import { commands } from "../ipc/commands";
import { failToast } from "../kit/toast";
import { enumeratePrefix } from "../local-storage";

// 类型由后端 `footprint/data.rs` 的 `#[derive(ts_rs::TS)]` 生成（`src/frontend/ui/generated/`）。
import type { OwnClass } from "../generated/OwnClass";
import type { OwnItem } from "./data-reads";
import { copyText } from "../copy-table";
import { icon } from "../kit/icon";

/** 每一行那一格「删了会怎样」（后端 `OwnItem.class`）：看得出哪个删了会丢东西。后端加了第三类不许整页炸、原样说出来。 */
export function describeDataClass(c: OwnClass): string {
  switch (c) {
    case "truth":
      return copyText("data.class.keep");
    case "cache":
      return copyText("data.class.disposable");
    default:
      return copyText("data.class.unknown", { kind: String(c) });
  }
}

/**
 * 「删了会怎样」那一格（本机那一块 · 远端那一块同一个）：类别进 DOM（`data-data-class`），判据与用户看的是同一份值；
 * 排在行头、名字与大小之间（`settings.css` 的 `class` 区）。
 */
export function dataClassCell(c: OwnClass): HTMLElement {
  const cell = document.createElement("span");
  cell.className = "settings-data-item-class";
  cell.dataset.dataClass = c;
  cell.textContent = describeDataClass(c);
  return cell;
}

/** 一个 `<strong>`：强调由 DOM 结构承担，不由字符串里的标记承担。 */
function strong(text: string): HTMLElement {
  const el = document.createElement("strong");
  el.textContent = text;
  return el;
}

export class DataSection {
  private root: HTMLElement;
  private headless: boolean;
  private mainBody: HTMLElement;

  /** `headless: true` 时不渲染外层 settings-group 容器（嵌进别的分组时用）。 */
  constructor(opts: { headless?: boolean } = {}) {
    this.headless = !!opts.headless;
    this.root = document.createElement("div");
    this.root.className = this.headless ? "settings-data-headless" : "settings-group";

    if (!this.headless) {
      const heading = document.createElement("div");
      heading.className = "settings-group-title";
      // 叫「数据位置」：内容是路径 ＋ 大小 ＋ 打开，说的是位置；也免得与同组「Claude 数据目录」两个「数据」并排。
      heading.textContent = copyText("data.ctor.title");
      this.root.appendChild(heading);
    }

    this.mainBody = document.createElement("div");
    this.mainBody.className = "settings-data-body";
    this.root.appendChild(this.mainBody);
    // 构造期不读：宿主（`panel.ts`）在该页首次可见时调 `loadNow()`。
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 宿主在「这一页首次可见」时调：照此刻的 localStorage 画（同步，不发 I/O）。 */
  loadNow(): void {
    this.render();
  }

  /** 重新画（设置每次 open 时调，本地存储要最新）。 */
  refresh(): void {
    this.loadNow();
  }

  private render(): void {
    this.mainBody.replaceChildren();

    // 浏览器 localStorage
    this.mainBody.appendChild(this.buildLocalStorageBlock());

    // 卸载说明（纯文字；`<strong>` 由 DOM 建，不拼 innerHTML）
    const note = document.createElement("div");
    note.className = "settings-data-note";
    const info = icon("info", "compact");
    info.classList.add("settings-data-note-icon");
    note.append(
      info,
      strong(copyText("data.note.uninstall")),
      copyText("data.note.byDefault"),
      strong(copyText("data.note.notCleared")),
      copyText("data.note.howToClear"),
      // 那一格怎么读：只想腾空间的话，删「可重建」的就够了。
      copyText("data.note.classes"),
    );
    this.mainBody.appendChild(note);
  }

  private buildLocalStorageBlock(): HTMLElement {
    const keys = collectMonitorLocalStorageKeys();

    const block = document.createElement("div");
    block.className = "settings-data-block";

    const head = document.createElement("div");
    head.className = "settings-data-block-head";
    const title = document.createElement("span");
    title.className = "settings-data-block-title";
    title.textContent = copyText("data.localStorage.title");
    head.appendChild(title);
    const sub = document.createElement("span");
    sub.className = "settings-data-block-subtitle";
    sub.textContent =
      keys.length > 0
        ? copyText("data.localStorage.hint")
        : copyText("data.localStorage.empty");
    head.appendChild(sub);
    block.appendChild(head);

    if (keys.length > 0) {
      const list = document.createElement("ul");
      list.className = "settings-data-list settings-data-ls-list";
      for (const { key, value } of keys) {
        const li = document.createElement("li");
        li.className = "settings-data-item kind-ls";
        const k = document.createElement("code");
        k.className = "settings-data-ls-key";
        k.textContent = key;
        li.appendChild(k);
        const eq = document.createElement("span");
        eq.className = "settings-data-ls-eq";
        eq.textContent = " = ";
        li.appendChild(eq);
        const v = document.createElement("code");
        v.className = "settings-data-ls-value";
        v.textContent = truncateValue(value);
        v.title = value;
        li.appendChild(v);
        list.appendChild(li);
      }
      block.appendChild(list);
    }

    return block;
  }
}

/**
 * 本机一条路径的［打开］（默认程序打开它本身）：本机那一格排在「在文件窗口里打开」前面（`own-block.ts` 的 `openCell`）。
 * 不在盘上 ⇒ 置灰、提示是后端写的 `absent_said`。
 */
export function systemOpenButton(o: OwnItem, path: string): HTMLElement {
  const open = document.createElement("button");
  open.type = "button";
  open.className = "settings-data-item-open";
  open.textContent = o.exists ? copyText("mobTerms.row.attach") : copyText("acct.val.none");
  open.disabled = !o.exists;
  open.title = o.exists ? copyText("beChore.step.open", { path }) : (o.absent_said ?? "");
  if (o.exists) open.addEventListener("click", () => void openItem(path));
  return open;
}

function collectMonitorLocalStorageKeys(): { key: string; value: string }[] {
  return enumeratePrefix("cc-monitor.");
}

async function openItem(path: string): Promise<void> {
  try {
    await commands.os_open({ kind: "path", path });
  } catch (e) {
    console.warn(`[data-section] os_open ${path} failed:`, e);
    failToast(copyText("machineAliases.openRc.failed"), e);
  }
}

function truncateValue(v: string): string {
  if (v.length <= 60) return v;
  return copyText("data.truncate.ellipsis", { text: v.slice(0, 57) });
}
