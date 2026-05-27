/**
 * 设置面板：PowerShell 集成区（v1.7.9 简化版）。
 *
 * v1.7.9 UI 改动：
 *  - 命令名固定 "cc"（不再让用户输自由文本，避免填错 / 跟 claude.exe 同名等坑）
 *  - "同时安装 cc wrapper" 改成复选框，**默认不勾选**（默认只装 helper，不覆盖用户已有 wrapper）
 *  - 所有冗长说明 → ! 图标 hover tooltip，UI 干净
 *
 * v1.7.0-1.7.1 的 bug：默认 profile 文件名搞成 `profile.ps1`（CurrentUserAllHosts），
 * 但 PowerShell 默认 `$PROFILE` 指向 `Microsoft.PowerShell_profile.ps1`
 * （CurrentUserCurrentHost）—— PowerShell 启动不读 `profile.ps1`，cc 集成形同虚设。
 * v1.7.2 改回正确文件名并扫描旧位置遗留。
 */

import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { makeInfoIcon, swapFileName } from "./info-icon";
import { showActionFailureToast } from "../error-toast";
import { LS_KEYS, safeGet, safeSet } from "../local-storage";

type ProfileKind = "Ps51" | "Ps7" | "Custom";

/** v1.7.12: 前端用的预设 id，覆盖 PS 版本 × profile scope 矩阵 */
type PresetId =
  | "Ps51-CurrentHost"
  | "Ps51-AllHosts"
  | "Ps7-CurrentHost"
  | "Ps7-AllHosts"
  | "Custom";

const PRESET_OPTIONS: Array<{ id: PresetId; label: string }> = [
  { id: "Ps51-CurrentHost", label: "PowerShell 5.1 - $PROFILE（默认）" },
  { id: "Ps51-AllHosts", label: "PowerShell 5.1 - 所有 host（profile.ps1）" },
  { id: "Ps7-CurrentHost", label: "PowerShell 7.x - $PROFILE" },
  { id: "Ps7-AllHosts", label: "PowerShell 7.x - 所有 host" },
  { id: "Custom", label: "自定义路径..." },
];

// P2.1: 通过 LS_KEYS 中转保留下划线 key 名（迁移成本大于收益）。

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
  profiles: ProfileScan[];
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

/** 固定命令名 —— 不让用户改。`claude` 跟 claude.exe 同名会无限递归；其他名字没必要让用户折腾 */
const CC_COMMAND_NAME = "cc";

export class CcIntegrationSection {
  private root: HTMLElement;
  private versionSelect!: HTMLSelectElement;
  private pathInput!: HTMLInputElement;
  private statusBadge!: HTMLSpanElement;
  private warnArea!: HTMLDivElement;
  private legacyArea!: HTMLDivElement;
  private regCountSpan!: HTMLSpanElement;
  private wrapperCheckbox!: HTMLInputElement;
  private installBtn!: HTMLButtonElement;
  private uninstallBtn!: HTMLButtonElement;
  private autoLaunchCheckbox!: HTMLInputElement;
  private autoLaunchPathSpan!: HTMLSpanElement;
  /** 当前从后端拿到的推荐路径（仅 PS 版本，CurrentHost 那条）。AllHosts 路径前端推算同目录的 profile.ps1 */
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

    // 标题 + 旁边 ! 图标显示原理
    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = "PowerShell 集成";
    heading.appendChild(
      makeInfoIcon(
        "在 PowerShell profile 里装 __ccm_bind helper：启动 claude 时把当前终端 HWND 注册给 monitor，" +
          "之后 Tab ↗ 跳焦能精确拉对应终端窗口。不装也能用 monitor，但拉前不工作。",
      ),
    );
    group.appendChild(heading);

