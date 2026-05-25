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
 * - 卸载行为说明：NSIS 默认不清 `~/.claude/claudecode-frontend/`，用户元数据保留
 */

import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";

interface DataPathInfo {
  label: string;
  path: string;
  kind: "file" | "dir";
  description: string;
  exists: boolean;
  sizeBytes?: number;
}

interface DataPathsResponse {
  monitorDataDir: string;
  entries: DataPathInfo[];
  webviewUserDataDir: DataPathInfo | null;
  profileBackupDirs: DataPathInfo[];
}

export const DATA_SECTION_INFO_TEXT =
  "列出 monitor 所有持久化到磁盘的数据位置 + WebView2 用户数据 + localStorage。" +
  "纯展示，不会动数据。点 [打开] 用资源管理器打开对应位置。卸载 monitor 默认不清这些数据。";

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
      heading.textContent = "数据存储";
      this.root.appendChild(heading);
    }

    this.mainBody = document.createElement("div");
    this.mainBody.className = "settings-data-body";
    this.root.appendChild(this.mainBody);

    this.renderPlaceholder();
    void this.load();
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 重新拉取（设置面板每次 open 时调，确保大小 / 存在性是最新的） */
  refresh(): void {
    this.loaded = false;
    this.renderPlaceholder();
    void this.load();
  }

  private renderPlaceholder(): void {
    this.mainBody.replaceChildren();
    const hint = document.createElement("div");
    hint.className = "settings-data-loading";
    hint.textContent = "加载中…";
    this.mainBody.appendChild(hint);
  }

  private async load(): Promise<void> {
    try {
      const data = await invoke<DataPathsResponse>("get_data_paths");
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
    const note = document.createElement("div");
    note.className = "settings-data-note";
    note.innerHTML =
      "🛈 <strong>卸载 monitor</strong> 默认<strong>不清</strong>这些数据。" +
      "想彻底清除（星标 / 颜色配置 / WebView2 cache 等）请手动删除上面的目录。";
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
  const out: { key: string; value: string }[] = [];
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (!k || !k.startsWith("cc-monitor.")) continue;
      out.push({ key: k, value: localStorage.getItem(k) ?? "" });
    }
  } catch (e) {
    console.warn("[data-section] localStorage enumerate failed:", e);
  }
  out.sort((a, b) => a.key.localeCompare(b.key));
  return out;
}

async function openItem(path: string): Promise<void> {
  try {
    await openPath(path);
  } catch (e) {
    console.warn(`[data-section] openPath ${path} failed:`, e);
    alert(`打开失败：${String(e)}`);
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function truncateValue(v: string): string {
  if (v.length <= 60) return v;
  return v.slice(0, 57) + "…";
}
