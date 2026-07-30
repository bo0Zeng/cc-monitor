/**
 * F48：SFTP 文件面板(独立 overlay,每 host 打开;照 SettingsPanel 范式,不碰 TabManager)。
 * 消费 F47 的 sftp_* 命令。Part 1 = 浏览(面包屑/列表/导航/排序);传输/写/拖入见后续。
 */
import { Channel } from "@tauri-apps/api/core";
import { commands } from "../ipc/commands";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { formatBytes } from "../format";
import { showActionFailureToast } from "../error-toast";
import { LS_KEYS, safeGetJson, safeSetJson } from "../local-storage";
import { buildOpenTerminalCmd } from "../remote-launch";
import type { RemoteHostConfig } from "../remote-config";
// F82a 修复：接入 dispatcher overlay 栈——SFTP 面板过去用自己的 document keydown 判 Esc，不入栈；
// 在独立设置窗口里 Esc 会被 dispatcher 路由到栈底的设置面板→关整窗（双关窗）。改为标准 overlay：
// pushOverlay/popOverlay + handleEsc，Esc 由 dispatcher LIFO 只关最上层（同 KeybindingsEditor 范式）。
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
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

// C03：改成 import 生成物（源：`sftp_pool.rs::TransferProgress`）。
// 两个字段在 Rust 侧是 `u64`，由 `#[ts(type = "number")]` 显式收窄（附上限论证）。
import type { TransferProgress } from "../generated/TransferProgress";

