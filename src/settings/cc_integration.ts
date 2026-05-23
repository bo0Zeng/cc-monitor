/**
 * 设置面板：PowerShell 集成区。
 *
 * 提供 UI 让用户：
 *  - 扫描 PS 5.1 / PS 7.x profile 文件状态
 *  - 选自定义命令名（默认 cc）
 *  - 预览将要写入的代码块（含 BEGIN/END marker）
 *  - 一键安装 / 卸载 cc function
 *
 * 块边界设计：`# === cc-monitor BEGIN v1 ===` / `# === cc-monitor END ===`
 * 重装时块整体替换，卸载时块整体删除，用户在块外的任何内容不动。
 */

import { invoke } from "@tauri-apps/api/core";

type ProfileKind = "Ps51" | "Ps7";

interface ProfileScan {
  kind: ProfileKind;
  path: string;
  exists: boolean;
  has_ccm_block: boolean;
  ccm_block_version: string | null;
  conflicting_functions: string[];
  size_bytes: number;
}

interface CcStatusResponse {
  profiles: ProfileScan[];
  active_registrations: number;
  default_command_name: string;
}

interface CcPreviewResponse {
  code: string;
}

export class CcIntegrationSection {
  private root: HTMLElement;
  private commandInput!: HTMLInputElement;
  private rowsContainer!: HTMLElement;
  private regCountSpan!: HTMLSpanElement;
  private status: CcStatusResponse | null = null;

  constructor() {
    this.root = this.build();
    void this.refresh();
  }

  get element(): HTMLElement {
    return this.root;
  }

  private build(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group";

    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = "PowerShell 集成";
    group.appendChild(heading);

    // 行 1：说明
    const intro = document.createElement("div");
    intro.className = "settings-hint";
    intro.innerHTML =
      "用 <code>cc</code> 命令启动 claude，自动绑定 Tab ↔ 终端窗口拉前。" +
      "<br>不装也能用 monitor，但 Tab ↗ / Ctrl+\\` 拉前不工作。";
    group.appendChild(intro);

    // 行 2：命令名输入 + 扫描按钮
    const row1 = document.createElement("div");
    row1.className = "settings-row";
    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = "命令名";
    row1.appendChild(label);
    this.commandInput = document.createElement("input");
    this.commandInput.type = "text";
    this.commandInput.className = "settings-input";
    this.commandInput.value = "cc";
    this.commandInput.placeholder = "cc";
    this.commandInput.addEventListener("change", () => void this.refresh());
    row1.appendChild(this.commandInput);
    const refreshBtn = document.createElement("button");
    refreshBtn.type = "button";
    refreshBtn.className = "settings-btn settings-btn-secondary";
    refreshBtn.textContent = "扫描 profile";
    refreshBtn.addEventListener("click", () => void this.refresh());
    row1.appendChild(refreshBtn);
    group.appendChild(row1);

    // 行 3：当前活跃注册数
    const statRow = document.createElement("div");
    statRow.className = "settings-hint";
    statRow.textContent = "当前已注册 PowerShell session: ";
    this.regCountSpan = document.createElement("span");
    this.regCountSpan.textContent = "—";
    this.regCountSpan.style.fontWeight = "500";
    statRow.appendChild(this.regCountSpan);
    group.appendChild(statRow);

    // profile 行容器（refresh 时填充）
    this.rowsContainer = document.createElement("div");
    this.rowsContainer.className = "settings-cc-profiles";
    group.appendChild(this.rowsContainer);

    return group;
  }

  private async refresh(): Promise<void> {
    const cmd = this.sanitizedCommandName();
    try {
      this.status = await invoke<CcStatusResponse>("cc_integration_status", {
        commandName: cmd,
      });
      this.render();
    } catch (e) {
      console.error("cc_integration_status failed:", e);
      this.rowsContainer.innerHTML = `<div class="settings-hint">扫描失败：${escapeHtml(String(e))}</div>`;
    }
  }

  private sanitizedCommandName(): string {
    const v = this.commandInput.value.trim();
    return v.length === 0 ? "cc" : v;
  }

  private render(): void {
    if (!this.status) return;
    this.regCountSpan.textContent = String(this.status.active_registrations);
    this.rowsContainer.innerHTML = "";
    for (const p of this.status.profiles) {
      this.rowsContainer.appendChild(this.renderProfileRow(p));
    }
  }