    // PowerShell 版本下拉
    const rowVer = document.createElement("div");
    rowVer.className = "settings-row";
    const lblVer = document.createElement("span");
    lblVer.className = "settings-label";
    lblVer.textContent = "PowerShell";
    rowVer.appendChild(lblVer);
    this.versionSelect = document.createElement("select");
    this.versionSelect.className = "settings-input";
    for (const opt of PRESET_OPTIONS) {
      const o = document.createElement("option");
      o.value = opt.id;
      o.textContent = opt.label;
      this.versionSelect.appendChild(o);
    }
    this.versionSelect.value = "Ps51-CurrentHost";
    this.versionSelect.addEventListener("change", () => this.onVersionChange());
    rowVer.appendChild(this.versionSelect);
    rowVer.appendChild(
      makeInfoIcon(
        "AllHosts (profile.ps1)：所有 PowerShell host 都读 —— powershell.exe / pwsh.exe / VSCode 终端 / ISE / SSH 都生效。\n\n" +
          "$PROFILE / CurrentHost (Microsoft.PowerShell_profile.ps1)：只有 powershell.exe / pwsh.exe 控制台读，VSCode/ISE 不读自己的同名文件。\n\n" +
          "推荐 AllHosts —— cc 函数在哪个终端都用。",
      ),
    );
    group.appendChild(rowVer);

    // profile 路径
    const rowPath = document.createElement("div");
    rowPath.className = "settings-row settings-row-stack";
    const lblPath = document.createElement("span");
    lblPath.className = "settings-label";
    lblPath.textContent = "Profile";
    lblPath.appendChild(
      makeInfoIcon(
        "PowerShell 启动时读的脚本文件。默认填 $PROFILE 实际指向的 " +
          "Microsoft.PowerShell_profile.ps1（CurrentUserCurrentHost）。在 PS 里跑 $PROFILE 看你机器上具体路径。",
      ),
    );
    rowPath.appendChild(lblPath);
    this.pathInput = document.createElement("input");
    this.pathInput.type = "text";
    this.pathInput.className = "settings-input settings-input-wide";
    this.pathInput.placeholder = "...\\Documents\\WindowsPowerShell\\Microsoft.PowerShell_profile.ps1";
    this.pathInput.addEventListener("change", () => {
      // 用户手编路径：保存到 localStorage，下次打开记得
      try {
        safeSet(LS_KEYS.profilePath, this.pathInput.value.trim());
        safeSet(LS_KEYS.profilePreset, "Custom");
      } catch {}
      void this.scanCurrentPath();
    });
    rowPath.appendChild(this.pathInput);
    group.appendChild(rowPath);

    // ★ v1.7.9：是否同时装 wrapper 复选框（默认不勾选）
    const wrapperRow = document.createElement("label");
    wrapperRow.className = "settings-row settings-row-checkbox";
    this.wrapperCheckbox = document.createElement("input");
    this.wrapperCheckbox.type = "checkbox";
    this.wrapperCheckbox.className = "settings-checkbox";
    this.wrapperCheckbox.checked = false; // 默认不覆盖
    this.wrapperCheckbox.addEventListener("change", () => void this.scanCurrentPath());
    wrapperRow.appendChild(this.wrapperCheckbox);
    const wrapperLabel = document.createElement("span");
    wrapperLabel.className = "settings-checkbox-label";
    wrapperLabel.textContent = "同时安装 cc wrapper";
    wrapperRow.appendChild(wrapperLabel);
    wrapperRow.appendChild(
      makeInfoIcon(
        "默认不勾选：只装 __ccm_bind helper。你需要在自己已有的 claude 启动 wrapper（function cc / function claude / 别名等）开头加一行 __ccm_bind 来触发绑定。\n\n" +
          "勾选：额外装 function cc { __ccm_bind; & claude $args }。\n" +
          "⚠ 如果你 profile 里已有 function cc 会被替换（cc-monitor 自己的块在 profile 后端，PowerShell function 后定义的会覆盖前面同名的）。",
      ),
    );
    group.appendChild(wrapperRow);

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

    // 按钮行
    const btnRow = document.createElement("div");
    btnRow.className = "settings-cc-profile-buttons";

