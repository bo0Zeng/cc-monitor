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
 *
 * Tier 1（issue #15，参考 Termius / VSCode Remote-SSH）：
 * - 顶部「从 ~/.ssh/config 导入」下拉：选一个 host 别名 → `resolve_ssh_host`
 *   (`ssh -G`) 自动填 host/port/user/keyPath，免手敲。
 * - 「测试连接」按钮：`test_remote_connection` 实连一次，展示 SSH ✓/✗ + host key 指纹
 *   + daemon ✓/✗（+ hello）。指纹可一键「保存为严格校验」（TOFU→strict，known_hosts 式）。
 */

import { invoke } from "@tauri-apps/api/core";
import { loadConfig, saveConfig } from "../config";
import { makeInfoIcon } from "./info-icon";

/** `resolve_ssh_host` 的返回（Rust ResolvedHost，camelCase）。 */
interface ResolvedHost {
  host: string;
  port: number;
  user: string;
  keyPath: string | null;
}

/** `test_remote_connection` 的返回（Rust ConnTestResult，camelCase）。 */
interface ConnTestResult {
  sshOk: boolean;
  fingerprint: string | null;
  daemonOk: boolean;
  daemonHello: string | null;
  message: string;
}

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

  // Tier 1（issue #15）控件
  private importSelect!: HTMLSelectElement;
  private importHint!: HTMLElement;
  private testButton!: HTMLButtonElement;
  private testResult!: HTMLElement;

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
    this.clearTestResult();
    void this.populateAliases();
  }

  /** 从 ~/.ssh/config 拉别名清单填进导入下拉。空 → 禁用下拉 + 提示。 */
  private async populateAliases(): Promise<void> {
    let aliases: string[] = [];
    try {
      aliases = await invoke<string[]>("list_ssh_host_aliases");
    } catch (e) {
      console.warn("list_ssh_host_aliases failed:", e);
    }

    // 重置选项：首项为 placeholder。
    this.importSelect.innerHTML = "";
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "选择一个主机别名…";
    this.importSelect.appendChild(placeholder);

    if (aliases.length === 0) {
      this.importSelect.disabled = true;
      this.importHint.textContent =
        "未在 ~/.ssh/config 找到可导入的主机别名（也可继续手动填写下方字段）。";
      this.importHint.style.display = "block";
      return;
    }

    for (const a of aliases) {
      const opt = document.createElement("option");
      opt.value = a;
      opt.textContent = a;
      this.importSelect.appendChild(opt);
    }
    this.importSelect.disabled = false;
    this.importHint.style.display = "none";
    this.importSelect.value = "";
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

    // 0. 从 ~/.ssh/config 导入（Tier 1 headline）：选别名 → ssh -G 自动填字段。
    this.buildImportRow(group);

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

    // 8. 测试连接（Tier 1）：实连一次，展示 SSH/指纹/daemon 结果 + 指纹固化。
    this.buildTestSection(group);

    return group;
  }

  /** 顶部「从 ~/.ssh/config 导入」行：label + select + hint。 */
  private buildImportRow(parent: HTMLElement): void {
    const row = document.createElement("div");
    row.className = "settings-row settings-row-stack";

    const labelLine = document.createElement("span");
    labelLine.className = "settings-label";
    labelLine.textContent = "从 ~/.ssh/config 导入";
    labelLine.appendChild(
      makeInfoIcon(
        "选一个 ~/.ssh/config 里的主机别名，自动用 `ssh -G` 解析出 host/port/user/私钥\n" +
          "路径并填入下方字段（免手敲）。仍可手动微调任意字段。",
      ),
    );
    row.appendChild(labelLine);

    this.importSelect = document.createElement("select");
    this.importSelect.className = "settings-input settings-input-select settings-input-wide";
    // 初始占位项；真正的别名在 populateAliases() 里填。
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "选择一个主机别名…";
    this.importSelect.appendChild(placeholder);
    this.importSelect.disabled = true;
    this.importSelect.addEventListener("change", () => void this.onImportAlias());
    row.appendChild(this.importSelect);

    this.importHint = document.createElement("div");
    this.importHint.className = "settings-hint";
    this.importHint.style.display = "none";
    row.appendChild(this.importHint);

    parent.appendChild(row);
  }

  /** 选了别名 → resolve_ssh_host → 填 host/port/user/keyPath → 保存。 */
  private async onImportAlias(): Promise<void> {
    const alias = this.importSelect.value;
    if (!alias) return;
    try {
      const resolved = await invoke<ResolvedHost>("resolve_ssh_host", { alias });
      this.hostInput.value = resolved.host;
      this.portInput.value = resolved.port ? String(resolved.port) : "22";
      this.userInput.value = resolved.user;
      // keyPath 只在 ssh -G 给出且文件存在时覆盖；null 时清空（留给 agent / 手填）。
      this.keyPathInput.value = resolved.keyPath ?? "";
      // daemonPath / enabled 保持原样（导入只动连接四要素）。
      await this.save();
      this.showBanner(`已从别名「${alias}」导入连接参数。`);
    } catch (e) {
      console.warn("resolve_ssh_host failed:", e);
      this.showBanner(`导入别名「${alias}」失败：${String(e)}`);
    } finally {
      // 复位下拉到 placeholder，便于重复导入同一别名时再次触发 change。
      this.importSelect.value = "";
    }
  }

  /** 「测试连接」按钮 + 结果容器。 */
  private buildTestSection(parent: HTMLElement): void {
    const row = document.createElement("div");
    row.className = "settings-row settings-row-end";

    this.testButton = document.createElement("button");
    this.testButton.type = "button";
    this.testButton.className = "settings-btn settings-btn-primary";
    this.testButton.textContent = "测试连接";
    this.testButton.addEventListener("click", () => void this.onTestConnection());
    row.appendChild(this.testButton);
    parent.appendChild(row);

    this.testResult = document.createElement("div");
    this.testResult.className = "remote-test-result";
    this.testResult.style.display = "none";
    parent.appendChild(this.testResult);
  }

  /** 点「测试连接」：组当前表单 → test_remote_connection → 渲染结果。 */
  private async onTestConnection(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user || !cfg.daemonPath) {
      this.renderTestResult(null, "请先填好 host / user / daemonPath 再测试。");
      return;
    }
    this.testButton.disabled = true;
    const prevLabel = this.testButton.textContent;
    this.testButton.textContent = "测试中…";
    try {
      const res = await invoke<ConnTestResult>("test_remote_connection", { cfg });
      this.renderTestResult(res, null);
    } catch (e) {
      console.warn("test_remote_connection failed:", e);
      this.renderTestResult(null, `测试失败：${String(e)}`);
    } finally {
      this.testButton.disabled = false;
      this.testButton.textContent = prevLabel;
    }
  }

  /** 渲染测试结果：SSH ✓/✗、指纹（+可固化）、daemon ✓/✗（+hello）。 */
  private renderTestResult(res: ConnTestResult | null, hardError: string | null): void {
    this.testResult.innerHTML = "";
    this.testResult.style.display = "block";

    if (hardError !== null) {
      const line = document.createElement("div");
      line.className = "remote-test-line remote-test-err";
      line.textContent = hardError;
      this.testResult.appendChild(line);
      return;
    }
    if (res === null) return;

    // SSH 行
    this.testResult.appendChild(
      makeStatusLine(res.sshOk, res.sshOk ? "SSH 连接成功" : "SSH 连接失败"),
    );

    // 指纹行 + 固化按钮
    if (res.fingerprint) {
      const fpLine = document.createElement("div");
      fpLine.className = "remote-test-line";
      const fpText = document.createElement("span");
      fpText.className = "remote-test-fp";
      fpText.textContent = `主机指纹：${res.fingerprint}`;
      fpLine.appendChild(fpText);

      const current = this.fingerprintInput.value.trim();
      if (current !== res.fingerprint) {
        const saveBtn = document.createElement("button");
        saveBtn.type = "button";
        saveBtn.className = "settings-btn";
        saveBtn.textContent = current ? "更新为该指纹（严格校验）" : "保存为严格校验";
        const fp = res.fingerprint;
        saveBtn.addEventListener("click", () => void this.onSaveFingerprint(fp));
        fpLine.appendChild(saveBtn);
      } else {
        const ok = document.createElement("span");
        ok.className = "remote-test-ok";
        ok.textContent = "（已固化为严格校验）";
        fpLine.appendChild(ok);
      }
      this.testResult.appendChild(fpLine);
    }

    // daemon 行
    const daemonText = res.daemonOk
      ? `daemon 响应正常${res.daemonHello ? `（${res.daemonHello}）` : ""}`
      : "daemon 未响应 / 未部署";
    this.testResult.appendChild(makeStatusLine(res.daemonOk, daemonText));

    // 总体 message
    if (res.message) {
      const msg = document.createElement("div");
      msg.className = "remote-test-line remote-test-msg";
      msg.textContent = res.message;
      this.testResult.appendChild(msg);
    }
  }

  /** 把测出的指纹写进 hostKeyFingerprint 字段并保存（TOFU→strict 固化）。 */
  private async onSaveFingerprint(fingerprint: string): Promise<void> {
    this.fingerprintInput.value = fingerprint;
    await this.save();
    this.showBanner("主机指纹已保存为严格校验 —— 重启 monitor 后生效。");
    // 重渲染结果，把固化按钮换成「已固化」标记。
    void this.onTestConnectionRerender(fingerprint);
  }

  /** 固化指纹后就地把结果区里的固化按钮换成「已固化」（不重连）。 */
  private onTestConnectionRerender(fingerprint: string): void {
    const fpLine = this.testResult.querySelector(".remote-test-fp");
    if (!fpLine || !fpLine.parentElement) return;
    const parent = fpLine.parentElement;
    const btn = parent.querySelector("button");
    if (btn) btn.remove();
    const ok = document.createElement("span");
    ok.className = "remote-test-ok";
    ok.textContent = "（已固化为严格校验）";
    parent.appendChild(ok);
    void fingerprint;
  }

  private clearTestResult(): void {
    if (this.testResult) {
      this.testResult.innerHTML = "";
      this.testResult.style.display = "none";
    }
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

/** 一行带 ✓/✗ 状态标的测试结果行。 */
function makeStatusLine(ok: boolean, text: string): HTMLElement {
  const line = document.createElement("div");
  line.className = `remote-test-line ${ok ? "remote-test-ok" : "remote-test-err"}`;
  const mark = document.createElement("span");
  mark.className = "remote-test-mark";
  mark.textContent = ok ? "✓" : "✗";
  line.appendChild(mark);
  const label = document.createElement("span");
  label.textContent = text;
  line.appendChild(label);
  return line;
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
