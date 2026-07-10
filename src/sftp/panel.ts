/**
 * F48：SFTP 文件面板(独立 overlay,每 host 打开;照 SettingsPanel 范式,不碰 TabManager)。
 * 消费 F47 的 sftp_* 命令。Part 1 = 浏览(面包屑/列表/导航/排序);传输/写/拖入见后续。
 */
import { invoke, Channel } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { formatBytes } from "../format";
import { showActionFailureToast } from "../error-toast";
import { LS_KEYS, safeGetJson, safeSetJson } from "../local-storage";
import { buildOpenTerminalCmd } from "../remote-launch";
import type { RemoteHostConfig } from "../settings/remote-section";
import {
  addBookmark,
  basename,
  breadcrumbs,
  joinPath,
  newTransferId,
  parentPath,
  removeBookmark,
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
  private bookmarkBar!: HTMLElement;
  private cfg: RemoteHostConfig | null = null;
  private cwd = "/";
  private sortBy: SortBy = "name";
  private entries: SftpEntry[] = [];
  private dropUnlisten: UnlistenFn | null = null;
  private registeringDrop = false;

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
    const newDir = mkBtn("新建目录", () => void this.newDir());
    header.appendChild(newDir);
    const term = mkBtn("在此打开终端", () => this.openTerminalHere());
    header.appendChild(term);
    const pin = mkBtn("★ Pin", () => this.toggleBookmark());
    header.appendChild(pin);
    const refresh = mkBtn("刷新", () => void this.reload());
    header.appendChild(refresh);
    const closeBtn = mkBtn("关闭", () => this.close());
    closeBtn.className = "sftp-btn sftp-close";
    header.appendChild(closeBtn);
    panel.appendChild(header);

    this.bookmarkBar = document.createElement("div");
    this.bookmarkBar.className = "sftp-bookmarks";
    panel.appendChild(this.bookmarkBar);

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
    // F48 Part 3：拖入上传——面板打开时监听 webview 拖放,drop 时上传到当前目录。
    // D 审计建议-1:先占位 registeringDrop 防快速双开在 await 期间重复注册(泄漏 unlisten)。
    if (!this.dropUnlisten && !this.registeringDrop) {
      this.registeringDrop = true;
      try {
        const panelBody = this.el.querySelector(".sftp-panel");
        this.dropUnlisten = await getCurrentWebviewWindow().onDragDropEvent((ev) => {
          if (this.el.style.display === "none") return; // 面板未开不理会
          const t = ev.payload.type;
          if (t === "enter" || t === "over") {
            panelBody?.classList.add("sftp-dragover");
          } else {
            panelBody?.classList.remove("sftp-dragover");
          }
          if (t === "drop") void this.uploadDropped(ev.payload.paths);
        });
      } catch {
        /* 拖放不可用(如某些平台)→ 忽略,菜单上传仍可用 */
      } finally {
        this.registeringDrop = false;
      }
    }
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
    this.transfersEl.textContent = ""; // D 审计建议-3:切 host 不残留上一台的进度行
    // 兜底:清任何残留的编辑对话框,避免重开/切 host 时旧编辑框(带旧文件内容)浮在最上层。
    this.el.querySelectorAll(".sftp-edit-back").forEach((n) => n.remove());
    this.sortBy = "name";
    if (this.dropUnlisten) {
      this.dropUnlisten();
      this.dropUnlisten = null;
    }
  }

  private async navigate(path: string): Promise<void> {
    this.cwd = path;
    await this.reload();
  }

  private async reload(): Promise<void> {
    if (!this.cfg) return;
    this.renderBookmarks();
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
    // D 审计建议-2:用本地 cancelled flag 判定「是用户取消的」,不靠脆弱的后端错误串匹配。
    let cancelled = false;
    const cancelBtn = mkBtn("取消", () => {
      cancelled = true;
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
      if (!cancelled) showActionFailureToast(`${label} 失败`, String(e));
    } finally {
      rowEl.remove();
    }
  }

  // === 写(Part 3):新建目录 / 重命名 / 删除 / 拖入上传 ===

  private async newDir(): Promise<void> {
    if (!this.cfg) return;
    const name = window.prompt("新建目录名:");
    if (!name?.trim()) return;
    await this.doWrite("sftp_mkdir", { cfg: this.cfg, path: joinPath(this.cwd, name.trim()) });
  }

  private async rename(e: SftpEntry): Promise<void> {
    if (!this.cfg) return;
    const to = window.prompt(`重命名 ${e.name} 为:`, e.name);
    if (!to?.trim() || to.trim() === e.name) return;
    await this.doWrite("sftp_rename", {
      cfg: this.cfg,
      from: joinPath(this.cwd, e.name),
      to: joinPath(this.cwd, to.trim()),
    });
  }

  private async remove(e: SftpEntry): Promise<void> {
    if (!this.cfg) return;
    // 二次确认,文案回显真实条目名(aterm 契约:防误删)。
    const kind = e.isDir ? "目录" : "文件";
    if (!window.confirm(`删除${kind} ${e.name}?此操作不可撤销。`)) return;
    await this.doWrite("sftp_delete", {
      cfg: this.cfg,
      path: joinPath(this.cwd, e.name),
      isDir: e.isDir,
    });
  }

  /** 拖入的本地文件 → 逐个上传到当前目录(复用 runTransfer)。 */
  private async uploadDropped(paths: string[]): Promise<void> {
    if (!this.cfg) return;
    for (const localPath of paths) {
      const name = basename(localPath);
      const remotePath = joinPath(this.cwd, name);
      let exists = false;
      try {
        await invoke("sftp_stat", { cfg: this.cfg, path: remotePath });
        exists = true;
      } catch {
        exists = false;
      }
      if (exists && !window.confirm(`远端已存在 ${name},覆盖?`)) continue;
      await this.runTransfer(`上传 ${name}`, 0, (transferId, onProgress) =>
        invoke("sftp_upload", { cfg: this.cfg, localPath, remotePath, transferId, onProgress }),
      );
    }
    await this.reload();
  }

  /** F49：编辑小文本文件——读(护栏)→ 弹编辑对话框 → 保存(原子写,失败保留内容)。 */
  private async editFile(e: SftpEntry): Promise<void> {
    if (!this.cfg) return;
    const path = joinPath(this.cwd, e.name);
    let text: string | null;
    try {
      text = await invoke<string | null>("sftp_read_text_for_edit", { cfg: this.cfg, path });
    } catch (err) {
      showActionFailureToast("读取失败", String(err));
      return;
    }
    if (text === null) {
      showActionFailureToast("不可编辑", `${e.name} 过大(>256KB)或含二进制/非 UTF-8 内容。`);
      return;
    }
    this.openEditDialog(e.name, path, text);
  }

  /** 编辑对话框(面板内浮层):textarea + 字符/字节数 + 保存(确认)/取消;失败保留内容。 */
  private openEditDialog(name: string, path: string, initial: string): void {
    const back = document.createElement("div");
    back.className = "sftp-edit-back";
    const box = document.createElement("div");
    box.className = "sftp-edit-box";
    const title = document.createElement("div");
    title.className = "sftp-edit-title";
    title.textContent = `编辑 ${name}`;
    const ta = document.createElement("textarea");
    ta.className = "sftp-edit-ta";
    ta.value = initial;
    ta.spellcheck = false;
    const foot = document.createElement("div");
    foot.className = "sftp-edit-foot";
    const stat = document.createElement("span");
    stat.className = "sftp-edit-stat";
    const enc = new TextEncoder();
    const updateStat = () => {
      stat.textContent = `${[...ta.value].length} 字符 / ${enc.encode(ta.value).length} 字节 UTF-8`;
    };
    ta.addEventListener("input", updateStat);
    updateStat();
    const cancel = mkBtn("取消", () => back.remove());
    const save = mkBtn("保存", () => void this.saveEdit(path, ta.value, back, save));
    foot.append(stat, cancel, save);
    box.append(title, ta, foot);
    back.appendChild(box);
    // Esc 在编辑态 = 取消本对话框,不冒泡到面板级 Esc(否则会关整个面板并丢未保存编辑)。
    back.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        back.remove();
      }
    });
    this.el.appendChild(back);
    ta.focus();
  }

  private async saveEdit(
    path: string,
    content: string,
    back: HTMLElement,
    saveBtn: HTMLButtonElement,
  ): Promise<void> {
    if (!this.cfg) return;
    // 覆盖前确认(aterm 契约:显示字符/字节数,不可撤销)。
    const bytes = new TextEncoder().encode(content).length;
    if (!window.confirm(`保存到 ${path}?\n${[...content].length} 字符 / ${bytes} 字节 UTF-8,覆盖不可撤销。`)) {
      return;
    }
    saveBtn.disabled = true;
    try {
      await invoke("sftp_write_text", { cfg: this.cfg, path, content });
      back.remove(); // 成功才关,刷新目录
      await this.reload();
    } catch (e) {
      // 失败保留编辑框内容不丢(aterm 契约)。
      showActionFailureToast("保存失败", String(e));
      saveBtn.disabled = false;
    }
  }

  /** 写命令通用:invoke → 失败 toast → 成功刷新目录。 */
  private async doWrite(cmd: string, args: Record<string, unknown>): Promise<void> {
    try {
      await invoke(cmd, args);
      await this.reload();
    } catch (e) {
      showActionFailureToast("操作失败", String(e));
    }
  }

  // === 书签 + 在此打开终端(Part 4)===

  private origin(): string {
    return this.cfg?.label || this.cfg?.host || "?";
  }

  private loadBookmarks(): string[] {
    return safeGetJson<string[]>(LS_KEYS.sftpBookmarks(this.origin())) ?? [];
  }

  private saveBookmarks(list: string[]): void {
    safeSetJson(LS_KEYS.sftpBookmarks(this.origin()), list);
    this.renderBookmarks();
  }

  /** 当前目录 Pin/取消 Pin。 */
  private toggleBookmark(): void {
    const list = this.loadBookmarks();
    const has = list.includes(this.cwd) || list.includes(this.cwd.replace(/\/$/, ""));
    this.saveBookmarks(has ? removeBookmark(list, this.cwd) : addBookmark(list, this.cwd));
  }

  private renderBookmarks(): void {
    this.bookmarkBar.textContent = "";
    const list = this.loadBookmarks();
    for (const path of list) {
      const chip = document.createElement("span");
      chip.className = "sftp-bookmark";
      const go = document.createElement("button");
      go.type = "button";
      go.className = "sftp-bookmark-go";
      go.textContent = path;
      go.title = `跳到 ${path}`;
      go.addEventListener("click", () => void this.navigate(path));
      const del = document.createElement("button");
      del.type = "button";
      del.className = "sftp-bookmark-del";
      del.textContent = "×";
      del.title = "移除书签";
      del.addEventListener("click", () => this.saveBookmarks(removeBookmark(this.loadBookmarks(), path)));
      chip.append(go, del);
      this.bookmarkBar.appendChild(chip);
    }
  }

  /** 在此目录打开终端:wt.exe 起 ssh -t 落到当前 cwd(复用 F41 launch_remote_terminal)。 */
  private openTerminalHere(): void {
    if (!this.cfg) return;
    const remoteCmd = buildOpenTerminalCmd(this.cwd);
    // D 审计重要-1:launch_remote_terminal 按 origin 从**已保存**配置加载(需完整 host/user/
    // daemonPath);SFTP 浏览用的是即时 cfg(daemonPath 可空)。配置未存全时给可操作提示。
    void invoke("launch_remote_terminal", { origin: this.origin(), remoteCmd }).catch((e) => {
      const msg = String(e);
      showActionFailureToast(
        "打开终端失败",
        msg.includes("未找到远端配置")
          ? "该主机配置未完整保存——请在设置里填好 daemonPath 等字段(SFTP 浏览不需要,但打开终端需要完整配置)。"
          : msg,
      );
    });
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
      // 行内操作:下载(文件)/改名/删除。非 UTF-8 名(lossy)禁所有写+下载(无法寻址真字节)。
      const acts = document.createElement("span");
      acts.className = "sftp-row-acts";
      // D 审计建议-5:按钮区双击不冒泡到行 dblclick(否则双击按钮会连带进目录)。
      acts.addEventListener("dblclick", (ev) => ev.stopPropagation());
      if (!e.isDir) {
        acts.appendChild(mkRowBtn("下载", e.lossyName, () => void this.download(e)));
        acts.appendChild(mkRowBtn("编辑", e.lossyName, () => void this.editFile(e)));
      }
      acts.appendChild(mkRowBtn("改名", e.lossyName, () => void this.rename(e)));
      acts.appendChild(mkRowBtn("删除", e.lossyName, () => void this.remove(e)));
      row.appendChild(acts);
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

/** 行内小按钮;disabled=true(非 UTF-8 名)灰置禁点。 */
function mkRowBtn(label: string, disabled: boolean, onClick: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "sftp-btn sftp-row-btn";
  b.textContent = label;
  b.disabled = disabled;
  if (!disabled) b.addEventListener("click", onClick);
  return b;
}

/** 单例(整个 app 共用一个 overlay 实例)。 */
let singleton: SftpPanel | null = null;
export function openSftpPanel(cfg: RemoteHostConfig): void {
  singleton ??= new SftpPanel();
  void singleton.open(cfg);
}