export class SftpPanel implements OverlayHandle {
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
  /** F54:open(revealPath) 一次性定位——reload 后高亮该 basename 那行并滚入视图,随即清空。 */
  private revealName: string | null = null;

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
    // Esc 关闭改由 dispatcher overlay 栈驱动（见 handleEsc / open 里的 pushOverlay）——不再自挂
    // document keydown（那会与 dispatcher 的 window-capture Esc 双触发，在独立设置窗里双关窗）。
  }

  /**
   * OverlayHandle：Esc 命中且本面板在栈顶时。**编辑对话框（`.sftp-edit-back`）打开时 Esc = 取消它**
   * （不关整个面板、不丢未保存编辑）；否则关面板。
   * 为何在此判而非编辑框自挂 keydown：dispatcher 在 window **capture** 相位先于任何 bubble 监听触发，
   * 编辑框的 `stopPropagation` 拦不住它，故 Esc 分流必须收在这个栈顶 handler 里。
   */
  handleEsc(): void {
    const editBack = this.el.querySelector<HTMLElement>(".sftp-edit-back");
    if (editBack) {
      editBack.remove(); // 取消编辑对话框
      return;
    }
    this.close();
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

  /**
   * 打开面板并浏览该 host。默认以 realpath('.') 为起点;F54:传 `revealPath`(远端文件绝对
   * 路径)则直接定位到其父目录并在列表里高亮该文件(会话工具卡→文件跳转)。
   * F78:传 `initialDir`(远端目录绝对路径)则**直接进入该目录**(远端会话「打开工作目录」),
   * 不高亮、不 realpath——与 revealPath(文件)语义区分。三者优先级 initialDir > revealPath > home。
   */
  async open(cfg: RemoteHostConfig, revealPath?: string, initialDir?: string): Promise<void> {
    this.cfg = cfg;
    this.titleEl.textContent = `文件:${cfg.label || cfg.host}`;
    this.el.style.display = "flex";
    dispatcher.pushOverlay(this); // 入栈顶（pushOverlay 自去重；重开则移到栈顶）

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
    const dir = initialDir?.trim();
    const reveal = revealPath?.trim();
    if (dir) {
      // F78:直接进入该远端目录(会话工作目录),不高亮、不 realpath(绝对路径)。
      this.revealName = null;
      this.cwd = dir;
    } else if (reveal) {
      // F54:远端文件绝对路径 → 直接定位父目录 + 记高亮目标(无需 realpath)。
      this.cwd = parentPath(reveal);
      this.revealName = basename(reveal);
    } else {
      this.revealName = null;
      try {
        const home = await commands.sftp_realpath({ cfg, path: "." });
        this.cwd = home || "/";
      } catch (e) {
        this.cwd = "/";
        showActionFailureToast("SFTP 连接失败", String(e));
      }
    }
    await this.reload();
  }

  close(): void {
    this.el.style.display = "none";
    dispatcher.popOverlay(this); // 出栈（popOverlay 对不在栈者安全无操作）
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
      this.entries = await commands.sftp_list_dir({
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
      commands.sftp_download({ cfg: this.cfg, remotePath, localPath, transferId, onProgress }),
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
      await commands.sftp_stat({ cfg: this.cfg, path: remotePath });
      exists = true;
    } catch {
      exists = false; // stat 失败 = 不存在(或不可读),按新建处理
    }
    if (exists && !window.confirm(`远端已存在 ${name},覆盖?`)) return;
    await this.runTransfer(`上传 ${name}`, 0, (transferId, onProgress) =>
      commands.sftp_upload({ cfg: this.cfg, localPath, remotePath, transferId, onProgress }),
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
      void commands.sftp_cancel_transfer({ transferId });
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
    await this.doWrite(() =>
      commands.sftp_mkdir({ cfg: this.cfg, path: joinPath(this.cwd, name.trim()) }),
    );
  }

  private async rename(e: SftpEntry): Promise<void> {
    if (!this.cfg) return;
    const to = window.prompt(`重命名 ${e.name} 为:`, e.name);
    if (!to?.trim() || to.trim() === e.name) return;
    await this.doWrite(() =>
      commands.sftp_rename({
        cfg: this.cfg,
        from: joinPath(this.cwd, e.name),
        to: joinPath(this.cwd, to.trim()),
      }),
    );
  }

  private async remove(e: SftpEntry): Promise<void> {
    if (!this.cfg) return;
    // 二次确认,文案回显真实条目名(aterm 契约:防误删)。
    const kind = e.isDir ? "目录" : "文件";
    if (!window.confirm(`删除${kind} ${e.name}?此操作不可撤销。`)) return;
    await this.doWrite(() =>
      commands.sftp_delete({
        cfg: this.cfg,
        path: joinPath(this.cwd, e.name),
        isDir: e.isDir,
      }),
    );
  }

  /** 拖入的本地文件 → 逐个上传到当前目录(复用 runTransfer)。 */
  private async uploadDropped(paths: string[]): Promise<void> {
    if (!this.cfg) return;
    for (const localPath of paths) {
      const name = basename(localPath);
      const remotePath = joinPath(this.cwd, name);
      let exists = false;
      try {
        await commands.sftp_stat({ cfg: this.cfg, path: remotePath });
        exists = true;
      } catch {
        exists = false;
      }
      if (exists && !window.confirm(`远端已存在 ${name},覆盖?`)) continue;
      await this.runTransfer(`上传 ${name}`, 0, (transferId, onProgress) =>
        commands.sftp_upload({ cfg: this.cfg, localPath, remotePath, transferId, onProgress }),
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
      text = await commands.sftp_read_text_for_edit({ cfg: this.cfg, path });
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
    // Esc 在编辑态 = 取消本对话框：改由 SftpPanel.handleEsc（overlay 栈顶）统一分流——dispatcher 是
    // window capture 相位，早于此处 bubble 监听，编辑框自挂 keydown 拦不住，故 Esc 逻辑收在 handleEsc。
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
      await commands.sftp_write_text({ cfg: this.cfg, path, content });
      back.remove(); // 成功才关,刷新目录
      await this.reload();
    } catch (e) {
      // 失败保留编辑框内容不丢(aterm 契约)。
      showActionFailureToast("保存失败", String(e));
      saveBtn.disabled = false;
    }
  }

  /**
   * 写命令通用:调用 → 失败 toast → 成功刷新目录。
   *
   * **C04d 批 6b：签名从 `(cmd: string, args: Record<string, unknown>)` 改成接一个 thunk。**
   * 原形态被 C04a 记成「TS 静态看不见的动态命令名」盲区之一，但**三个调用方传的全是字面量**
   * （`sftp_mkdir`/`sftp_rename`/`sftp_delete`）——「动态」只在于名字从一个封闭集合里选。
   * 改成 thunk 后：命令名回到调用点成为字面量（守卫扫得到）、
   * **每个命令的实参由包装层各自的精确签名把关**（不再是 `Record<string, unknown>` 一锅端）。
   * 这比原计划的 `invokeDynamic(name, args)` 逃生口好——那会留一个 `string` 键的后门。
   */
  private async doWrite(run: () => Promise<void>): Promise<void> {
    try {
      await run();
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
    void commands.launch_remote_terminal({ origin: this.origin(), remoteCmd }).catch((e) => {
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
    // F54:一次性消费高亮目标(下次 reload/浏览不再高亮)。
    const reveal = this.revealName;
    this.revealName = null;
    const sorted = sortEntries(this.entries, this.sortBy);
    if (sorted.length === 0) {
      const empty = document.createElement("div");
      empty.className = "sftp-empty";
      empty.textContent = "(空目录)";
      this.listEl.appendChild(empty);
      return;
    }
    let revealRow: HTMLElement | null = null;
    for (const e of sorted) {
      const row = document.createElement("div");
      row.className = "sftp-row";
      row.classList.toggle("sftp-lossy", e.lossyName);
      if (reveal && e.name === reveal) {
        row.classList.add("sftp-row-reveal"); // F54:会话工具卡跳来的那个文件
        revealRow = row;
      }
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
    if (revealRow) revealRow.scrollIntoView({ block: "center" });
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
/** 打开 SFTP 面板。F54:传 `revealPath`(远端文件绝对路径)则定位到其父目录并高亮该文件。 */
export function openSftpPanel(cfg: RemoteHostConfig, revealPath?: string): void {
  singleton ??= new SftpPanel();
  void singleton.open(cfg, revealPath);
}

/** F78:打开 SFTP 面板并**直接进入**指定远端目录(远端会话「打开工作目录」用；绝对路径)。 */
export function openSftpPanelDir(cfg: RemoteHostConfig, dir: string): void {
  singleton ??= new SftpPanel();
  void singleton.open(cfg, undefined, dir);
}
