/**
 * 设置面板「诊断」区（v2.0.0 落地 issue #4）。
 *
 * 给用户：
 * - 看到 log 文件路径 + 当前大小
 * - 切日志级别（trace/debug/info/warn/error/off），立即生效不用重启
 * - 切错误 toast 开关
 * - 切是否写 log 文件（需要重启）
 * - 一键打开 log 文件 / log 目录
 *
 * 之所以独立 section 不混进字体/颜色：诊断是「出问题才用」的工具，跟外观无关。
 * 参照 cc_integration.ts 的 section 范式。
 */

import { invoke } from "@tauri-apps/api/core";
import { makeInfoIcon } from "./info-icon";

interface DiagnosticsConfig {
  log_enabled: boolean;
  log_level: string;
  error_toast: boolean;
  max_files: number;
}

interface LogFileEntry {
  path: string;
  size_bytes: number;
  modified_ms: number;
}

interface LogFileInfo {
  dir: string;
  current_file: string | null;
  current_size_bytes: number;
  all_files: LogFileEntry[];
}

type RestartHint = "none" | "needs_restart";

const LOG_LEVELS = ["trace", "debug", "info", "warn", "error", "off"] as const;

export interface DiagnosticsSectionOptions {
  /**
   * v2.x (issue #7)：被 CollapsibleGroup 包起来时传 `headless: true` —— 不渲染
   * 自己的「诊断」小标题（用 collapsible header 那一行即可，避免重复）。
   */
  headless?: boolean;
}

/** 给 collapsible header 复用的 i 图标说明文字。headless 模式丢失了内嵌图标 → 让外面挂一下。 */
export const DIAGNOSTICS_INFO_TEXT =
  "monitor 是 GUI 应用（windows_subsystem=windows），没有 stderr 控制台。\n" +
  "所有后端 tracing 输出写到 ~/.claude/claudecode-frontend/logs/monitor.YYYY-MM-DD.log。\n" +
  "ERROR 级别同时弹右下角红色 toast，点击 toast 直接跳到 log 文件。";

export class DiagnosticsSection {
  private root: HTMLElement;
  private current: DiagnosticsConfig = {
    log_enabled: true,
    log_level: "info",
    error_toast: true,
    max_files: 3,
  };
  private headless: boolean;

  private logEnabledCheckbox!: HTMLInputElement;
  private levelSelect!: HTMLSelectElement;
  private errorToastCheckbox!: HTMLInputElement;
  private pathSpan!: HTMLSpanElement;
  private sizeSpan!: HTMLSpanElement;
  private openFileBtn!: HTMLButtonElement;

  constructor(opts: DiagnosticsSectionOptions = {}) {
    this.headless = opts.headless ?? false;
    this.root = this.build();
    void this.refresh();
  }

  get element(): HTMLElement {
    return this.root;
  }

