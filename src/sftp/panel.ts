/**
 * F48：SFTP 文件面板(独立 overlay,每 host 打开;照 SettingsPanel 范式,不碰 TabManager)。
 * 消费 F47 的 sftp_* 命令。Part 1 = 浏览(面包屑/列表/导航/排序);传输/写/拖入见后续。
 */
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "../format";
import { showActionFailureToast } from "../error-toast";
import type { RemoteHostConfig } from "../settings/remote-section";
import {
  breadcrumbs,
  joinPath,
  parentPath,
  sortEntries,
  type SftpEntry,
  type SortBy,
} from "./paths";

export class SftpPanel {
  private el: HTMLElement;
  private crumbBar!: HTMLElement;
  private listEl!: HTMLElement;
  private titleEl!: HTMLElement;
  private cfg: RemoteHostConfig | null = null;
  private cwd = "/";
  private sortBy: SortBy = "name";
  private entries: SftpEntry[] = [];

  constructor() {
    this.el = document.createElement("div");
    this.el.className = "sftp-overlay";
    this.el.style.display = "none";
    this.el.appendChild(this.buildChrome());
    document.body.appendChild(this.el);
    // 点遮罩空白关闭(点面板体不关)
    this.el.addEventListener("click", (e) => {
      if (e.target === this.el) this.close();
    });
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && this.el.style.display !== "none") this.close();
    });
  }

  private buildChrome(): HTMLElement {
    const panel = document.createElement("div");
    panel.className = "sftp-panel";

    const header = document.createElement("div");
    header.className = "sftp-header";
    this.titleEl = document.createElement("span");
    this.titleEl.className = "sftp-title";
    header.appendChild(this.titleEl);

    const sortSel = document.createElement("select");
    sortSel.className = "sftp-sort";
    for (const [v, label] of [
      ["name", "名称"],
      ["size", "大小"],
      ["type", "类型"],
    ] as [SortBy, string][]) {
      const o = document.createElement("option");
      o.value = v;
      o.textContent = `排序:${label}`;
      sortSel.appendChild(o);
    }
    sortSel.addEventListener("change", () => {
      this.sortBy = sortSel.value as SortBy;
      this.renderList();
    });
    header.appendChild(sortSel);

    const refresh = mkBtn("刷新", () => void this.reload());
    header.appendChild(refresh);
    const closeBtn = mkBtn("关闭", () => this.close());
    closeBtn.className = "sftp-btn sftp-close";
    header.appendChild(closeBtn);
    panel.appendChild(header);

    this.crumbBar = document.createElement("div");
    this.crumbBar.className = "sftp-crumbs";
    panel.appendChild(this.crumbBar);

    this.listEl = document.createElement("div");
    this.listEl.className = "sftp-list";
    panel.appendChild(this.listEl);

    return panel;
  }

  /** 打开面板并浏览该 host(以 realpath('.') 为起点)。 */
  async open(cfg: RemoteHostConfig): Promise<void> {
    this.cfg = cfg;
    this.titleEl.textContent = `文件:${cfg.label || cfg.host}`;
    this.el.style.display = "flex";
    this.listEl.textContent = "连接中…";
    try {
      const home = await invoke<string>("sftp_realpath", { cfg, path: "." });
      this.cwd = home || "/";
    } catch (e) {
      this.cwd = "/";
      showActionFailureToast("SFTP 连接失败", String(e));
    }
    await this.reload();
  }

  close(): void {
    this.el.style.display = "none";
    this.entries = [];
    this.listEl.textContent = "";
  }

  private async navigate(path: string): Promise<void> {
    this.cwd = path;
    await this.reload();
  }

  private async reload(): Promise<void> {
    if (!this.cfg) return;
    this.renderCrumbs();
    this.listEl.textContent = "读取中…";
    try {
      this.entries = await invoke<SftpEntry[]>("sftp_list_dir", {
        cfg: this.cfg,
        path: this.cwd,
      });
      this.renderList();
    } catch (e) {
      this.listEl.textContent = "";
      showActionFailureToast("读目录失败", String(e));
    }
  }

  private renderCrumbs(): void {
    this.crumbBar.textContent = "";
    // 上级
    const up = mkBtn("↑", () => void this.navigate(parentPath(this.cwd)));
    up.className = "sftp-crumb-up";
    up.title = "上级目录";
    this.crumbBar.appendChild(up);
    for (const seg of breadcrumbs(this.cwd)) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "sftp-crumb";
      b.textContent = seg.name;
      b.addEventListener("click", () => void this.navigate(seg.path));
      this.crumbBar.appendChild(b);
    }
  }

  private renderList(): void {
    this.listEl.textContent = "";
    const sorted = sortEntries(this.entries, this.sortBy);
    if (sorted.length === 0) {
      const empty = document.createElement("div");
      empty.className = "sftp-empty";
      empty.textContent = "(空目录)";
      this.listEl.appendChild(empty);
      return;
    }
    for (const e of sorted) {
      const row = document.createElement("div");
      row.className = "sftp-row";
      row.classList.toggle("sftp-lossy", e.lossyName);
      const icon = document.createElement("span");
      icon.className = "sftp-icon";
      icon.textContent = e.isSymlink ? "↳" : e.isDir ? "📁" : "📄";
      row.appendChild(icon);
      const name = document.createElement("span");
      name.className = "sftp-name";
      name.textContent = e.name;
      row.appendChild(name);
      if (!e.isDir) {
        const size = document.createElement("span");
        size.className = "sftp-size";
        size.textContent = formatBytes(e.size);
        row.appendChild(size);
      }
      if (e.isDir) {
        row.classList.add("sftp-dir");
        row.addEventListener("dblclick", () => void this.navigate(joinPath(this.cwd, e.name)));
      }
      if (e.lossyName) row.title = "文件名含非 UTF-8 字节,只读(无法安全写操作)";
      this.listEl.appendChild(row);
    }
  }
}

function mkBtn(label: string, onClick: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "sftp-btn";
  b.textContent = label;
  b.addEventListener("click", onClick);
  return b;
}

/** 单例(整个 app 共用一个 overlay 实例)。 */
let singleton: SftpPanel | null = null;
export function openSftpPanel(cfg: RemoteHostConfig): void {
  singleton ??= new SftpPanel();
  void singleton.open(cfg);
}
