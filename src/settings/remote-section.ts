/**
 * 设置面板「远端 (SSH)」区（SSH-remote Phase 0 / S6, issue #15）。
 *
 * 让用户配置 + 启用「远端模式」：monitor 通过 SSH 连到远端主机，由远端 daemon
 * 取代本地 jsonl-watcher 作为数据源。配置写入 config.json 的 `remote` 子对象，
 * 由 Rust 侧 `lib.rs::load_remote_config` 在启动时读取。
 *
 * **camelCase key 必须与 Rust reader 严格一致**（否则后端读不到）：
 *   enabled (bool) / host (string) / port (number, 默认 22) / user (string) /
 *   keyPath (string, 可选) / daemonPath (string) / hostKeyFingerprint (string, 可选)
 *
 * 设计（对齐 behavior.ts / diagnostics-section.ts 范式）：
 * - 读写走 config.ts 的 loadConfig / saveConfig（schema-agnostic 透传）。
 * - **MERGE 而非覆盖**：保存时先 loadConfig 拿到完整 config，只替换 `remote`
 *   子对象，其余顶层字段（theme / claudeDir / diagnostics / behavior 等）原样写回。
 * - 改动后需**重启 monitor 才生效**（数据源在 setup() 启动时定型，跟 claudeDir 一样），
 *   保存后用 banner 文字提示用户重启。
 * - Phase 0 **不**做实时「测试连接」按钮（需要一条 Rust IPC 命令，超出本步范围）。
 */

import { loadConfig, saveConfig } from "../config";
import { makeInfoIcon } from "./info-icon";

/**
 * config.json `remote` 子对象的 TS 形状。**key 必须与 Rust reader 完全一致**。
 * port 缺省由 Rust 兜底为 22；keyPath / hostKeyFingerprint 可选。
 */
export interface RemoteConfig {
  enabled: boolean;
  host: string;
  port: number;
  user: string;
  keyPath: string;
  daemonPath: string;
  hostKeyFingerprint: string;
}

/** daemonPath 建议默认值（仅作 placeholder 提示；用户填绝对路径，原样下发）。 */
const DAEMON_PATH_PLACEHOLDER = "~/.cc-monitor/bin/cc-monitor-remote";

const DEFAULTS: RemoteConfig = {
  enabled: false,
  host: "",
  port: 22,
  user: "",
  keyPath: "",
  daemonPath: "",
  hostKeyFingerprint: "",
};

const REMOTE_INFO_TEXT =
  "远端模式：monitor 通过 SSH 连到远端主机，由远端 daemon 取代本地 jsonl-watcher\n" +
  "作为数据源（渲染、Tab、分支等行为完全相同）。关闭（默认）时一切走本地，不受影响。\n\n" +
  "⚠ 启用 / 修改任意远端设置后，需重启 monitor 才生效。\n" +
  "配置不完整（缺 host / user / daemonPath）时后端会自动回退到本地模式。";

export interface RemoteSectionOptions {
  /** 被 CollapsibleGroup 包起来时传 headless: true，不渲染自己的小标题。 */
  headless?: boolean;
}

export class RemoteSection {
  private root: HTMLElement;
  private headless: boolean;

  /** 打开面板时从 config 拉到的快照，用于判断是否变化（变了就提示重启）。 */
  private original: RemoteConfig = { ...DEFAULTS };

  private enabledCheckbox!: HTMLInputElement;
  private hostInput!: HTMLInputElement;
  private portInput!: HTMLInputElement;
  private userInput!: HTMLInputElement;
  private keyPathInput!: HTMLInputElement;
  private daemonPathInput!: HTMLInputElement;
  private fingerprintInput!: HTMLInputElement;
  private banner!: HTMLElement;

