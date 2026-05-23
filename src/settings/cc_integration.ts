/**
 * 设置面板：PowerShell 集成区（v1.7.2 重写）。
 *
 * UI 单卡片，让用户：
 *  - 选 PS 版本（默认 5.1，Windows 自带）或自定义路径
 *  - 编辑 profile 文件路径（默认填充 `Microsoft.PowerShell_profile.ps1`，可改）
 *  - 预览 / 扫描 / 安装 / 卸载
 *  - 看 v1.7.0-1.7.1 旧位置（profile.ps1）的遗留警告
 *  - 控制 auto-launch monitor toggle
 *
 * v1.7.0-1.7.1 的 bug：默认 profile 文件名搞成 `profile.ps1`（CurrentUserAllHosts），
 * 但 PowerShell 默认 `$PROFILE` 指向 `Microsoft.PowerShell_profile.ps1`
 * （CurrentUserCurrentHost）—— PowerShell 启动不读 `profile.ps1`，cc 集成形同虚设。
 * v1.7.2 改回正确文件名并扫描旧位置遗留。
 */

import { invoke } from "@tauri-apps/api/core";

type ProfileKind = "Ps51" | "Ps7" | "Custom";

interface ProfileScan {
  kind: ProfileKind;
  path: string;
  exists: boolean;
  has_ccm_block: boolean;
  ccm_block_version: string | null;
  conflicting_functions: string[];
  size_bytes: number;
}

interface LegacyEntry {
  kind: ProfileKind;
  path: string;
}

interface CcStatusResponse {
  profiles: ProfileScan[]; // 自动检测到的"推荐"路径列表（5.1 + 可选 7.x）
  active_registrations: number;
  default_command_name: string;
  legacy_profile_paths_with_block: LegacyEntry[];
}

interface CcPreviewResponse {
  code: string;
}

interface AutoLaunchConfig {
  auto_launch_enabled: boolean;
  monitor_exe_path: string | null;
}

export class CcIntegrationSection {
  private root: HTMLElement;
  private commandInput!: HTMLInputElement;
  private versionSelect!: HTMLSelectElement;
  private pathInput!: HTMLInputElement;
  private statusBadge!: HTMLSpanElement;
  private warnArea!: HTMLDivElement;
  private legacyArea!: HTMLDivElement;
  private regCountSpan!: HTMLSpanElement;
  private installBtn!: HTMLButtonElement;
  private uninstallBtn!: HTMLButtonElement;
  private autoLaunchCheckbox!: HTMLInputElement;
  private autoLaunchPathSpan!: HTMLSpanElement;
  private includeCcFnCheckbox!: HTMLInputElement;
  private includeCcFnHint!: HTMLDivElement;
  /** 当前从后端拿到的推荐路径（按 PS 版本索引）。版本下拉改时用来回填 path 输入 */
  private recommended: Record<ProfileKind, string | null> = {
    Ps51: null,
    Ps7: null,
    Custom: null,
  };

