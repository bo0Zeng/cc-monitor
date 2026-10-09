/**
 * 设置「数据位置」：monitor 所有持久化数据的位置 ＋ WebView2 用户数据目录 ＋ localStorage 键，
 * 每项［打开］与「在文件夹中显示」。纯展示，不做删除 / 清空（用户要的是路径）；
 * `data-section.vitest.ts` 钉着：效应面两向相等（只读 `get_data_paths`、只会打开 / 显示、localStorage 零写）＋ 路径以纯文本上屏。
 * 不存在的灰显标「尚未创建」；卸载不清 `~/.cc-monitor/`。
 */

import { commands } from "../ipc/commands";
import { openPath } from "@tauri-apps/plugin-opener";
import { toast } from "../kit/toast";
import { enumeratePrefix } from "../local-storage";
import { formatBytes } from "../format";
import { holdSkeletonHeight, makeSkeleton } from "./skeleton";
import { revealInFolder } from "../reveal-in-folder";

// 类型由 `data_paths.rs` 的 `#[derive(ts_rs::TS)]` 生成（`src/frontend/ui/generated/`）。
import type { DataClass } from "../generated/DataClass";
import type { DataPathInfo } from "../generated/DataPathInfo";
import type { DataPathsResponse } from "../generated/DataPathsResponse";
import { copyText } from "../copy-table";
import { detailOf, sayWithDetail } from "../kit/detail";
import { icon } from "../kit/icon";

/** 每一行那一格「删了会怎样」（后端 `DataPathInfo.class`）：看得出哪个删了会丢东西。后端加了第三类不许整页炸、原样说出来。 */
/**
 * 日志目录那一行的名字：这一行不自带［打开］，指向「日志」那一页（那一页自己有「打开日志目录」）。
 * 跨语言常量：Rust 侧 `data_paths.rs::LOGS_DIR_LABEL` 同名同值，`data-section.vitest.ts` 读源码对拍。
 */
export const LOGS_DIR_LABEL = "logs/";