    const previewBtn = document.createElement("button");
    previewBtn.type = "button";
    previewBtn.className = "settings-btn settings-btn-secondary";
    previewBtn.textContent = "预览代码";
    previewBtn.title = "看一眼即将写入 profile 的 BEGIN/END 块内容";
    previewBtn.addEventListener("click", () => void this.openPreview());
    btnRow.appendChild(previewBtn);

    const scanBtn = document.createElement("button");
    scanBtn.type = "button";
    scanBtn.className = "settings-btn settings-btn-secondary";
    scanBtn.textContent = "重新扫描";
    scanBtn.title = "重新读 profile 文件，刷新安装状态";
    scanBtn.addEventListener("click", () => void this.scanCurrentPath(true));
    btnRow.appendChild(scanBtn);

    const openBtn = document.createElement("button");
    openBtn.type = "button";
    openBtn.className = "settings-btn settings-btn-secondary";
    openBtn.textContent = "打开 profile";
    openBtn.title = "用系统默认编辑器打开当前 profile 文件";
    openBtn.addEventListener("click", () => void this.openProfileInEditor());
    btnRow.appendChild(openBtn);

    this.installBtn = document.createElement("button");
    this.installBtn.type = "button";
    this.installBtn.className = "settings-btn";
    this.installBtn.textContent = "安装";
    this.installBtn.title =
      "v1.7.10+：写入前自动备份原 profile 到同目录 <profile>.ccm-backup-<时间戳>，写入失败自动回滚。" +
      "用 Win32 ReplaceFileW 保留原文件 ACL，不会出现 v1.7.9 那种用户读不了自己 profile 的事故。";
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
    statRow.className = "settings-cc-stat-row";
    const statLabel = document.createElement("span");
    statLabel.className = "settings-cc-stat-label";
    statLabel.textContent = "已注册 PowerShell session";
    statLabel.appendChild(
      makeInfoIcon(
        "正在跟 monitor 握手成功、可被 Tab ↗ 拉前的 PowerShell 进程数。\n" +
          "数字 0 ≠ 没装好：只要你那个 PS 窗口最近没跑过 cc/__ccm_bind，就不会出现在这里。",
      ),
    );
    statRow.appendChild(statLabel);
    this.regCountSpan = document.createElement("span");
    this.regCountSpan.className = "settings-cc-stat-value";
    this.regCountSpan.textContent = "—";
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
    row.className = "settings-row settings-row-checkbox";

    this.autoLaunchCheckbox = document.createElement("input");
    this.autoLaunchCheckbox.type = "checkbox";
    this.autoLaunchCheckbox.className = "settings-checkbox";
    this.autoLaunchCheckbox.addEventListener("change", () => {
      void this.toggleAutoLaunch(this.autoLaunchCheckbox.checked);
    });
    row.appendChild(this.autoLaunchCheckbox);

    const label = document.createElement("span");
    label.className = "settings-checkbox-label";
    label.textContent = "用 cc 启动 claude 时自动打开 monitor";
    row.appendChild(label);
    row.appendChild(
      makeInfoIcon(
        "勾选后：跑 cc / __ccm_bind 时如果 monitor 没在跑，PowerShell 会自动启动它（路径下方显示）。\n" +
          "不勾选：必须先手动开 monitor 再跑 cc，否则握手超时（800ms）。",
      ),
    );

    wrap.appendChild(row);

    const hint = document.createElement("div");
    hint.className = "settings-cc-autolaunch-path";
    const pathLabel = document.createElement("span");
    pathLabel.textContent = "monitor 路径: ";
    pathLabel.style.color = "var(--text-faint)";
    hint.appendChild(pathLabel);
    this.autoLaunchPathSpan = document.createElement("span");
    this.autoLaunchPathSpan.className = "settings-cc-autolaunch-path-value";
    this.autoLaunchPathSpan.textContent = "—";
    hint.appendChild(this.autoLaunchPathSpan);
    wrap.appendChild(hint);