  private build(): HTMLElement {
    const group = document.createElement("div");
    // headless 模式：不挂 .settings-group 边距，直接作为 collapsible body 内容
    group.className = this.headless ? "settings-headless" : "settings-group";

    if (!this.headless) {
      // 标题 + 信息图标
      const heading = document.createElement("div");
      heading.className = "settings-group-title";
      heading.textContent = "诊断";
      heading.appendChild(makeInfoIcon(DIAGNOSTICS_INFO_TEXT));
      group.appendChild(heading);
    }

    // 1. 启用 log 文件 toggle
    const logRow = document.createElement("label");
    logRow.className = "settings-row settings-row-checkbox";
    this.logEnabledCheckbox = document.createElement("input");
    this.logEnabledCheckbox.type = "checkbox";
    this.logEnabledCheckbox.className = "settings-checkbox";
    this.logEnabledCheckbox.addEventListener("change", () => void this.save());
    logRow.appendChild(this.logEnabledCheckbox);
    const logLabel = document.createElement("span");
    logLabel.className = "settings-checkbox-label";
    logLabel.textContent = "启用 log 文件";
    logRow.appendChild(logLabel);
    logRow.appendChild(
      makeInfoIcon(
        "按天滚动写入 monitor.YYYY-MM-DD.log，保留最近 3 天。\n" +
          "关闭后已存在的 log 文件不会被删除，但不再写新内容。\n" +
          "⚠ 切换此项需要重启 monitor 才能生效（tracing layer 启动时定型）。",
      ),
    );
    group.appendChild(logRow);

    // 2. 日志级别 select
    const levelRow = document.createElement("div");
    levelRow.className = "settings-row";
    const levelLabel = document.createElement("span");
    levelLabel.className = "settings-label";
    levelLabel.textContent = "日志级别";
    levelRow.appendChild(levelLabel);
    this.levelSelect = document.createElement("select");
    this.levelSelect.className = "settings-input";
    for (const lv of LOG_LEVELS) {
      const opt = document.createElement("option");
      opt.value = lv;
      opt.textContent = lv;
      this.levelSelect.appendChild(opt);
    }
    this.levelSelect.addEventListener("change", () => void this.save());
    levelRow.appendChild(this.levelSelect);
    levelRow.appendChild(
      makeInfoIcon(
        "info（默认）：每个 IPC / watcher 关键步骤都记一行。\n" +
          "debug：加细节，约 10× 体积。诊断疑难时短期开启用。\n" +
          "warn / error：只记问题。\n" +
          "off：完全不记。\n" +
          "✓ 切换立即生效，无需重启。",
      ),
    );
    group.appendChild(levelRow);

    // 3. error toast toggle
    const toastRow = document.createElement("label");
    toastRow.className = "settings-row settings-row-checkbox";
    this.errorToastCheckbox = document.createElement("input");
    this.errorToastCheckbox.type = "checkbox";
    this.errorToastCheckbox.className = "settings-checkbox";
    this.errorToastCheckbox.addEventListener("change", () => void this.save());
    toastRow.appendChild(this.errorToastCheckbox);
    const toastLabel = document.createElement("span");
    toastLabel.className = "settings-checkbox-label";
    toastLabel.textContent = "后端 ERROR 时显示右下角 toast";
    toastRow.appendChild(toastLabel);
    toastRow.appendChild(
      makeInfoIcon(
        "勾选后：tracing::error! 触发右下角红色 toast，6 秒自动消失，点击直接打开 log 文件。\n" +
          "限频 60s 内最多 20 条，避免错误风暴时屏幕被刷满。\n" +
          "✓ 切换立即生效，无需重启。",
      ),
    );
    group.appendChild(toastRow);

    // 4. log 路径 + 大小 + 操作按钮
    const pathRow = document.createElement("div");
    pathRow.className = "settings-row settings-row-stack";
    const pathLabel = document.createElement("span");
    pathLabel.className = "settings-label";
    pathLabel.textContent = "log 文件";
    pathRow.appendChild(pathLabel);
    this.pathSpan = document.createElement("span");
    this.pathSpan.className = "settings-cc-autolaunch-path-value";
    this.pathSpan.style.fontFamily = "var(--font-mono, monospace)";
    this.pathSpan.style.fontSize = "11px";
    this.pathSpan.style.wordBreak = "break-all";
    this.pathSpan.textContent = "—";
    pathRow.appendChild(this.pathSpan);
    group.appendChild(pathRow);

    const sizeRow = document.createElement("div");
    sizeRow.className = "settings-row";
    const sizeLabel = document.createElement("span");
    sizeLabel.className = "settings-label";
    sizeLabel.textContent = "当前文件大小";
    sizeRow.appendChild(sizeLabel);
    this.sizeSpan = document.createElement("span");
    this.sizeSpan.className = "settings-cc-stat-value";
    this.sizeSpan.textContent = "—";
    sizeRow.appendChild(this.sizeSpan);
    group.appendChild(sizeRow);

    const btnRow = document.createElement("div");
    btnRow.className = "settings-cc-profile-buttons";
    this.openFileBtn = document.createElement("button");
    this.openFileBtn.type = "button";
    this.openFileBtn.className = "settings-btn settings-btn-secondary";
    this.openFileBtn.textContent = "打开 log 文件";
    this.openFileBtn.addEventListener("click", () => void this.openFile());
    btnRow.appendChild(this.openFileBtn);

    const openDirBtn = document.createElement("button");
    openDirBtn.type = "button";
    openDirBtn.className = "settings-btn settings-btn-secondary";
    openDirBtn.textContent = "打开 log 目录";
    openDirBtn.addEventListener("click", () => void this.openDir());
    btnRow.appendChild(openDirBtn);

    const refreshBtn = document.createElement("button");
    refreshBtn.type = "button";
    refreshBtn.className = "settings-btn settings-btn-secondary";
    refreshBtn.textContent = "刷新信息";
    refreshBtn.title = "重新读 log 目录看当前文件大小";
    refreshBtn.addEventListener("click", () => void this.refresh());
    btnRow.appendChild(refreshBtn);
    group.appendChild(btnRow);

    return group;
  }

  /** 从后端拉当前配置 + log 文件信息，刷新 UI */
  private async refresh(): Promise<void> {
    try {
      const cfg = await invoke<DiagnosticsConfig>("get_diagnostics_config");
      this.current = cfg;
      this.logEnabledCheckbox.checked = cfg.log_enabled;
      this.levelSelect.value = cfg.log_level;
      this.errorToastCheckbox.checked = cfg.error_toast;
    } catch (e) {
      console.warn("get_diagnostics_config failed:", e);
    }
    try {
      const info = await invoke<LogFileInfo>("get_log_file_info");
      if (info.current_file) {
        this.pathSpan.textContent = info.current_file;
        this.pathSpan.title = info.current_file;
        this.sizeSpan.textContent = formatBytes(info.current_size_bytes);
        this.openFileBtn.disabled = false;
      } else {
        this.pathSpan.textContent = `（log 目录: ${info.dir} —— 还没产生 log 文件）`;
        this.sizeSpan.textContent = "—";
        this.openFileBtn.disabled = true;
      }
    } catch (e) {
      console.warn("get_log_file_info failed:", e);
      this.pathSpan.textContent = `(无法读取 log 目录: ${String(e)})`;
    }
  }

  /** 收到任一控件变化 → 组装 DiagnosticsConfig → invoke set */
  private async save(): Promise<void> {
    const cfg: DiagnosticsConfig = {
      log_enabled: this.logEnabledCheckbox.checked,
      log_level: this.levelSelect.value,
      error_toast: this.errorToastCheckbox.checked,
      max_files: this.current.max_files, // UI 不暴露，保持原值
    };
    try {
      const hint = await invoke<RestartHint>("set_diagnostics_config", { cfg });
      this.current = cfg;
      if (hint === "needs_restart") {
        window.alert(
          "设置已保存。\n\n切换「启用 log 文件」需要重启 monitor 才能生效。",
        );
      }
      await this.refresh();
    } catch (e) {
      window.alert(`保存诊断配置失败：${e}`);
      // 失败 → 回退到当前实际值
      await this.refresh();
    }
  }

  private async openFile(): Promise<void> {
    try {
      await invoke("open_log_file");
    } catch (e) {
      window.alert(`打开 log 文件失败：${e}`);
    }
  }

  private async openDir(): Promise<void> {
    try {
      await invoke("open_log_dir");
    } catch (e) {
      window.alert(`打开 log 目录失败：${e}`);
    }
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(2)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