  constructor() {
    this.root = this.build();
    void this.refresh();
    void this.refreshAutoLaunch();
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

    // 说明
    const intro = document.createElement("div");
    intro.className = "settings-hint";
    intro.innerHTML =
      "用 <code>cc</code> 命令启动 claude，自动绑定 Tab ↔ 终端窗口拉前。" +
      "<br>不装也能用 monitor，但 Tab ↗ / Ctrl+\\` 拉前不工作。";
    group.appendChild(intro);

    // 命令名
    const rowCmd = document.createElement("div");
    rowCmd.className = "settings-row";
    const lblCmd = document.createElement("span");
    lblCmd.className = "settings-label";
    lblCmd.textContent = "命令名";
    rowCmd.appendChild(lblCmd);
    this.commandInput = document.createElement("input");
    this.commandInput.type = "text";
    this.commandInput.className = "settings-input";
    this.commandInput.value = "cc";
    this.commandInput.addEventListener("change", () => void this.scanCurrentPath());
    rowCmd.appendChild(this.commandInput);
    group.appendChild(rowCmd);

    // PowerShell 版本下拉
    const rowVer = document.createElement("div");
    rowVer.className = "settings-row";
    const lblVer = document.createElement("span");
    lblVer.className = "settings-label";
    lblVer.textContent = "PowerShell";
    rowVer.appendChild(lblVer);
    this.versionSelect = document.createElement("select");
    this.versionSelect.className = "settings-input";
    [
      { v: "Ps51", t: "Windows PowerShell 5.1 （Windows 自带，推荐）" },
      { v: "Ps7", t: "PowerShell 7.x（独立安装）" },
      { v: "Custom", t: "自定义路径..." },
    ].forEach((opt) => {
      const o = document.createElement("option");
      o.value = opt.v;
      o.textContent = opt.t;
      this.versionSelect.appendChild(o);
    });
    this.versionSelect.value = "Ps51";
    this.versionSelect.addEventListener("change", () => this.onVersionChange());
    rowVer.appendChild(this.versionSelect);
    group.appendChild(rowVer);

    // profile 路径
    const rowPath = document.createElement("div");
    rowPath.className = "settings-row settings-row-stack";
    const lblPath = document.createElement("span");
    lblPath.className = "settings-label";
    lblPath.textContent = "Profile 路径";
    rowPath.appendChild(lblPath);
    this.pathInput = document.createElement("input");
    this.pathInput.type = "text";
    this.pathInput.className = "settings-input settings-input-wide";
    this.pathInput.placeholder = "...\\Documents\\WindowsPowerShell\\Microsoft.PowerShell_profile.ps1";
    this.pathInput.addEventListener("change", () => void this.scanCurrentPath());
    rowPath.appendChild(this.pathInput);
    group.appendChild(rowPath);

    // 路径提示
    const pathHint = document.createElement("div");
    pathHint.className = "settings-hint";
    pathHint.innerHTML =
      "默认填充 PowerShell 启动时实际读的 <code>$PROFILE</code>（即 <code>Microsoft.PowerShell_profile.ps1</code>）。" +
      "在 PS 里跑 <code>$PROFILE</code> 看你机器上具体路径。";
    group.appendChild(pathHint);

    // 状态行
    const rowStatus = document.createElement("div");
    rowStatus.className = "settings-cc-profile-status";
    this.statusBadge = document.createElement("span");
    this.statusBadge.className = "settings-cc-profile-badge";
    this.statusBadge.textContent = "...";
    rowStatus.appendChild(this.statusBadge);
    group.appendChild(rowStatus);

    // 冲突警告
    this.warnArea = document.createElement("div");
    group.appendChild(this.warnArea);

    // v1.7.3：是否安装 function cc（用户已有自定义 cc 时默认不装，避免覆盖）
    const ccFnRow = document.createElement("div");
    ccFnRow.className = "settings-cc-include-cc";
    const ccFnLabel = document.createElement("label");
    ccFnLabel.className = "settings-row";
    this.includeCcFnCheckbox = document.createElement("input");
    this.includeCcFnCheckbox.type = "checkbox";
    this.includeCcFnCheckbox.className = "settings-checkbox";
    this.includeCcFnCheckbox.checked = true;
    this.includeCcFnCheckbox.addEventListener("change", () => this.updateIncludeHint());
    ccFnLabel.appendChild(this.includeCcFnCheckbox);
    const ccFnTxt = document.createElement("span");
    ccFnTxt.className = "settings-checkbox-label";
    ccFnTxt.textContent = "同时安装一个默认 function cc（一键即用）";
    ccFnLabel.appendChild(ccFnTxt);
    ccFnRow.appendChild(ccFnLabel);
    this.includeCcFnHint = document.createElement("div");
    this.includeCcFnHint.className = "settings-hint";
    ccFnRow.appendChild(this.includeCcFnHint);
    group.appendChild(ccFnRow);

    // 按钮行
    const btnRow = document.createElement("div");
    btnRow.className = "settings-cc-profile-buttons";
    const previewBtn = document.createElement("button");
    previewBtn.type = "button";
    previewBtn.className = "settings-btn settings-btn-secondary";
    previewBtn.textContent = "预览代码";
    previewBtn.addEventListener("click", () => void this.openPreview());
    btnRow.appendChild(previewBtn);

    const scanBtn = document.createElement("button");
    scanBtn.type = "button";
    scanBtn.className = "settings-btn settings-btn-secondary";
    scanBtn.textContent = "重新扫描";
    scanBtn.addEventListener("click", () => void this.scanCurrentPath(true));
    btnRow.appendChild(scanBtn);

    this.installBtn = document.createElement("button");
    this.installBtn.type = "button";
    this.installBtn.className = "settings-btn";
    this.installBtn.textContent = "安装";
    this.installBtn.addEventListener("click", () => void this.install());
    btnRow.appendChild(this.installBtn);

    this.uninstallBtn = document.createElement("button");
    this.uninstallBtn.type = "button";
    this.uninstallBtn.className = "settings-btn settings-btn-secondary";
    this.uninstallBtn.textContent = "卸载";
    this.uninstallBtn.style.display = "none";
    this.uninstallBtn.addEventListener("click", () => void this.uninstall());
    btnRow.appendChild(this.uninstallBtn);
    group.appendChild(btnRow);

    // 活跃注册数
    const statRow = document.createElement("div");
    statRow.className = "settings-hint";
    statRow.textContent = "当前已注册 PowerShell session: ";
    this.regCountSpan = document.createElement("span");
    this.regCountSpan.textContent = "—";
    this.regCountSpan.style.fontWeight = "500";
    statRow.appendChild(this.regCountSpan);
    group.appendChild(statRow);

    // v1.7.0-1.7.1 旧位置遗留警告
    this.legacyArea = document.createElement("div");
    group.appendChild(this.legacyArea);

    // auto-launch toggle
    group.appendChild(this.buildAutoLaunchRow());

    return group;
  }