  constructor(opts: RemoteSectionOptions = {}) {
    this.headless = opts.headless ?? false;
    this.root = this.build();
    void this.refresh();
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 设置面板每次 open 时调，确保展示的是 config.json 里的最新值。 */
  async refresh(): Promise<void> {
    this.original = await readRemoteConfig();
    this.syncInputs(this.original);
    this.hideBanner();
  }

  // === DOM 构建 ===

  private build(): HTMLElement {
    const group = document.createElement("div");
    group.className = this.headless ? "settings-headless" : "settings-group";

    if (!this.headless) {
      const heading = document.createElement("div");
      heading.className = "settings-group-title";
      heading.textContent = "远端 (SSH)";
      heading.appendChild(makeInfoIcon(REMOTE_INFO_TEXT));
      group.appendChild(heading);
    }

    // 保存后的重启提示 banner（默认隐藏）
    this.banner = document.createElement("div");
    this.banner.className = "settings-banner";
    group.appendChild(this.banner);

    // 1. 启用 toggle
    const enabledRow = document.createElement("label");
    enabledRow.className = "settings-row settings-row-checkbox";
    this.enabledCheckbox = document.createElement("input");
    this.enabledCheckbox.type = "checkbox";
    this.enabledCheckbox.className = "settings-checkbox";
    this.enabledCheckbox.addEventListener("change", () => void this.save());
    enabledRow.appendChild(this.enabledCheckbox);
    const enabledLabel = document.createElement("span");
    enabledLabel.className = "settings-checkbox-label";
    enabledLabel.textContent = "启用远端模式（通过 SSH 连远端主机取数据）";
    enabledRow.appendChild(enabledLabel);
    enabledRow.appendChild(
      makeInfoIcon(
        "勾选后 monitor 启动时会用 SSH 数据源取代本地 jsonl-watcher。\n" +
          "⚠ 需重启 monitor 才生效。配置不完整时后端自动回退本地模式。",
      ),
    );
    group.appendChild(enabledRow);

    // 2~7. 文本 / 数字输入
    this.hostInput = this.buildTextRow(group, "主机 (host)", "raspberrypi.local 或 192.168.1.10");
    this.portInput = this.buildNumberRow(group, "端口 (port)", 22);
    this.userInput = this.buildTextRow(group, "用户 (user)", "pi");
    this.daemonPathInput = this.buildTextRow(
      group,
      "daemon 路径 (daemonPath)",
      DAEMON_PATH_PLACEHOLDER,
    );
    this.keyPathInput = this.buildTextRow(
      group,
      "私钥路径 (keyPath，可选)",
      "C:\\Users\\me\\.ssh\\id_ed25519",
    );
    this.fingerprintInput = this.buildTextRow(
      group,
      "主机指纹 (hostKeyFingerprint，可选)",
      "SHA256:…（留空则首连 TOFU）",
    );

    // TODO(S8/S9): 加「测试连接」按钮（需一条 Rust IPC 命令，Phase 0 不做）。

    return group;
  }

  /** 一行：label（上）+ 宽文本 input（下）。change 即保存（merge）。 */
  private buildTextRow(
    parent: HTMLElement,
    labelText: string,
    placeholder: string,
  ): HTMLInputElement {
    const row = document.createElement("div");
    row.className = "settings-row settings-row-stack";
    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = labelText;
    row.appendChild(label);
    const input = document.createElement("input");
    input.type = "text";
    input.className = "settings-input settings-input-wide";
    input.placeholder = placeholder;
    // spellcheck/autocomplete 关掉：这些是路径 / 主机名，不是自然语言
    input.spellcheck = false;
    input.autocomplete = "off";
    input.addEventListener("change", () => void this.save());
    row.appendChild(input);
    parent.appendChild(row);
    return input;
  }

  /** 一行：label + 数字 input（端口）。change 即保存。 */
  private buildNumberRow(
    parent: HTMLElement,
    labelText: string,
    defaultValue: number,
  ): HTMLInputElement {
    const row = document.createElement("div");
    row.className = "settings-row";
    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = labelText;
    row.appendChild(label);
    const input = document.createElement("input");
    input.type = "number";
    input.className = "settings-input";
    input.min = "1";
    input.max = "65535";
    input.step = "1";
    input.placeholder = String(defaultValue);
    input.addEventListener("change", () => void this.save());
    row.appendChild(input);
    parent.appendChild(row);
    return input;
  }

  // === 数据同步 ===

  private syncInputs(cfg: RemoteConfig): void {
    this.enabledCheckbox.checked = cfg.enabled;
    this.hostInput.value = cfg.host;
    this.portInput.value = cfg.port ? String(cfg.port) : "";
    this.userInput.value = cfg.user;
    this.keyPathInput.value = cfg.keyPath;
    this.daemonPathInput.value = cfg.daemonPath;
    this.fingerprintInput.value = cfg.hostKeyFingerprint;
  }

  /** 从所有控件读出当前 RemoteConfig。port 解析失败 / 越界 → 兜底 22。 */
  private collect(): RemoteConfig {
    const portRaw = this.portInput.value.trim();
    let port = Number.parseInt(portRaw, 10);
    if (!Number.isFinite(port) || port < 1 || port > 65535) port = 22;
    return {
      enabled: this.enabledCheckbox.checked,
      host: this.hostInput.value.trim(),
      port,
      user: this.userInput.value.trim(),
      keyPath: this.keyPathInput.value.trim(),
      daemonPath: this.daemonPathInput.value.trim(),
      hostKeyFingerprint: this.fingerprintInput.value.trim(),
    };
  }

  /** 任一控件变化 → 组装 RemoteConfig → merge 进 config.json → 提示重启。 */
  private async save(): Promise<void> {
    const next = this.collect();

    // best-effort UI 校验：启用但缺必填字段时只警告，不阻止保存
    // （Rust 侧已会在缺字段时安全回退到本地模式）。
    if (next.enabled && (!next.host || !next.user || !next.daemonPath)) {
      this.showBanner(
        "已保存，但 host / user / daemonPath 还不完整 —— 后端会回退到本地模式。" +
          "补全后重启 monitor 才会走远端。",
      );
    }

    try {
      await writeRemoteConfig(next);
      const changed = !sameRemote(next, this.original);
      this.original = next;
      if (changed && !(next.enabled && (!next.host || !next.user || !next.daemonPath))) {
        this.showBanner("远端设置已更新 —— 需要重启 monitor 才能生效。");
      }
    } catch (e) {
      console.warn("save remote config failed:", e);
      this.showBanner(`保存失败：${String(e)}`);
    }
  }

  private showBanner(text: string): void {
    this.banner.textContent = text;
    this.banner.classList.add("settings-banner-show");
  }

  private hideBanner(): void {
    this.banner.textContent = "";
    this.banner.classList.remove("settings-banner-show");
  }
}

/**
 * 读 config.json 的 `remote` 子对象，缺失 / 类型不对的字段走默认值，永不抛。
 * 导出供面板（或将来 Rust IPC 之外的逻辑）复用。
 */
export async function readRemoteConfig(): Promise<RemoteConfig> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const r = cfg.remote;
    if (r === null || typeof r !== "object") return { ...DEFAULTS };
    const obj = r as Record<string, unknown>;
    return {
      enabled: typeof obj.enabled === "boolean" ? obj.enabled : DEFAULTS.enabled,
      host: typeof obj.host === "string" ? obj.host : DEFAULTS.host,
      port:
        typeof obj.port === "number" && Number.isFinite(obj.port)
          ? obj.port
          : DEFAULTS.port,
      user: typeof obj.user === "string" ? obj.user : DEFAULTS.user,
      keyPath: typeof obj.keyPath === "string" ? obj.keyPath : DEFAULTS.keyPath,
      daemonPath:
        typeof obj.daemonPath === "string" ? obj.daemonPath : DEFAULTS.daemonPath,
      hostKeyFingerprint:
        typeof obj.hostKeyFingerprint === "string"
          ? obj.hostKeyFingerprint
          : DEFAULTS.hostKeyFingerprint,
    };
  } catch (e) {
    console.warn("readRemoteConfig failed:", e);
    return { ...DEFAULTS };
  }
}

/**
 * 把 RemoteConfig MERGE 进 config.json 顶层的 `remote` 键，不动其他字段
 * （theme / claudeDir / diagnostics / behavior 等原样写回）。
 *
 * key 是 camelCase，与 Rust `lib.rs::load_remote_config` 读的键严格一致。
 * 可选字段为空字符串时仍写入（Rust 侧用 `.filter(|s| !s.is_empty())` 把空串当缺省处理）。
 */
export async function writeRemoteConfig(next: RemoteConfig): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg.remote = {
    enabled: next.enabled,
    host: next.host,
    port: next.port,
    user: next.user,
    keyPath: next.keyPath,
    daemonPath: next.daemonPath,
    hostKeyFingerprint: next.hostKeyFingerprint,
  };
  await saveConfig(cfg);
}

function sameRemote(a: RemoteConfig, b: RemoteConfig): boolean {
  return (
    a.enabled === b.enabled &&
    a.host === b.host &&
    a.port === b.port &&
    a.user === b.user &&
    a.keyPath === b.keyPath &&
    a.daemonPath === b.daemonPath &&
    a.hostKeyFingerprint === b.hostKeyFingerprint
  );
}