    return wrap;
  }

  private wantsWrapper(): boolean {
    return this.wrapperCheckbox.checked;
  }

  private async openProfileInEditor(): Promise<void> {
    const p = this.pathInput.value.trim();
    if (!p) {
      showActionFailureToast("请先选 PS 版本", "或在自定义里填一个 profile 路径", { level: "info" });
      return;
    }
    try {
      await openPath(p);
    } catch (e) {
      showActionFailureToast("打开失败", `${e}\n手动路径：${p}`);
    }
  }

  /** 打开面板时调用：拿推荐路径 + 填默认值 + 扫一遍当前路径状态 */
  private async refresh(): Promise<void> {
    try {
      const status = await invoke<CcStatusResponse>("cc_integration_status", {
        commandName: CC_COMMAND_NAME,
      });
      this.recommended.Ps51 = null;
      this.recommended.Ps7 = null;
      for (const p of status.profiles) {
        this.recommended[p.kind] = p.path;
      }

      // v1.7.12: 优先恢复用户上次的选择 (localStorage)；没有就默认 PS 5.1 CurrentHost
      let savedPreset: PresetId | null = null;
      let savedPath: string | null = null;
      try {
        savedPreset = safeGet(LS_KEYS.profilePreset) as PresetId | null;
        savedPath = safeGet(LS_KEYS.profilePath);
      } catch {}
      if (savedPreset && PRESET_OPTIONS.some((o) => o.id === savedPreset)) {
        this.versionSelect.value = savedPreset;
        if (savedPreset === "Custom" && savedPath) {
          this.pathInput.value = savedPath;
        } else {
          this.pathInput.value = this.pathForPreset(savedPreset) ?? "";
        }
      } else {
        this.versionSelect.value = "Ps51-CurrentHost";
        this.pathInput.value = this.pathForPreset("Ps51-CurrentHost") ?? "";
      }

      this.regCountSpan.textContent = String(status.active_registrations);
      this.renderLegacy(status.legacy_profile_paths_with_block);
      await this.scanCurrentPath();
    } catch (e) {
      this.statusBadge.textContent = `扫描失败: ${e}`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-warn";
    }
  }

  /** 根据下拉选项 id 推算实际 profile 路径。CurrentHost 用后端给的推荐，AllHosts 用同目录 profile.ps1 */
  private pathForPreset(id: PresetId): string | null {
    switch (id) {
      case "Ps51-CurrentHost":
        return this.recommended.Ps51;
      case "Ps7-CurrentHost":
        return this.recommended.Ps7;
      case "Ps51-AllHosts":
        return this.recommended.Ps51 ? swapFileName(this.recommended.Ps51, "profile.ps1") : null;
      case "Ps7-AllHosts":
        return this.recommended.Ps7 ? swapFileName(this.recommended.Ps7, "profile.ps1") : null;
      case "Custom":
        return null; // 由用户输入
    }
  }

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
        commandName: CC_COMMAND_NAME,
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
    // 命令名固定 cc：如果 profile 已有同名 function 且用户勾了 "同时安装 wrapper"，警告会覆盖
    if (
      scan.conflicting_functions.length > 0 &&
      !scan.has_ccm_block &&
      this.wantsWrapper()
    ) {
      const warn = document.createElement("div");
      warn.className = "settings-cc-profile-warn";
      warn.textContent =
        `⚠ profile 已有 function ${scan.conflicting_functions.join(", ")}。` +
        ` 你勾了 "同时安装 cc wrapper"，安装后 PowerShell 会用 cc-monitor 块里的版本（因为它在 profile 后端）。` +
        ` 想保留你自己的：取消勾选，然后在自己的 function 开头加 __ccm_bind。`;
      this.warnArea.appendChild(warn);
    }
    this.installBtn.textContent = scan.has_ccm_block ? "重新安装" : "安装";
    this.uninstallBtn.style.display = scan.has_ccm_block ? "" : "none";
  }

  private renderLegacy(entries: LegacyEntry[]): void {
    this.legacyArea.innerHTML = "";
    if (entries.length === 0) return;
    const warn = document.createElement("div");
    warn.className = "settings-cc-legacy-warn";
    const head = document.createElement("div");
    head.style.fontWeight = "600";
    head.textContent = "ℹ 在 profile.ps1 (AllHosts) 也检测到 cc-monitor 块";
    warn.appendChild(head);
    const body = document.createElement("div");
    body.style.marginTop = "4px";
    body.textContent =
      "profile.ps1（CurrentUserAllHosts）是合法的 PowerShell profile 位置——所有 host 都会读它。" +
      "如果你**故意**装在那里（比如想让 VSCode 终端 / ISE / SSH 也用 cc），保留即可。" +
      "如果是 v1.7.0/1.7.1 残留 或 重复安装（同时也在 $PROFILE 装了一份），建议清理其中一份避免重复定义：";
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
    const id = this.versionSelect.value as PresetId;
    // 持久化用户选择，下次打开面板恢复
    try {
      safeSet(LS_KEYS.profilePreset, id);
    } catch {}
    if (id === "Custom") {
      // Custom 不强填路径，让用户自己输
      this.pathInput.focus();
      return;
    }
    const path = this.pathForPreset(id);
    if (path) {
      this.pathInput.value = path;
      try {
        safeSet(LS_KEYS.profilePath, path);
      } catch {}
      void this.scanCurrentPath();
    } else {
      this.pathInput.value = "";
      const isPs7 = id.startsWith("Ps7");
      this.statusBadge.textContent = `（${isPs7 ? "PS 7.x" : "PS 5.1"} 没自动检测到）`;
      this.statusBadge.className = "settings-cc-profile-badge settings-cc-badge-info";
    }
  }

  private async openPreview(): Promise<void> {
    try {
      const resp = await invoke<CcPreviewResponse>("cc_integration_preview", {
        commandName: CC_COMMAND_NAME,
        includeCcFunction: this.wantsWrapper(),
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
    title.textContent = this.wantsWrapper()
      ? "将写入 profile（helper + function cc）"
      : "将写入 profile（仅 helper）";
    modal.appendChild(title);

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
      showActionFailureToast("请先选 PS 版本", "或在自定义里填一个 profile 路径", { level: "info" });
      return;
    }
    const includeCc = this.wantsWrapper();
    try {
      await invoke<void>("cc_integration_install", {
        path: p,
        commandName: CC_COMMAND_NAME,
        includeCcFunction: includeCc,
      });
      await this.scanCurrentPath();
      const tail = includeCc
        ? "请重启 PowerShell，cc 命令立即可用。"
        : "下一步：[打开 profile]，在自己的 claude wrapper 开头加 __ccm_bind 再重启 PowerShell。";
      showActionFailureToast("已写入 profile", tail, { level: "info", durationMs: 8000 });
    } catch (e) {
      showActionFailureToast("安装失败", String(e));
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
      showActionFailureToast("卸载失败", String(e));
    }
  }

  private async refreshAutoLaunch(): Promise<void> {
    try {
      const cfg = await invoke<AutoLaunchConfig>("cc_get_auto_launch");
      this.autoLaunchCheckbox.checked = cfg.auto_launch_enabled;
      this.autoLaunchPathSpan.textContent =
        cfg.monitor_exe_path ?? "(未记录，启动一次 monitor 后自动记录)";
      this.autoLaunchPathSpan.title = cfg.monitor_exe_path ?? "";
    } catch (e) {
      console.warn("cc_get_auto_launch failed:", e);
    }
  }

  private async toggleAutoLaunch(enabled: boolean): Promise<void> {
    try {
      await invoke<void>("cc_set_auto_launch", { enabled });
    } catch (e) {
      showActionFailureToast("保存失败", String(e));
      this.autoLaunchCheckbox.checked = !enabled;
    }
  }
}

// makeInfoIcon + swapFileName 已拆到 ./info-icon.ts（v1.7.13）