export function describeDataClass(c: DataClass): string {
  switch (c) {
    case "truth":
      return copyText("data.class.keep");
    case "cache":
      return copyText("data.class.disposable");
    default:
      return copyText("data.class.unknown", { kind: String(c) });
  }
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
  private loaded = false;

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
    // 容器先钉住高度，与骨架同一个数（住址只有 `skeleton.ts`）：等数据回来再钉，那一跳已经发生过了。
    holdSkeletonHeight(this.mainBody, "data-places");
    this.root.appendChild(this.mainBody);

    this.renderPlaceholder();
    // 构造期不发 I/O：宿主（`panel.ts`）在该页首次可见时调 `loadNow()`。
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 宿主在「这一页首次可见」时调。幂等由宿主保证（`panel.ts::pagesLoaded`）：这里再记一份，重开设置想重读时会被旧标记挡住。 */
  loadNow(): void {
    this.loaded = false;
    this.renderPlaceholder();
    void this.load();
  }

  /** 重新拉取（设置每次 open 时调，大小 / 存在性要最新）。 */
  refresh(): void {
    this.loadNow();
  }

  private renderPlaceholder(): void {
    this.mainBody.replaceChildren();
    // 加载态是与渲染态同高的骨架（不重排）。类名沿用 `settings-data-loading`（`styles.css` 那条规则的读者）；多出来的高度、身份走 `data-*`。
    this.mainBody.appendChild(
      makeSkeleton("data-places", copyText("data.renderPlaceholder.loading"), "settings-data-loading"),
    );
  }

  private async load(): Promise<void> {
    try {
      const data = await commands.get_data_paths();
      this.loaded = true;
      this.render(data);
    } catch (e) {
      this.mainBody.replaceChildren();
      const err = document.createElement("div");
      err.className = "settings-data-error";
      sayWithDetail(err, copyText("data.load.failed", { e: String(e) }), detailOf(e));
      this.mainBody.appendChild(err);
    }
  }

  private render(data: DataPathsResponse): void {
    if (!this.loaded) return;
    this.mainBody.replaceChildren();

    // 卡片 1：monitor 持久化数据
    this.mainBody.appendChild(
      this.buildBlock({
        title: copyText("data.card.own"),
        subtitle: data.monitorDataDir,
        subtitlePath: data.monitorDataDir,
        items: data.entries,
      }),
    );

    // 卡片 1b：本机后端住在同一个家里的那几样（只给路径）
    this.mainBody.appendChild(
      this.buildBlock({
        title: copyText("data.card.backend"),
        subtitle: data.backendHome,
        subtitlePath: data.backendHome,
        items: data.backendEntries,
      }),
    );

    // 卡片 2：WebView2 用户数据
    if (data.webviewUserDataDir) {
      this.mainBody.appendChild(
        this.buildBlock({
          title: copyText("data.card.webview"),
          subtitle: copyText("data.card.webviewHint"),
          items: [data.webviewUserDataDir],
        }),
      );
    }


    // 卡片 4：浏览器 localStorage
    this.mainBody.appendChild(this.buildLocalStorageBlock());

    // 卡片 5：卸载说明（纯文字；`<strong>` 由 DOM 建，不拼 innerHTML）
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
      // 那一格怎么读：只想腾空间的话，删「可随手删」的就够了。
      copyText("data.note.classes"),
    );
    this.mainBody.appendChild(note);
  }

  private buildBlock(opts: {
    title: string;
    subtitle?: string;
    subtitlePath?: string;
    items: DataPathInfo[];
  }): HTMLElement {
    const block = document.createElement("div");
    block.className = "settings-data-block";

    const head = document.createElement("div");
    head.className = "settings-data-block-head";
    const titleEl = document.createElement("span");
    titleEl.className = "settings-data-block-title";
    titleEl.textContent = opts.title;
    head.appendChild(titleEl);

    if (opts.subtitle) {
      const sub = document.createElement("span");
      sub.className = "settings-data-block-subtitle";
      sub.textContent = opts.subtitle;
      if (opts.subtitlePath) {
        sub.title = opts.subtitlePath;
        sub.classList.add("clickable");
        sub.addEventListener("click", () => void openItem(opts.subtitlePath!));
      }
      head.appendChild(sub);
    }
    block.appendChild(head);

    const list = document.createElement("ul");
    list.className = "settings-data-list";
    for (const it of opts.items) {
      list.appendChild(this.buildItemRow(it));
    }
    block.appendChild(list);
    return block;
  }

  private buildItemRow(info: DataPathInfo): HTMLElement {
    const li = document.createElement("li");
    // 条目种类是有限枚举 ⇒ 走 `data-kind`（全仓 `kind` 这个状态名只用这一种载体），不再拼 `kind-<值>` 类名。
    li.className = `settings-data-item ${info.exists ? "exists" : "absent"}`;
    li.dataset.kind = info.kind;

    const label = document.createElement("span");
    label.className = "settings-data-item-label";
    label.textContent = info.label;
    li.appendChild(label);

    const desc = document.createElement("span");
    desc.className = "settings-data-item-desc";
    desc.textContent = info.description;
    li.appendChild(desc);

    // 「删了会怎样」那一格：类别进 DOM（`data-class`），判据与用户看的是同一份值；不挂类名（只要字，不要样式）。
    li.dataset.class = info.class;
    const cls = document.createElement("span");
    cls.dataset.dataClass = info.class;
    cls.textContent = describeDataClass(info.class);
    li.appendChild(cls);

    const meta = document.createElement("span");
    meta.className = "settings-data-item-meta";
    if (info.exists) {
      // `sizeBytes` 是 number：Rust 侧 `#[ts(type = "number")]` 的显式决定（JSON IPC 到 TS 侧本来就是 number），见 `data_paths.rs`。
      meta.textContent =
        info.sizeBytes !== undefined ? formatBytes(info.sizeBytes) : copyText("data.item.created");
    } else {
      meta.textContent = copyText("data.item.notCreated");
    }
    li.appendChild(meta);

    if (info.label.startsWith(LOGS_DIR_LABEL)) {
      // 日志目录（连同后端那一份 `logs/backend/`）：这一行只给路径，打开去「日志」那一页（见 `LOGS_DIR_LABEL` 头注）。
      const see = document.createElement("span");
      see.dataset.seeAlso = "logs";
      see.textContent = copyText("data.item.inLogsPage");
      li.appendChild(see);
    } else {
      // [打开] 与「在文件夹中显示」并排住同一格（这一格是网格里的一个区，两颗按钮装进一个盒子）。
      const actions = document.createElement("span");
      actions.className = "settings-data-item-actions";
      actions.appendChild(this.buildOpenButton(info));
      // 不在盘上的东西没有可选中的那一项 ⇒ 不给这颗按钮。
      if (info.exists) actions.appendChild(buildRevealButton(info.path));
      li.appendChild(actions);
    }

    // 完整路径单独一行（小字 + ellipsis）
    const pathRow = document.createElement("div");
    pathRow.className = "settings-data-item-path";
    pathRow.textContent = info.path;
    pathRow.title = info.path;
    li.appendChild(pathRow);

    return li;
  }

  private buildOpenButton(info: DataPathInfo): HTMLElement {
    const open = document.createElement("button");
    open.type = "button";
    open.className = "settings-data-item-open";
    open.textContent = info.exists ? copyText("data.item.open") : copyText("data.item.openNone");
    open.disabled = !info.exists;
    open.title = info.exists ? copyText("data.item.openHint", { path: info.path }) : copyText("data.item.missing");
    if (info.exists) {
      open.addEventListener("click", () => void openItem(info.path));
    }
    return open;
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

/** 「在文件夹中显示」：系统文件管理器打开它所在的文件夹并选中它（[打开] 是用默认程序打开它本身）。 */
function buildRevealButton(path: string): HTMLElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "settings-data-item-open";
  b.dataset.reveal = "";
  b.textContent = copyText("data.item.reveal");
  b.addEventListener("click", () => void revealInFolder(path));
  return b;
}

function collectMonitorLocalStorageKeys(): { key: string; value: string }[] {
  return enumeratePrefix("cc-monitor.");
}

async function openItem(path: string): Promise<void> {
  try {
    await openPath(path);
  } catch (e) {
    console.warn(`[data-section] openPath ${path} failed:`, e);
    toast(copyText("data.open.failed"), String(e), { detail: detailOf(e) });
  }
}

function truncateValue(v: string): string {
  if (v.length <= 60) return v;
  return copyText("data.truncate.ellipsis", { text: v.slice(0, 57) });
}