  private renderProfileRow(p: ProfileScan): HTMLElement {
    const card = document.createElement("div");
    card.className = "settings-cc-profile-card";

    // 头部：名字 + 路径
    const head = document.createElement("div");
    head.className = "settings-cc-profile-head";
    const name = document.createElement("span");
    name.className = "settings-cc-profile-name";
    name.textContent = p.kind === "Ps51" ? "Windows PowerShell 5.1" : "PowerShell 7.x";
    head.appendChild(name);
    const path = document.createElement("span");
    path.className = "settings-cc-profile-path";
    path.textContent = p.path;
    path.title = p.path;
    head.appendChild(path);
    card.appendChild(head);

    // 状态行
    const status = document.createElement("div");
    status.className = "settings-cc-profile-status";
    status.appendChild(this.renderStatusBadge(p));
    card.appendChild(status);

    // 冲突警告
    if (p.conflicting_functions.length > 0 && !p.has_ccm_block) {
      const warn = document.createElement("div");
      warn.className = "settings-cc-profile-warn";
      warn.textContent = `⚠ profile 已有自定义 function ${p.conflicting_functions.join(", ")}。` +
        `安装会覆盖（或改命令名避免冲突）。`;
      card.appendChild(warn);
    }

    // 按钮行
    const buttons = document.createElement("div");
    buttons.className = "settings-cc-profile-buttons";
    const previewBtn = document.createElement("button");
    previewBtn.type = "button";
    previewBtn.className = "settings-btn settings-btn-secondary";
    previewBtn.textContent = "预览代码";
    previewBtn.addEventListener("click", () => void this.openPreview());
    buttons.appendChild(previewBtn);

    const installBtn = document.createElement("button");
    installBtn.type = "button";
    installBtn.className = "settings-btn";
    installBtn.textContent = p.has_ccm_block ? "重新安装" : "安装";
    installBtn.addEventListener("click", () => void this.install(p.kind));
    buttons.appendChild(installBtn);

    if (p.has_ccm_block) {
      const uninstallBtn = document.createElement("button");
      uninstallBtn.type = "button";
      uninstallBtn.className = "settings-btn settings-btn-secondary";
      uninstallBtn.textContent = "卸载";
      uninstallBtn.addEventListener("click", () => void this.uninstall(p.kind));
      buttons.appendChild(uninstallBtn);
    }
    card.appendChild(buttons);

    return card;
  }

  private renderStatusBadge(p: ProfileScan): HTMLElement {
    const badge = document.createElement("span");
    badge.className = "settings-cc-profile-badge";
    if (!p.exists) {
      badge.textContent = "○ profile 文件不存在（安装时自动创建）";
      badge.classList.add("settings-cc-badge-info");
    } else if (p.has_ccm_block) {
      badge.textContent = `✓ 已安装${p.ccm_block_version ? " (" + p.ccm_block_version + ")" : ""}`;
      badge.classList.add("settings-cc-badge-ok");
    } else {
      badge.textContent = "✗ 未安装";
      badge.classList.add("settings-cc-badge-warn");
    }
    return badge;
  }

  private async openPreview(): Promise<void> {
    const cmd = this.sanitizedCommandName();
    try {
      const resp = await invoke<CcPreviewResponse>("cc_integration_preview", {
        commandName: cmd,
      });
      this.showPreviewModal(resp.code, cmd);
    } catch (e) {
      console.error("preview failed:", e);
    }
  }

  private showPreviewModal(code: string, commandName: string): void {
    document.querySelector(".settings-cc-modal-backdrop")?.remove();
    const backdrop = document.createElement("div");
    backdrop.className = "settings-cc-modal-backdrop";
    const modal = document.createElement("div");
    modal.className = "settings-cc-modal";
    const title = document.createElement("div");
    title.className = "settings-cc-modal-title";
    title.textContent = `将要写入 PS profile 的代码（function ${commandName}）`;
    modal.appendChild(title);

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent = "BEGIN/END 之间是 cc-monitor 管理的块。你 profile 内的其他内容完全不动。";
    modal.appendChild(hint);

    const pre = document.createElement("pre");
    pre.className = "settings-cc-modal-code";
    pre.textContent = code;
    modal.appendChild(pre);

    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "settings-btn settings-btn-secondary";
    closeBtn.textContent = "关闭";
    closeBtn.addEventListener("click", () => backdrop.remove());
    const buttons = document.createElement("div");
    buttons.className = "settings-cc-modal-buttons";
    buttons.appendChild(closeBtn);
    modal.appendChild(buttons);

    backdrop.appendChild(modal);
    backdrop.addEventListener("click", (e) => {
      if (e.target === backdrop) backdrop.remove();
    });
    document.body.appendChild(backdrop);
  }

  private async install(kind: ProfileKind): Promise<void> {
    const cmd = this.sanitizedCommandName();
    try {
      await invoke<void>("cc_integration_install", { kind, commandName: cmd });
      await this.refresh();
      alert("已安装。请重启 PowerShell（关闭并新开窗口），新 session 启动时会自动注册。");
    } catch (e) {
      alert(`安装失败：${e}`);
    }
  }

  private async uninstall(kind: ProfileKind): Promise<void> {
    if (!confirm("确认卸载？BEGIN/END 块会被整块删除，profile 其他内容不动。")) return;
    try {
      await invoke<void>("cc_integration_uninstall", { kind });
      await this.refresh();
    } catch (e) {
      alert(`卸载失败：${e}`);
    }
  }
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
