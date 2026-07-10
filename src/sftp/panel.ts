/**
 * F48：SFTP 文件面板(独立 overlay,每 host 打开;照 SettingsPanel 范式,不碰 TabManager)。
 * 消费 F47 的 sftp_* 命令。Part 1 = 浏览(面包屑/列表/导航/排序);传输/写/拖入见后续。
 */
import { invoke, Channel } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { formatBytes } from "../format";
import { showActionFailureToast } from "../error-toast";
import type { RemoteHostConfig } from "../settings/remote-section";
import {
  basename,
  breadcrumbs,
  joinPath,
  newTransferId,
  parentPath,
  sortEntries,
  type SftpEntry,
  type SortBy,
} from "./paths";

/** F47 TransferProgress（camelCase）。 */
interface TransferProgress {
  transferred: number;
  total: number;
}

export class SftpPanel {
  private el: HTMLElement;
  private crumbBar!: HTMLElement;
  private listEl!: HTMLElement;
  private titleEl!: HTMLElement;
  private transfersEl!: HTMLElement;
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

    const upload = mkBtn("上传", () => void this.uploadHere());
    header.appendChild(upload);
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

    // F48 Part 2：进行中的传输(进度条 + 取消)。
    this.transfersEl = document.createElement("div");
    this.transfersEl.className = "sftp-transfers";
    panel.appendChild(this.transfersEl);

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

  // === 传输(Part 2)===

  /** 下载一个远端文件:save 对话框选本地落点 → sftp_download + 进度/取消。 */
  private async download(e: SftpEntry): Promise<void> {
    if (!this.cfg) return;
    let localPath: string | null;
    try {
      localPath = await saveDialog({ defaultPath: e.name });
    } catch (err) {
      showActionFailureToast("选择保存位置失败", String(err));
      return;
    }
    if (!localPath) return; // 用户取消对话框
    const remotePath = joinPath(this.cwd, e.name);
    await this.runTransfer(`下载 ${e.name}`, e.size, (transferId, onProgress) =>
      invoke("sftp_download", { cfg: this.cfg, remotePath, localPath, transferId, onProgress }),
    );
  }

  /** 上传本地文件到当前目录:open 选文件 → 覆盖前 stat 确认 → sftp_upload + 进度/取消。 */
  private async uploadHere(): Promise<void> {
    if (!this.cfg) return;
    let picked: string | string[] | null;
    try {
      picked = await openDialog({ multiple: false, directory: false });
    } catch (err) {
      showActionFailureToast("选择文件失败", String(err));
      return;
    }
    const localPath = Array.isArray(picked) ? picked[0] : picked;
    if (!localPath) return;
    const name = basename(localPath);
    const remotePath = joinPath(this.cwd, name);
    // 覆盖前确认(aterm 契约:不静默覆盖)。
    let exists = false;
    try {
      await invoke("sftp_stat", { cfg: this.cfg, path: remotePath });
      exists = true;
    } catch {
      exists = false; // stat 失败 = 不存在(或不可读),按新建处理
    }
    if (exists && !window.confirm(`远端已存在 ${name},覆盖?`)) return;
    await this.runTransfer(`上传 ${name}`, 0, (transferId, onProgress) =>
      invoke("sftp_upload", { cfg: this.cfg, localPath, remotePath, transferId, onProgress }),
    );
    await this.reload(); // 上传后刷新目录,新文件出现
  }

  /** 通用传输执行:建进度条 + 取消,跑 op,完成/失败清理。 */
  private async runTransfer(
    label: string,
    knownTotal: number,
    op: (transferId: string, onProgress: Channel<TransferProgress>) => Promise<unknown>,
  ): Promise<void> {
    const transferId = newTransferId();
    const rowEl = document.createElement("div");
    rowEl.className = "sftp-transfer";
    const labelEl = document.createElement("span");
    labelEl.className = "sftp-transfer-label";
    labelEl.textContent = label;
    const barWrap = document.createElement("div");
    barWrap.className = "sftp-bar";
    const bar = document.createElement("div");
    bar.className = "sftp-bar-fill";
    barWrap.appendChild(bar);
    const pct = document.createElement("span");
    pct.className = "sftp-transfer-pct";
    pct.textContent = knownTotal ? "0%" : "…";
    const cancelBtn = mkBtn("取消", () => {
      void invoke("sftp_cancel_transfer", { transferId });
    });
    cancelBtn.className = "sftp-btn sftp-cancel-transfer";
    rowEl.append(labelEl, barWrap, pct, cancelBtn);
    this.transfersEl.appendChild(rowEl);

    const onProgress = new Channel<TransferProgress>();
    onProgress.onmessage = (p) => {
      const total = p.total || knownTotal;
      if (total > 0) {
        const ratio = Math.min(1, p.transferred / total);
        bar.style.width = `${(ratio * 100).toFixed(1)}%`;
        pct.textContent = `${(ratio * 100).toFixed(0)}% (${formatBytes(p.transferred)}/${formatBytes(total)})`;
      } else {
        pct.textContent = formatBytes(p.transferred);
      }
    };
    try {
      await op(transferId, onProgress);
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("已取消")) showActionFailureToast(`${label} 失败`, msg);
    } finally {
      rowEl.remove();
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
      } else {
        // 文件:行内「下载」按钮(Part 3 再加右键改名/删除)。
        const dl = mkBtn("下载", () => void this.download(e));
        dl.className = "sftp-btn sftp-row-btn";
        row.appendChild(dl);
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
