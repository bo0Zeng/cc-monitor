/**
 * Issue #3 (A 透明化): 设置面板「数据」区。
 *
 * 列出 monitor 所有持久化数据的位置 + WebView2 用户数据目录 + localStorage keys，
 * 每项配 [打开] 按钮。**纯展示，不做删除 / 清空操作**——避免误点。
 *
 * 设计：
 * - 进入面板时 invoke `get_data_paths` 拉一次后端探测（async + spawn_blocking）
 * - 前端再 enumerate localStorage 加到本地 section
 * - 文件不存在时灰显 + 标 "(尚未创建)"
 * - 卸载行为说明：NSIS 默认不清 `~/.claude/work/`，用户元数据保留
 */

// C04a：**本模块是包装层的样板**——不再直接 import `invoke`，改用 `commands`。
// 选它做样板的理由：全仓 29 个 import invoke 的文件里，它的调用点最少（1 处），
// 且它的返回类型 C01 就已经生成好了。
import { commands } from "../ipc/commands";
import { openPath } from "@tauri-apps/plugin-opener";
import { showActionFailureToast } from "../error-toast";
import { enumeratePrefix } from "../local-storage";
import { formatBytes } from "../format";
import { holdSkeletonHeight, makeSkeleton } from "./skeleton";

// C01（rust-ts-boundary）：这两个类型**改成从生成物 import**，不再手写。
// 生成源是 `src/bridge/src/data_paths.rs` 的 `#[derive(ts_rs::TS)]`，产出 `src/generated/`。
//
// **换过来当场发现两处手写与 Rust 不一致**，都记在 C01 计划的 §7：
// ① `sizeBytes` 手写成 `number`，而 Rust 是 `u64` ⇒ **静默有损**（JS number 是 f64，
//    安全整数上限 2^53-1）。生成物给的是 `bigint`，诚实。收窄见下方 `Number(...)` 那行。
// ② `kind` 手写成 `"file" | "dir"`，而 Rust 是任意 `String` ⇒ **生成物会放宽这个类型**。
//    正确方向是把 Rust 侧改成 enum（让源更严），**但那是类型收紧、不属于 C01 范围**，
//    已登记。在那之前 `kind` 在 TS 侧是 `string`。
// `ts-rs` 一个类型一个文件，所以是两条 import（本文件其余 import 不带扩展名，这里对齐）。
import type { DataPathInfo } from "../generated/DataPathInfo";
import type { DataPathsResponse } from "../generated/DataPathsResponse";