  private buildAutoLaunchRow(): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "settings-cc-autolaunch";

    const row = document.createElement("label");
    row.className = "settings-row";

    this.autoLaunchCheckbox = document.createElement("input");
    this.autoLaunchCheckbox.type = "checkbox";
    this.autoLaunchCheckbox.className = "settings-checkbox";
    this.autoLaunchCheckbox.addEventListener("change", () => {
      void this.toggleAutoLaunch(this.autoLaunchCheckbox.checked);
    });
    row.appendChild(this.autoLaunchCheckbox);

    const label = document.createElement("span");
    label.className = "settings-checkbox-label";
    label.textContent = "用 cc 启动 claude 时自动打开 monitor（如果未在跑）";
    row.appendChild(label);

    wrap.appendChild(row);

    const hint = document.createElement("div");
    hint.className = "settings-hint settings-cc-autolaunch-path";
    hint.textContent = "monitor 路径: ";
    this.autoLaunchPathSpan = document.createElement("span");
    this.autoLaunchPathSpan.className = "settings-cc-autolaunch-path-value";
    this.autoLaunchPathSpan.textContent = "—";
    hint.appendChild(this.autoLaunchPathSpan);
    wrap.appendChild(hint);

    return wrap;
  }

  private currentCommand(): string {
    return this.commandInput.value.trim() || "cc";
  }

  /** 打开面板时调用：拿推荐路径 + 填默认值 + 扫一遍当前路径状态 */
  private async refresh(): Promise<void> {
    try {
      const status = await invoke<CcStatusResponse>("cc_integration_status", {
        commandName: this.currentCommand(),
      });
      // 把推荐路径填入 lookup
      this.recommended.Ps51 = null;
      this.recommended.Ps7 = null;
      for (const p of status.profiles) {
        this.recommended[p.kind] = p.path;
      }
      // 如果只检测到 PS 5.1，下拉里 PS 7.x 也保留但路径会留空
      // 默认选 PS 5.1
      if (!this.pathInput.value) {
        this.versionSelect.value = "Ps51";
        this.pathInput.value = this.recommended.Ps51 ?? "";
      }
      this.regCountSpan.textContent = String(status.active_registrations);
      this.renderLegacy(status.legacy_profile_paths_with_block);
      await this.scanCurrentPath();
    } catch (e) {
      this.statusBadge.textContent = `扫描失败: ${e}`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-warn";
    }
  }

  /** 用户改了路径输入框或命令名时 / 重新扫描按钮 */
  private async scanCurrentPath(notify = false): Promise<void> {
    const p = this.pathInput.value.trim();
    if (!p) {
      this.statusBadge.textContent = "（请选 PS 版本或填路径）";
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-info";
      return;
    }
    try {
      const scan = await invoke<ProfileScan>("cc_integration_scan_path", {
        path: p,
        commandName: this.currentCommand(),
      });
      this.renderScanResult(scan);
      if (notify) {
        this.flashScan();
      }
    } catch (e) {
      this.statusBadge.textContent = `扫描失败: ${e}`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-warn";
    }
  }

  private renderScanResult(scan: ProfileScan): void {
    this.warnArea.innerHTML = "";
    if (!scan.exists) {
      this.statusBadge.textContent = "○ profile 文件不存在（安装时自动创建）";
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-info";
    } else if (scan.has_ccm_block) {
      const ver = scan.ccm_block_version ? ` (${scan.ccm_block_version})` : "";
      this.statusBadge.textContent = `✓ 已安装${ver}`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-ok";
    } else {
      this.statusBadge.textContent = "✗ 未安装";
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-warn";
    }
    // v1.7.3：检测到用户自定义 function cc 时，默认取消 "也装 cc function" 复选框
    // 避免我们的 cc 覆盖用户的（PowerShell 后定义的同名 function 覆盖前面的）
    if (scan.conflicting_functions.length > 0 && !scan.has_ccm_block) {
      const warn = document.createElement("div");
      warn.className = "settings-cc-profile-warn";
      warn.innerHTML =
        `⚠ profile 已含自定义 function <code>${scan.conflicting_functions.join(", ")}</code>。` +
        `<br>cc-monitor 不会覆盖它——下方"也装默认 function cc"会自动取消勾选，` +
        `仅安装 <code>__ccm_bind</code> helper。` +
        `<br><b>请在你的 <code>function cc</code> 开头加一行调用：<code>__ccm_bind</code></b>。`;
      this.warnArea.appendChild(warn);
      this.includeCcFnCheckbox.checked = false;
    } else if (!scan.has_ccm_block) {
      this.includeCcFnCheckbox.checked = true;
    }
    this.updateIncludeHint();
    this.installBtn.textContent = scan.has_ccm_block ? "重新安装" : "安装";
    this.uninstallBtn.style.display = scan.has_ccm_block ? "" : "none";
  }

  private updateIncludeHint(): void {
    if (this.includeCcFnCheckbox.checked) {
      this.includeCcFnHint.innerHTML =
        "勾上：安装 <code>__ccm_bind</code> + <code>function cc</code>（` __ccm_bind; claude $args`）。新用户一键即用。";
    } else {
      this.includeCcFnHint.innerHTML =
        "未勾：只安装 <code>__ccm_bind</code> helper。请在你自己的 cc function 开头加 <code>__ccm_bind</code> 调用。";
    }
  }

  private renderLegacy(entries: LegacyEntry[]): void {
    this.legacyArea.innerHTML = "";
    if (entries.length === 0) return;
    const warn = document.createElement("div");
    warn.className = "settings-cc-legacy-warn";
    const head = document.createElement("div");
    head.style.fontWeight = "600";
    head.textContent = "⚠ 检测到 v1.7.0-1.7.1 旧位置遗留的 cc-monitor 块";
    warn.appendChild(head);
    const body = document.createElement("div");
    body.style.marginTop = "4px";
    body.textContent =
      "v1.7.0-1.7.1 错把 cc function 装到 profile.ps1（PowerShell 启动时不读），实际无效。" +
      "请手动删除这些文件（或保留但删除 BEGIN/END 之间的内容），然后用上面的安装按钮装到正确位置：";
    warn.appendChild(body);
    const list = document.createElement("ul");
    list.style.marginTop = "4px";
    list.style.marginLeft = "16px";
    list.style.fontFamily = "var(--font-mono, monospace)";
    list.style.fontSize = "11px";
    for (const e of entries) {
      const li = document.createElement("li");
      li.textContent = e.path;
      list.appendChild(li);
    }
    warn.appendChild(list);
    this.legacyArea.appendChild(warn);
  }

  private flashScan(): void {
    const prev = this.statusBadge.style.outline;
    this.statusBadge.style.outline = "2px solid var(--accent, #c25b3b)";
    window.setTimeout(() => {
      this.statusBadge.style.outline = prev;
    }, 500);
  }

  private onVersionChange(): void {
    const v = this.versionSelect.value as ProfileKind;
    if (v === "Custom") {
      // 保留当前路径不变，等用户编辑
      this.pathInput.focus();
      return;
    }
    const rec = this.recommended[v];
    if (rec) {
      this.pathInput.value = rec;
      void this.scanCurrentPath();
    } else {
      this.pathInput.value = "";
      this.statusBadge.textContent = `（${v === "Ps7" ? "PS 7.x" : "PS 5.1"} 没自动检测到）`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-info";
    }
  }

  private async openPreview(): Promise<void> {
    try {
      const resp = await invoke<CcPreviewResponse>("cc_integration_preview", {
        commandName: this.currentCommand(),
        includeCcFunction: this.includeCcFnCheckbox.checked,
      });
      this.showPreviewModal(resp.code);
    } catch (e) {
      console.error("preview failed:", e);
    }
  }

  private showPreviewModal(code: string): void {
    document.querySelector(".settings-cc-modal-backdrop")?.remove();
    const backdrop = document.createElement("div");
    backdrop.className = "settings-cc-modal-backdrop";
    const modal = document.createElement("div");
    modal.className = "settings-cc-modal";
    const title = document.createElement("div");
    title.className = "settings-cc-modal-title";
    title.textContent = `将要写入 profile 的代码（function ${this.currentCommand()}）`;
    modal.appendChild(title);

    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent = "BEGIN/END 之间是 cc-monitor 管理的块。profile 内其他内容完全不动。";
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

  private async install(): Promise<void> {
    const p = this.pathInput.value.trim();
    if (!p) {
      alert("请先填 profile 路径");
      return;
    }
    const includeCc = this.includeCcFnCheckbox.checked;
    try {
      await invoke<void>("cc_integration_install", {
        path: p,
        commandName: this.currentCommand(),
        includeCcFunction: includeCc,
      });
      await this.scanCurrentPath();
      const tail = includeCc
        ? "请重启 PowerShell，cc 命令立即可用。"
        : "已装 __ccm_bind helper。请在你自己的 function cc 开头加一行 `__ccm_bind`，然后重启 PowerShell。";
      alert("已写入 profile。" + tail);
    } catch (e) {
      alert(`安装失败：${e}`);
    }
  }

  private async uninstall(): Promise<void> {
    if (!confirm("确认卸载？BEGIN/END 块会被整块删除，profile 其他内容不动。")) return;
    try {
      await invoke<void>("cc_integration_uninstall", {
        path: this.pathInput.value.trim(),
      });
      await this.scanCurrentPath();
    } catch (e) {
      alert(`卸载失败：${e}`);
    }
  }

  private async refreshAutoLaunch(): Promise<void> {
    try {
      const cfg = await invoke<AutoLaunchConfig>("cc_get_auto_launch");
      this.autoLaunchCheckbox.checked = cfg.auto_launch_enabled;
      this.autoLaunchPathSpan.textContent =
        cfg.monitor_exe_path ?? "(未记录，重启一次 monitor 后会自动记录)";
      this.autoLaunchPathSpan.title = cfg.monitor_exe_path ?? "";
    } catch (e) {
      console.warn("cc_get_auto_launch failed:", e);
    }
  }

  private async toggleAutoLaunch(enabled: boolean): Promise<void> {
    try {
      await invoke<void>("cc_set_auto_launch", { enabled });
    } catch (e) {
      alert(`保存失败：${e}`);
      this.autoLaunchCheckbox.checked = !enabled;
    }
  }
}