/** 一个 `<strong>` —— 让「强调」由 DOM 结构承担，而不是由字符串里的标记承担（`70 §2.3`）。 */
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

  /**
   * `headless: true` 时不渲染外层 settings-group 容器（用于 CollapsibleGroup 内嵌）。
   * 跟 DiagnosticsSection 一致的契约。
   */
  constructor(opts: { headless?: boolean } = {}) {
    this.headless = !!opts.headless;
    this.root = document.createElement("div");
    this.root.className = this.headless ? "settings-data-headless" : "settings-group";

    if (!this.headless) {
      const heading = document.createElement("div");
      heading.className = "settings-group-title";
      // 🔴 `70 §10.2`「名字」：**「数据存储」→「数据位置」**。
      // 理由**不是 R5 命中**（「数据存储」本来就是名词短语，`91 §4` 的 R5 抓不到它），
      // 是**同组重名**：同一个折叠组里已经有一块叫「Claude 数据目录」（`panel.ts`），
      // 两个「数据」并排；而这一块的全部内容是**路径 + 大小 + 打开** —— 说的是位置，
      // 不是存储策略。
      heading.textContent = "数据位置";
      this.root.appendChild(heading);
    }

    this.mainBody = document.createElement("div");
    this.mainBody.className = "settings-data-body";
    // 步 1（`70 §1.3 A`）：**容器先钉住高度**，与骨架同一个数（住址只有 `skeleton.ts` 一处）。
    // 要在这里钉、不能等数据回来再钉 —— 等回来那一跳已经发生过了。
    holdSkeletonHeight(this.mainBody, "data-places");
    this.root.appendChild(this.mainBody);

    this.renderPlaceholder();
    // 🔴 步 2（`70 §1.3 B` · `§10.4`）：**构造期不再发 I/O。**
    // 原来这里是 `void this.load()`，而这一块住「应用」页、落地页是「机器」
    // ⇒ 每次打开设置都白发一趟 `get_data_paths`，还正好在那 3 秒重排窗口里。
    // 现在由宿主（`panel.ts`）在**该页首次可见**时调 `loadNow()`。
    // ⚠ 折叠**不省**这一发：折叠是纯 CSS（`collapsible-group.ts` 的 `0fr ↔ 1fr`），
    //   孩子照常构造 —— 所以门只能开在这里，不能指望折叠。
  }

  get element(): HTMLElement {
    return this.root;
  }

  /**
   * 步 2：宿主在「这一页首次可见」时调它。
   *
   * ⚠ **幂等由宿主保证，不在这里**（`panel.ts::pagesLoaded`）。两处都判「放过没有」
   * 的话，`open()` 想让读数重新来一遍时会被这一侧的旧标记挡住 —— 而界面上看不出来，
   * 它只是继续显示上一次那份。**一个事实一个住址。**
   */
  loadNow(): void {
    this.loaded = false;
    this.renderPlaceholder();
    void this.load();
  }

  /** 重新拉取（设置面板每次 open 时调，确保大小 / 存在性是最新的） */
  refresh(): void {
    this.loadNow();
  }

  private renderPlaceholder(): void {
    this.mainBody.replaceChildren();
    // 步 1：加载态从**会长高的一行字**换成**与渲染态同高**的骨架
    //（`70 §10.2` 差项 3：「加载中…」→ 4~5 个卡片 ⇒ 重排）。
    // ⚠ 类名沿用 `settings-data-loading`（`src/styles.css` 里那条规则的**唯一读者**）——
    //   换成新类名会当场让那条规则变成死规则，撞 `css-ledger` 的「每个 CSS 类名都说得出
    //   谁在用它」。骨架多出来的那一维（高度、身份）走 `data-*`，不新造类。
    this.mainBody.appendChild(
      makeSkeleton("data-places", "正在读这台机器上的数据位置…", "settings-data-loading"),
    );
  }

  private async load(): Promise<void> {
    try {
      // 命令名与返回类型都由包装层给出——这里既不写字面量也不写类型参数。
      const data = await commands.get_data_paths();
      this.loaded = true;
      this.render(data);
    } catch (e) {
      this.mainBody.replaceChildren();
      const err = document.createElement("div");
      err.className = "settings-data-error";
      err.textContent = `加载失败：${String(e)}`;
      this.mainBody.appendChild(err);
    }
  }

  private render(data: DataPathsResponse): void {
    if (!this.loaded) return;
    this.mainBody.replaceChildren();

    // 卡片 1：monitor 持久化数据
    this.mainBody.appendChild(
      this.buildBlock({
        title: "monitor 持久化",
        subtitle: data.monitorDataDir,
        subtitlePath: data.monitorDataDir,
        items: data.entries,
      }),
    );

    // 卡片 2：WebView2 用户数据
    if (data.webviewUserDataDir) {
      this.mainBody.appendChild(
        this.buildBlock({
          title: "WebView2 用户数据",
          subtitle: "由 WebView2 Runtime 管理，含 cache / localStorage / IndexedDB / cookies",
          items: [data.webviewUserDataDir],
        }),
      );
    }

    // 卡片 3：PowerShell profile 备份（只在装过 cc 集成时出现）
    if (data.profileBackupDirs.length > 0) {
      this.mainBody.appendChild(
        this.buildBlock({
          title: "PowerShell profile 备份",
          subtitle: "v1.7.10+ 装 cc 集成时自动备份 profile 到同目录的 .ccm-backup-<时间戳>",
          items: data.profileBackupDirs,
        }),
      );
    }

    // 卡片 4：浏览器 localStorage
    this.mainBody.appendChild(this.buildLocalStorageBlock());

    // 卡片 5：卸载说明（纯文字提示）
    //
    // 🔴 `70 §10.2` 差项 5：原来这里是 `note.innerHTML = "🛈 <strong>…</strong>…"`。
    // 安全上无碍（常量串），但它是 `§2.3` 那条「**强调由 DOM 结构承担**」的反例 ——
    // 而且 `91 §5.1` 抽文案表那一步**抽不动**嵌着标记的句子。
    // ⇒ 换成真 DOM：`<strong>` 由 `document.createElement` 建，文字是纯文本。
    const note = document.createElement("div");
    note.className = "settings-data-note";
    note.append(
      "🛈 ",
      strong("卸载 monitor"),
      " 默认",
      strong("不清"),
      "这些数据。想彻底清除（星标 / 颜色配置 / WebView2 cache 等）请手动删除上面的目录。",
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
    li.className = `settings-data-item kind-${info.kind} ${info.exists ? "exists" : "absent"}`;

    const label = document.createElement("span");
    label.className = "settings-data-item-label";
    label.textContent = info.label;
    li.appendChild(label);

    const desc = document.createElement("span");
    desc.className = "settings-data-item-desc";
    desc.textContent = info.description;
    li.appendChild(desc);

    const meta = document.createElement("span");
    meta.className = "settings-data-item-meta";
    if (info.exists) {
      // C01：`sizeBytes` 的类型现在由生成物给出（`sizeBytes?: number`）。
      // 那个 `number` 是 Rust 侧 `#[ts(type = "number")]` 的**显式决定**（附上限论证），
      // 不是这里的巧合——`ts-rs` 默认会把 `u64` 映射成 `bigint`，而 Tauri 的 JSON IPC
      // 到 TS 侧是 number，那个默认值对运行时是错的。详见 `data_paths.rs` 该字段的注释。
      meta.textContent =
        info.sizeBytes !== undefined ? formatBytes(info.sizeBytes) : "已创建";
    } else {
      meta.textContent = "(尚未创建)";
    }
    li.appendChild(meta);

    const open = document.createElement("button");
    open.type = "button";
    open.className = "settings-data-item-open";
    open.textContent = info.exists ? "打开" : "—";
    open.disabled = !info.exists;
    open.title = info.exists ? `打开 ${info.path}` : "文件 / 目录不存在";
    if (info.exists) {
      open.addEventListener("click", () => void openItem(info.path));
    }
    li.appendChild(open);

    // 完整路径单独一行（小字 + ellipsis）
    const pathRow = document.createElement("div");
    pathRow.className = "settings-data-item-path";
    pathRow.textContent = info.path;
    pathRow.title = info.path;
    li.appendChild(pathRow);

    return li;
  }

  private buildLocalStorageBlock(): HTMLElement {
    const keys = collectMonitorLocalStorageKeys();

    const block = document.createElement("div");
    block.className = "settings-data-block";

    const head = document.createElement("div");
    head.className = "settings-data-block-head";
    const title = document.createElement("span");
    title.className = "settings-data-block-title";
    title.textContent = "浏览器 localStorage";
    head.appendChild(title);
    const sub = document.createElement("span");
    sub.className = "settings-data-block-subtitle";
    sub.textContent =
      keys.length > 0
        ? "前端持久化的 UI 偏好（折叠 / 渲染模式 / profile 选项等）"
        : "尚无任何 cc-monitor.* key";
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

function collectMonitorLocalStorageKeys(): { key: string; value: string }[] {
  return enumeratePrefix("cc-monitor.");
}

async function openItem(path: string): Promise<void> {
  try {
    await openPath(path);
  } catch (e) {
    console.warn(`[data-section] openPath ${path} failed:`, e);
    showActionFailureToast("打开失败", String(e));
  }
}

function truncateValue(v: string): string {
  if (v.length <= 60) return v;
  return v.slice(0, 57) + "…";
}
