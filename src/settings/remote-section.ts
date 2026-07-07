/**
 * 设置面板「远端 (SSH)」区（SSH-remote issue #15 / 多机 #30）。
 *
 * 让用户配置 + 启用「远端模式」：monitor 通过 SSH 连到 **0..N 台** 远端主机，由各台的
 * daemon 作为额外数据源（与本地 jsonl-watcher 聚合）。配置写入 config.json 的 `remote`
 * 子对象（`{ enabled, hosts: [...] }`），由 Rust 侧 `lib.rs::load_remote_configs` 启动时读。
 *
 * **camelCase key 必须与 Rust reader 严格一致**（否则后端读不到）：
 *   enabled (bool) / hosts[] 内每台：label (string, 可选默认 host) / host / port (默认 22) /
 *   user / keyPath (可选) / daemonPath / hostKeyFingerprint (可选)
 *
 * **向后兼容**：旧的单对象 `remote: { enabled, host, ... }`（无 `hosts` 键）读取时归一成
 * 1 台（label 默认 = host）；保存时升级写成 `hosts` 数组。
 *
 * 设计（对齐 behavior.ts / diagnostics-section.ts 范式）：
 * - 读写走 config.ts 的 loadConfig / saveConfig（schema-agnostic 透传）。
 * - **MERGE 而非覆盖**：保存时先 loadConfig 拿到完整 config，只替换 `remote` 子对象。
 * - 改动后需**重启 monitor 才生效**（数据源在 setup() 启动时定型），保存后 banner 提示。
 * - 每次输入 change 立即保存（无"未保存"中间态）→ refresh() 可安全从 config 重建卡片。
 *
 * Tier 1（issue #15）：从 ~/.ssh/config 导入别名（`ssh -G`）→ 作为**新机器**加入列表；
 * 每台各有「测试连接」（`test_remote_connection`）展示 SSH/指纹/daemon，指纹可一键固化。
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
 * 单台远端机器配置（config.json `remote.hosts[]` 的元素）。**key 必须与 Rust reader 一致**。
 * `label` 是该台的稳定身份（origin tag，多机 #30）：Tab 前缀 / 历史分组 / 选台 key。
 * 留空时后端回退用 host。port 缺省 22；keyPath / hostKeyFingerprint 可选。
 */
export interface RemoteHostConfig {
  label: string;
  host: string;
  port: number;
  user: string;
  keyPath: string;
  daemonPath: string;
  hostKeyFingerprint: string;
}

/** config.json `remote` 段：全局 enabled + 机器列表。 */
export interface RemoteConfig {
  enabled: boolean;
  hosts: RemoteHostConfig[];
}

/**
 * daemonPath placeholder：必须是**绝对路径**。SSH exec 不经 shell，`~` 不会被展开，
 * 故用 `/home/<user>/...` 形式而非 `~/...`（避免误导用户以为 `~` 可用）。
 */
const DAEMON_PATH_PLACEHOLDER = "/home/<user>/.cc-monitor/bin/cc-monitor-remote";

/**
 * 按远端用户名生成 daemonPath 默认值（与自动部署的约定路径一致，
 * 见 doc/REMOTE-PHASE0-DEPLOY.md）。root 的 home 不在 /home 下，特判。
 * 只是预填——远端 home 不标准（如 macOS /Users）时用户可改，「测试连接」会暴露问题。
 */
function defaultDaemonPathFor(user: string): string {
  const home = user === "root" ? "/root" : `/home/${user}`;
  return `${home}/.cc-monitor/bin/cc-monitor-remote`;
}

const HOST_DEFAULTS: RemoteHostConfig = {
  label: "",
  host: "",
  port: 22,
  user: "",
  keyPath: "",
  daemonPath: "",
  hostKeyFingerprint: "",
};

const REMOTE_INFO_TEXT =
  "远端模式：monitor 通过 SSH 连到一台或多台远端主机，由各台 daemon 作为额外数据源\n" +
  "与本地聚合（渲染、Tab、分支等行为完全相同；远端 Tab 标题带 [机器名] 前缀）。\n" +
  "关闭（默认）或机器列表为空时一切走本地，不受影响。\n\n" +
  "⚠ 启用 / 修改任意远端设置后，需重启 monitor 才生效。\n" +
  "某台配置不完整（缺 host / user / daemonPath）时后端会跳过该台。";

/**
 * Feature ②：远端 ↗ 拉前的 bashrc 块——**注册原语与启动器分离**（镜像本地
 * `__ccm_bind` + 可选 `cc` wrapper 的设计；用户设计评审指正：注册不该耦合启动）：
 *
 * - `__ccm_rbind`（注册原语）：只做注册——tmux 内对当前 session 开标题直通 +
 *   给"即将 `exec claude` 的当前 (子)shell PID"挂 marker watcher（每秒读
 *   `sessions/<PID>.json` 的 sid，变了就刷窗口标题 `ccm-rbind-<sid>`）。
 *   不设环境、不启动任何东西。**契约**：须与 `exec claude` 同一 (子)shell——
 *   `( __ccm_rbind; exec claude ... )`，exec 后 shell PID 即 claude PID。
 * - `ccm`（可选便捷启动器）：一行薄壳，且**不覆盖用户已有的同名函数**（旧版
 *   曾无条件覆盖，实测清掉过用户自己带代理的 ccm 启动器——防撞守卫由此而来）。
 *   自有启动器（cc/cct 等）的用户不用 ccm，在自己的函数里调原语即可。
 *
 * `ccm-rbind-%s` 标记必须与后端 `bind.rs` 的 `format!("ccm-rbind-{sid}")` 完全一致。
 *
 * tmux 自适配（Batch7 真机排查实证）：tmux 默认 `set-titles off`——OSC 标题转义
 * 只落到 pane title、到不了外层 ssh 终端窗口标题，marker 被截住导致绑定必然
 * 失败，而 tmux 恰是远端最常见形态。原语内自动对**当前 session** 开直通
 * （session 级选项，不写 tmux.conf、不影响其它 session）。
 */
// 单一来源：shared/ccm-wrapper.sh（后端 sftp.rs include_str! 同一文件，杜绝漂移）
import CCM_WRAPPER_SNIPPET from "../../shared/ccm-wrapper.sh?raw";

export interface RemoteSectionOptions {
  /** 被 CollapsibleGroup 包起来时传 headless: true，不渲染自己的小标题。 */
  headless?: boolean;
}

// === 共享 DOM 小工具 ===

/** 一行：label（上）+ 宽文本 input（下）。change 触发 onChange。 */
function buildTextRow(
  parent: HTMLElement,
  labelText: string,
  placeholder: string,
  onChange: () => void,
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
  input.addEventListener("change", onChange);
  row.appendChild(input);
  parent.appendChild(row);
  return input;
}

/** 一行：label + 数字 input（端口）。change 触发 onChange。 */
function buildNumberRow(
  parent: HTMLElement,
  labelText: string,
  defaultValue: number,
  onChange: () => void,
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
  input.addEventListener("change", onChange);
  row.appendChild(input);
  parent.appendChild(row);
  return input;
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

/** 解析端口字符串：失败 / 越界 → 兜底 22。 */
function parsePort(raw: string): number {
  let port = Number.parseInt(raw.trim(), 10);
  if (!Number.isFinite(port) || port < 1 || port > 65535) port = 22;
  return port;
}

// === 单台机器卡片 ===

interface MachineCardHooks {
  /** 任一字段变化 → 让 section 保存全部。 */
  onChange: () => void;
  /** 点删除 → 让 section 移除本卡片。 */
  onRemove: (card: MachineCard) => void;
}

/**
 * 一台远端机器的 UI 卡片：自己的字段输入 + 测试连接（含 TOFU 指纹固化）+ 删除。
 * collect() 读出 RemoteHostConfig。所有字段 change 都通过 hooks.onChange 触发 section 保存。
 */
class MachineCard {
  readonly element: HTMLElement;
  private legend!: HTMLElement;
  private labelInput!: HTMLInputElement;
  private hostInput!: HTMLInputElement;
  private portInput!: HTMLInputElement;
  private userInput!: HTMLInputElement;
  private keyPathInput!: HTMLInputElement;
  private daemonPathInput!: HTMLInputElement;
  private fingerprintInput!: HTMLInputElement;
  private testButton!: HTMLButtonElement;
  private installButton!: HTMLButtonElement;
  private daemonInstallButton!: HTMLButtonElement;
  private daemonUninstallButton!: HTMLButtonElement;
  private ccmUninstallButton!: HTMLButtonElement;
  private testResult!: HTMLElement;
  /** 折叠时隐藏的字段 + 测试/安装区（legend 始终可见）。 */
  private body!: HTMLElement;
  /** legend 里承载机器名的 span（label || host）。 */
  private nameSpan!: HTMLElement;
  /** legend 左侧折叠指示符（▸ 折叠 / ▾ 展开）。 */
  private toggleIndicator!: HTMLElement;
  private collapsed = false;

  constructor(
    initial: RemoteHostConfig,
    private hooks: MachineCardHooks,
    collapsed = false,
  ) {
    this.element = this.build();
    this.syncInputs(initial);
    this.updateLegend();
    this.setCollapsed(collapsed);
  }

  /** 读出本卡片的 RemoteHostConfig（trim；port 兜底 22）。 */
  collect(): RemoteHostConfig {
    return {
      label: this.labelInput.value.trim(),
      host: this.hostInput.value.trim(),
      port: parsePort(this.portInput.value),
      user: this.userInput.value.trim(),
      keyPath: this.keyPathInput.value.trim(),
      daemonPath: this.daemonPathInput.value.trim(),
      hostKeyFingerprint: this.fingerprintInput.value.trim(),
    };
  }

  /** 导入别名时填充连接参数（host/port/user/keyPath + daemon 兜底 + label=别名）。 */
  applyResolved(resolved: ResolvedHost, alias: string): void {
    if (!this.labelInput.value.trim()) this.labelInput.value = alias;
    this.hostInput.value = resolved.host;
    this.portInput.value = resolved.port ? String(resolved.port) : "22";
    this.userInput.value = resolved.user;
    this.keyPathInput.value = resolved.keyPath ?? "";
    if (!this.daemonPathInput.value.trim() && resolved.user) {
      this.daemonPathInput.value = defaultDaemonPathFor(resolved.user);
    }
    this.updateLegend();
  }

  private build(): HTMLElement {
    const card = document.createElement("fieldset");
    card.className = "remote-machine";

    const legend = document.createElement("legend");
    legend.className = "remote-machine-legend";
    this.legend = legend;
    card.appendChild(legend);

    // 折叠指示符（▸ 折叠 / ▾ 展开）。点 legend（非删除按钮）切换折叠。
    this.toggleIndicator = document.createElement("span");
    this.toggleIndicator.className = "remote-machine-toggle";
    this.toggleIndicator.textContent = "▾";
    legend.appendChild(this.toggleIndicator);

    // 机器名（label || host）—— 独立 span（不靠脆弱的 firstChild 文本节点）。flex:1 把删除推到右侧。
    this.nameSpan = document.createElement("span");
    this.nameSpan.className = "remote-machine-name";
    legend.appendChild(this.nameSpan);

    // 删除按钮（legend 右侧）
    const removeBtn = document.createElement("button");
    removeBtn.type = "button";
    removeBtn.className = "settings-btn settings-btn-secondary remote-machine-remove";
    removeBtn.textContent = "删除";
    removeBtn.title = "从列表移除这台机器";
    removeBtn.addEventListener("click", (ev) => {
      ev.stopPropagation(); // 别让删除点击冒泡到 legend 触发折叠
      this.hooks.onRemove(this);
    });
    legend.appendChild(removeBtn);

    // 点 legend 折叠/展开（删除按钮已 stopPropagation；再防御性排除一次）。
    legend.addEventListener("click", (ev) => {
      if ((ev.target as HTMLElement).closest(".remote-machine-remove")) return;
      this.setCollapsed(!this.collapsed);
    });

    // body：折叠时整体隐藏（legend 始终在）。所有字段 + 测试/安装区都挂这里。
    this.body = document.createElement("div");
    this.body.className = "remote-machine-body";
    card.appendChild(this.body);
    const body = this.body;

    const onChange = () => {
      this.updateLegend();
      this.hooks.onChange();
    };

    this.labelInput = buildTextRow(body, "名称 (label，可选)", "pi / nano（留空用主机名）", onChange);
    this.hostInput = buildTextRow(body, "主机 (host)", "raspberrypi.local 或 192.168.1.10", onChange);
    this.portInput = buildNumberRow(body, "端口 (port)", 22, onChange);
    this.userInput = buildTextRow(body, "用户 (user)", "pi", onChange);
    this.daemonPathInput = buildTextRow(body, "daemon 路径 (daemonPath)", DAEMON_PATH_PLACEHOLDER, onChange);
    const daemonHint = document.createElement("div");
    daemonHint.className = "settings-hint";
    daemonHint.textContent =
      "须为绝对路径（如 /home/pi/.cc-monitor/bin/cc-monitor-remote）；SSH 直接 exec 不经 shell，`~` 不会被展开。";
    body.appendChild(daemonHint);
    // F13：手动填完 user（change = 失焦提交，避免逐键拿半截用户名）后，daemonPath
    // 为空则按约定路径预填——与 ssh config 导入（applyResolved）同一兜底；已有值不覆盖。
    this.userInput.addEventListener("change", () => {
      const user = this.userInput.value.trim();
      if (user && !this.daemonPathInput.value.trim()) {
        this.daemonPathInput.value = defaultDaemonPathFor(user);
        onChange();
      }
    });
    this.keyPathInput = buildTextRow(body, "私钥路径 (keyPath，可选)", "C:\\Users\\me\\.ssh\\id_ed25519", onChange);
    this.fingerprintInput = buildTextRow(
      body,
      "主机指纹 (hostKeyFingerprint，可选)",
      "SHA256:…（留空则首连 TOFU）",
      onChange,
    );

    // 安装位置提示：明确告诉用户「在哪里装什么」。
    const installInfo = document.createElement("div");
    installInfo.className = "settings-hint remote-install-info";
    installInfo.textContent =
      "安装位置：① daemon（远端数据源，必需）→ 上方「daemon 路径」填的位置" +
      "（默认 ~/.cc-monitor/bin/cc-monitor-remote）+ 同目录 .build_id；启用远端后连接时会自动安装，" +
      "下面按钮供手动装 / 卸。② ccm 助手（↗ 拉前用，可选）→ 远端 ~/.bashrc 里一段带 " +
      "cc-monitor BEGIN/END 标记的函数（先备份原文件、只动标记块内）。" +
      "装好后：无自有启动器 → 直接用 ccm 启动 claude；有自有启动器（cc/cct 等）→ " +
      "在函数里调注册原语 __ccm_rbind（与 exec claude 同一子 shell：( __ccm_rbind; exec claude ... )）。" +
      "ccm 不会覆盖你已有的同名函数；tmux 里开箱即用（自动对当前 session 开标题直通，不改 tmux.conf）。";
    body.appendChild(installInfo);

    // 动作区：连接测试 + daemon 装/卸 + ccm 装/卸。按钮多，行内可换行。
    const mkBtn = (
      label: string,
      variant: string,
      title: string,
      onClick: () => void,
    ): HTMLButtonElement => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = `settings-btn ${variant}`;
      b.textContent = label;
      b.title = title;
      b.addEventListener("click", onClick);
      return b;
    };

    const actionRow = document.createElement("div");
    actionRow.className = "settings-row settings-row-actions";

    this.testButton = mkBtn(
      "测试连接",
      "settings-btn-primary",
      "测试 SSH 连接 / 主机指纹 / daemon 是否在线",
      () => void this.onTestConnection(),
    );
    actionRow.appendChild(this.testButton);

    // F08c：手动安装 / 卸载 daemon（两个独立按钮）。
    this.daemonInstallButton = mkBtn(
      "安装 daemon",
      "settings-btn-secondary",
      "把内嵌的 daemon 二进制按远端架构装到 daemonPath（已是最新则跳过）",
      () => void this.onDeployDaemon(),
    );
    actionRow.appendChild(this.daemonInstallButton);

    this.daemonUninstallButton = mkBtn(
      "卸载 daemon",
      "settings-btn-secondary",
      "删除远端 daemon 二进制 + .build_id（若机器仍启用，下次连接会自动装回）",
      () => void this.onUninstallDaemon(),
    );
    actionRow.appendChild(this.daemonUninstallButton);

    // F10：装 / 卸 ccm 助手到 ~/.bashrc（↗ 拉前用）。
    this.installButton = mkBtn(
      "装 ccm 助手",
      "settings-btn-secondary",
      "把 ccm wrapper 装进远端 ~/.bashrc（↗ 拉前用）；先备份原文件、幂等可重装",
      () => void this.onInstallCcm(),
    );
    actionRow.appendChild(this.installButton);

    this.ccmUninstallButton = mkBtn(
      "卸载 ccm",
      "settings-btn-secondary",
      "从远端 ~/.bashrc 删掉 cc-monitor 的 ccm 块（先备份；块外内容不动）",
      () => void this.onUninstallCcm(),
    );
    actionRow.appendChild(this.ccmUninstallButton);

    body.appendChild(actionRow);

    this.testResult = document.createElement("div");
    this.testResult.className = "remote-test-result";
    this.testResult.style.display = "none";
    body.appendChild(this.testResult);

    return card;
  }

  /** 折叠/展开本卡片：折叠时只剩 legend（机器名行），隐藏全部字段 + 测试/安装。 */
  private setCollapsed(next: boolean): void {
    this.collapsed = next;
    this.element.classList.toggle("is-collapsed", next);
    this.toggleIndicator.textContent = next ? "▸" : "▾";
    this.legend.setAttribute("aria-expanded", next ? "false" : "true");
  }

  private syncInputs(cfg: RemoteHostConfig): void {
    this.labelInput.value = cfg.label;
    this.hostInput.value = cfg.host;
    this.portInput.value = cfg.port ? String(cfg.port) : "";
    this.userInput.value = cfg.user;
    this.keyPathInput.value = cfg.keyPath;
    this.daemonPathInput.value = cfg.daemonPath;
    this.fingerprintInput.value = cfg.hostKeyFingerprint;
  }

  /** legend 显示 label || host || 占位。 */
  private updateLegend(): void {
    this.nameSpan.textContent =
      this.labelInput.value.trim() || this.hostInput.value.trim() || "（未命名机器）";
  }

  /** 点「测试连接」：组本卡片 → test_remote_connection → 渲染结果。 */
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

  /** F10：点「装 ccm 助手」——把 CCM_WRAPPER_SNIPPET 经 SFTP 装进这台远端的 ~/.bashrc。 */
  private async onInstallCcm(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.testResult.style.display = "block";
      this.testResult.textContent = "请先填好 host / user 再安装 ccm 助手。";
      return;
    }
    this.installButton.disabled = true;
    const prev = this.installButton.textContent;
    this.installButton.textContent = "安装中…";
    this.testResult.style.display = "block";
    this.testResult.textContent = "安装 ccm 助手中…";
    try {
      // snippet 由后端拥有（写进 ~/.bashrc 的是被 shell 执行的代码，不让前端注入）；
      // 前端 CCM_WRAPPER_SNIPPET 仅用于面板展示/手动复制，须与后端常量逐字一致。
      const msg = await invoke<string>("install_remote_ccm_helper", {
        cfg,
        profile: ".bashrc",
      });
      this.testResult.textContent = `✓ ${msg}`;
    } catch (e) {
      this.testResult.textContent = `✗ 安装失败：${String(e)}`;
    } finally {
      this.installButton.disabled = false;
      this.installButton.textContent = prev;
    }
  }

  /** 在结果区显示一行提示（缺字段 / 取消等）。 */
  private showResultText(text: string): void {
    this.testResult.style.display = "block";
    this.testResult.textContent = text;
  }

  /** 通用远端动作：禁用按钮 + 显示进度 → invoke → 结果写结果区 → 恢复按钮。 */
  private async runRemoteAction(
    btn: HTMLButtonElement,
    busyLabel: string,
    fn: () => Promise<string>,
  ): Promise<void> {
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = `${busyLabel}…`;
    this.testResult.style.display = "block";
    this.testResult.textContent = `${busyLabel}…`;
    try {
      const msg = await fn();
      this.testResult.textContent = `✓ ${msg}`;
    } catch (e) {
      this.testResult.textContent = `✗ ${String(e)}`;
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
    }
  }

  /** F08c：点「安装 daemon」——把内嵌 daemon 按远端架构装到 daemonPath。 */
  private async onDeployDaemon(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user || !cfg.daemonPath) {
      this.showResultText("请先填好 host / user / daemonPath 再安装 daemon。");
      return;
    }
    await this.runRemoteAction(this.daemonInstallButton, "安装 daemon 中", () =>
      invoke<string>("deploy_remote_daemon", { cfg }),
    );
  }

  /** F08c：点「卸载 daemon」——删远端 daemon 二进制 + .build_id（二次确认）。 */
  private async onUninstallDaemon(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user || !cfg.daemonPath) {
      this.showResultText("请先填好 host / user / daemonPath 再卸载 daemon。");
      return;
    }
    if (
      !window.confirm(
        `确认从 ${cfg.host} 删除 daemon？\n会删：${cfg.daemonPath} 及同目录 .build_id。\n（若该机器仍勾选启用，下次连接会自动装回。）`,
      )
    ) {
      return;
    }
    await this.runRemoteAction(this.daemonUninstallButton, "卸载 daemon 中", () =>
      invoke<string>("uninstall_remote_daemon", { cfg }),
    );
  }

  /** F10：点「卸载 ccm」——从远端 ~/.bashrc 删掉 ccm 块（二次确认）。 */
  private async onUninstallCcm(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.showResultText("请先填好 host / user 再卸载 ccm。");
      return;
    }
    if (
      !window.confirm(
        `确认从 ${cfg.host} 的 ~/.bashrc 删除 ccm 块？\n（只删 cc-monitor BEGIN/END 标记块，块外内容不动，会先备份原文件。）`,
      )
    ) {
      return;
    }
    await this.runRemoteAction(this.ccmUninstallButton, "卸载 ccm 中", () =>
      invoke<string>("uninstall_remote_ccm_helper", { cfg, profile: ".bashrc" }),
    );
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

    this.testResult.appendChild(makeStatusLine(res.sshOk, res.sshOk ? "SSH 连接成功" : "SSH 连接失败"));

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

        // FIX 4：首次 / TOFU 捕获（之前没配过指纹）时该指纹**未经验证**，首连本身可能已被
        // 中间人篡改。显眼提示用户先在远端用 ssh-keyscan 核对。
        if (!current) {
          const caution = document.createElement("div");
          caution.className = "remote-test-line remote-test-caution";
          caution.textContent =
            "⚠ 此指纹未经验证 —— 首次连接可能被中间人篡改。建议在远端用 `ssh-keyscan` 核对（见部署文档 step 6）后再固化。";
          fpLine.appendChild(caution);
        }
      } else {
        const ok = document.createElement("span");
        ok.className = "remote-test-ok";
        ok.textContent = "（已固化为严格校验）";
        fpLine.appendChild(ok);
      }
      this.testResult.appendChild(fpLine);
    }

    const daemonText = res.daemonOk
      ? `daemon 响应正常${res.daemonHello ? `（${res.daemonHello}）` : ""}`
      : "daemon 未响应 / 未部署";
    this.testResult.appendChild(makeStatusLine(res.daemonOk, daemonText));

    if (res.message) {
      const msg = document.createElement("div");
      msg.className = "remote-test-line remote-test-msg";
      msg.textContent = res.message;
      this.testResult.appendChild(msg);
    }
  }

  /** 把测出的指纹写进字段并保存（TOFU→strict 固化）。 */
  private async onSaveFingerprint(fingerprint: string): Promise<void> {
    this.fingerprintInput.value = fingerprint;
    this.hooks.onChange(); // 触发 section 保存
    // 就地把固化按钮换成「已固化」（不重连）。
    const fpLine = this.testResult.querySelector(".remote-test-fp");
    if (fpLine && fpLine.parentElement) {
      const btn = fpLine.parentElement.querySelector("button");
      if (btn) btn.remove();
      const ok = document.createElement("span");
      ok.className = "remote-test-ok";
      ok.textContent = "（已固化为严格校验）";
      fpLine.parentElement.appendChild(ok);
    }
  }
}

// === 远端区（机器列表容器）===

export class RemoteSection {
  private root: HTMLElement;
  private headless: boolean;

  /** 打开面板时从 config 拉到的快照，用于判断是否变化（变了就提示重启）。 */
  private original: RemoteConfig = { enabled: false, hosts: [] };

  private enabledCheckbox!: HTMLInputElement;
  private machinesContainer!: HTMLElement;
  private emptyHint!: HTMLElement;
  private banner!: HTMLElement;
  private importSelect!: HTMLSelectElement;
  private importHint!: HTMLElement;

  private cards: MachineCard[] = [];

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
    this.enabledCheckbox.checked = this.original.enabled;
    this.rebuildCards(this.original.hosts);
    this.hideBanner();
    void this.populateAliases();
  }

  /** 用 config 里的机器列表重建卡片。 */
  private rebuildCards(hosts: RemoteHostConfig[]): void {
    this.cards = [];
    this.machinesContainer.innerHTML = "";
    for (const h of hosts) {
      // 从 config 重建的卡片默认折叠（只显示机器名）——多机时列表整洁；点名称展开编辑。
      this.appendCard(h, true);
    }
    this.updateEmptyHint();
  }

  private appendCard(initial: RemoteHostConfig, collapsed = false): MachineCard {
    const card = new MachineCard(
      initial,
      {
        onChange: () => void this.save(),
        onRemove: (c) => this.removeCard(c),
      },
      collapsed,
    );
    this.cards.push(card);
    this.machinesContainer.appendChild(card.element);
    this.updateEmptyHint();
    return card;
  }

  private removeCard(card: MachineCard): void {
    const idx = this.cards.indexOf(card);
    if (idx < 0) return;
    this.cards.splice(idx, 1);
    card.element.remove();
    this.updateEmptyHint();
    void this.save();
  }

  private updateEmptyHint(): void {
    this.emptyHint.style.display = this.cards.length === 0 ? "block" : "none";
  }

  /** 从 ~/.ssh/config 拉别名清单填进导入下拉。空 → 禁用下拉 + 提示。 */
  private async populateAliases(): Promise<void> {
    let aliases: string[] = [];
    try {
      aliases = await invoke<string[]>("list_ssh_host_aliases");
    } catch (e) {
      console.warn("list_ssh_host_aliases failed:", e);
    }

    this.importSelect.innerHTML = "";
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "选择一个主机别名…（导入为新机器）";
    this.importSelect.appendChild(placeholder);

    if (aliases.length === 0) {
      this.importSelect.disabled = true;
      this.importHint.textContent =
        "未在 ~/.ssh/config 找到可导入的主机别名（也可点「添加机器」手动填写）。";
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

    this.banner = document.createElement("div");
    this.banner.className = "settings-banner";
    group.appendChild(this.banner);

    // 0. 从 ~/.ssh/config 导入（选别名 → 加为新机器）
    this.buildImportRow(group);

    // 1. 启用 toggle（全局）
    const enabledRow = document.createElement("label");
    enabledRow.className = "settings-row settings-row-checkbox";
    this.enabledCheckbox = document.createElement("input");
    this.enabledCheckbox.type = "checkbox";
    this.enabledCheckbox.className = "settings-checkbox";
    this.enabledCheckbox.addEventListener("change", () => void this.save());
    enabledRow.appendChild(this.enabledCheckbox);
    const enabledLabel = document.createElement("span");
    enabledLabel.className = "settings-checkbox-label";
    enabledLabel.textContent = "启用远端模式（通过 SSH 连下列机器取数据）";
    enabledRow.appendChild(enabledLabel);
    enabledRow.appendChild(
      makeInfoIcon(
        "勾选后 monitor 启动时会**额外**用 SSH 连下列每台机器作为数据源（与本地聚合）。\n" +
          "⚠ 需重启 monitor 才生效。某台配置不完整时后端跳过该台。列表为空 = 等于关闭。",
      ),
    );
    group.appendChild(enabledRow);

    // 2. 机器列表容器
    this.machinesContainer = document.createElement("div");
    this.machinesContainer.className = "remote-machines";
    group.appendChild(this.machinesContainer);

    // 空列表提示
    this.emptyHint = document.createElement("div");
    this.emptyHint.className = "settings-hint";
    this.emptyHint.textContent = "尚未添加远端机器。点下方「添加机器」，或从上方下拉导入别名。";
    this.emptyHint.style.display = "none";
    group.appendChild(this.emptyHint);

    // 3. 添加机器按钮
    const addRow = document.createElement("div");
    addRow.className = "settings-row settings-row-end";
    const addBtn = document.createElement("button");
    addBtn.type = "button";
    addBtn.className = "settings-btn settings-btn-secondary";
    addBtn.textContent = "+ 添加机器";
    addBtn.addEventListener("click", () => {
      this.appendCard({ ...HOST_DEFAULTS });
      // 空白机器先不写 config（缺必填字段无意义）；用户填了字段 change 时才 save。
    });
    addRow.appendChild(addBtn);
    group.appendChild(addRow);

    // 4. Feature ②：远端 ↗ 拉前用的只读 ccm wrapper 片段
    this.buildWrapperSnippetRow(group);

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
        "选一个 ~/.ssh/config 里的主机别名，自动用 `ssh -G` 解析出 host/port/user/私钥路径\n" +
          "并**新增一台机器**填好（免手敲）。仍可手动微调任意字段。",
      ),
    );
    row.appendChild(labelLine);

    this.importSelect = document.createElement("select");
    this.importSelect.className = "settings-input settings-input-select settings-input-wide";
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "选择一个主机别名…（导入为新机器）";
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

  /** 选了别名 → resolve_ssh_host → 新增一台机器并填好 → 保存。 */
  private async onImportAlias(): Promise<void> {
    const alias = this.importSelect.value;
    if (!alias) return;
    try {
      const resolved = await invoke<ResolvedHost>("resolve_ssh_host", { alias });
      const card = this.appendCard({ ...HOST_DEFAULTS });
      card.applyResolved(resolved, alias);
      await this.save();
      this.showBanner(`已从别名「${alias}」导入为新机器。`);
    } catch (e) {
      console.warn("resolve_ssh_host failed:", e);
      this.showBanner(`导入别名「${alias}」失败：${String(e)}`);
    } finally {
      this.importSelect.value = "";
    }
  }

  /**
   * Feature ②：远端 ↗ 拉前的只读 `ccm` wrapper 片段。纯 DOM/信息展示，不读写 config。
   */
  private buildWrapperSnippetRow(parent: HTMLElement): void {
    const row = document.createElement("div");
    row.className = "settings-row settings-row-stack";

    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = "远端 ↗ 拉前（可选）";
    label.appendChild(
      makeInfoIcon(
        "想让本地 ↗ 拉前对应的 ssh 窗口，远端 `.bashrc` 里需要下面的 `ccm` 函数，并用\n" +
          "`ccm` 代替 `claude` 启动。ccm 会周期性把 ssh 窗口标题设成 `ccm-rbind-<sid>`，\n" +
          "本地 monitor 扫到即绑定该窗口。\n\n" +
          "✅ 每台机器卡片上的「装 ccm 助手」按钮可一键装到远端 `~/.bashrc`（先备份原文件、\n" +
          "幂等可重装）；也可手动复制下面片段（zsh / 自定义 profile 用）。\n\n" +
          "⚠ 限制：多个 ssh 会话若开在同一个 Windows Terminal 窗口的不同 tab 里，↗ 只能\n" +
          "拉起该窗口、无法切到具体 tab。建议每个远端会话单独开窗。",
      ),
    );
    row.appendChild(label);

    const pre = document.createElement("pre");
    pre.className = "remote-wrapper-snippet";
    pre.textContent = CCM_WRAPPER_SNIPPET;
    row.appendChild(pre);

    const btnRow = document.createElement("div");
    btnRow.className = "settings-row settings-row-end";
    const copyBtn = document.createElement("button");
    copyBtn.type = "button";
    copyBtn.className = "settings-btn settings-btn-secondary";
    copyBtn.textContent = "复制";
    copyBtn.addEventListener("click", () => {
      void navigator.clipboard.writeText(CCM_WRAPPER_SNIPPET).then(
        () => {
          const prev = copyBtn.textContent;
          copyBtn.textContent = "已复制";
          window.setTimeout(() => {
            copyBtn.textContent = prev;
          }, 1500);
        },
        (e) => console.warn("copy ccm wrapper failed:", e),
      );
    });
    btnRow.appendChild(copyBtn);
    row.appendChild(btnRow);

    parent.appendChild(row);
  }

  // === 数据 ===

  /** 读出当前整段 RemoteConfig（enabled + 所有卡片）。 */
  private collect(): RemoteConfig {
    return {
      enabled: this.enabledCheckbox.checked,
      hosts: this.cards.map((c) => c.collect()),
    };
  }

  /** 任一控件变化 → 组装 → merge 进 config.json → 提示重启。 */
  private async save(): Promise<void> {
    const next = this.collect();
    // best-effort UI 校验：启用但某台缺必填字段 → 软提示（不拦保存，后端会跳过该台）。
    const incompleteCount = next.enabled
      ? next.hosts.filter((h) => !h.host || !h.user || !h.daemonPath).length
      : 0;
    // 指纹格式软校验：非空且不以 SHA256: 开头 → 大概率粘错字段。
    const fingerprintLooksOff = next.hosts.some(
      (h) => !!h.hostKeyFingerprint && !h.hostKeyFingerprint.startsWith("SHA256:"),
    );

    if (incompleteCount > 0) {
      this.showBanner(
        `已保存，但有 ${incompleteCount} 台 host/user/daemonPath 不完整 —— 后端会跳过这些台。补全后重启 monitor 才会连。`,
      );
    } else if (fingerprintLooksOff) {
      this.showBanner("已保存。注意：某台主机指纹不是 `SHA256:` 开头格式 —— 请确认没粘错。");
    }

    try {
      await writeRemoteConfig(next);
      const changed = !sameRemote(next, this.original);
      this.original = next;
      if (changed && incompleteCount === 0 && !fingerprintLooksOff) {
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

// === config.json 读写（多机，向后兼容旧单对象）===

/** 把一个任意 JSON 对象规整成 RemoteHostConfig（缺失/类型不对走默认）。 */
function coerceHost(obj: Record<string, unknown>): RemoteHostConfig {
  const str = (k: string, d: string) => (typeof obj[k] === "string" ? (obj[k] as string) : d);
  const host = str("host", HOST_DEFAULTS.host);
  return {
    // label 缺省回退 host（与 Rust origin_label 一致：空 label → host）
    label: str("label", "") || host,
    host,
    port:
      typeof obj.port === "number" && Number.isFinite(obj.port)
        ? (obj.port as number)
        : HOST_DEFAULTS.port,
    user: str("user", HOST_DEFAULTS.user),
    keyPath: str("keyPath", HOST_DEFAULTS.keyPath),
    daemonPath: str("daemonPath", HOST_DEFAULTS.daemonPath),
    hostKeyFingerprint: str("hostKeyFingerprint", HOST_DEFAULTS.hostKeyFingerprint),
  };
}

/**
 * 读 config.json 的 `remote` 段 → RemoteConfig。**向后兼容**：有 `hosts` 数组 → 逐台读；
 * 无 `hosts` 但有 `host`（旧单对象）→ 归一成 1 台；都没有 → 空列表。永不抛。
 */
export async function readRemoteConfig(): Promise<RemoteConfig> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const r = cfg.remote;
    if (r === null || typeof r !== "object") return { enabled: false, hosts: [] };
    const obj = r as Record<string, unknown>;
    const enabled = typeof obj.enabled === "boolean" ? obj.enabled : false;

    let hosts: RemoteHostConfig[] = [];
    if (Array.isArray(obj.hosts)) {
      hosts = obj.hosts
        .filter((h): h is Record<string, unknown> => h !== null && typeof h === "object")
        .map(coerceHost);
    } else if (typeof obj.host === "string" && obj.host) {
      // 旧单对象形态：把 remote 自身当一台。
      hosts = [coerceHost(obj)];
    }
    return { enabled, hosts };
  } catch (e) {
    console.warn("readRemoteConfig failed:", e);
    return { enabled: false, hosts: [] };
  }
}

/**
 * 把 RemoteConfig MERGE 进 config.json 顶层的 `remote` 键，不动其他字段。
 * 写成 `{ enabled, hosts: [...] }`（升级旧单对象形态）；key 是 camelCase，与 Rust
 * `lib.rs::load_remote_configs` 读的键严格一致。
 */
export async function writeRemoteConfig(next: RemoteConfig): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg.remote = {
    enabled: next.enabled,
    hosts: next.hosts.map((h) => ({
      label: h.label,
      host: h.host,
      port: h.port,
      user: h.user,
      keyPath: h.keyPath,
      daemonPath: h.daemonPath,
      hostKeyFingerprint: h.hostKeyFingerprint,
    })),
  };
  await saveConfig(cfg);
}

function sameHost(a: RemoteHostConfig, b: RemoteHostConfig): boolean {
  return (
    a.label === b.label &&
    a.host === b.host &&
    a.port === b.port &&
    a.user === b.user &&
    a.keyPath === b.keyPath &&
    a.daemonPath === b.daemonPath &&
    a.hostKeyFingerprint === b.hostKeyFingerprint
  );
}

function sameRemote(a: RemoteConfig, b: RemoteConfig): boolean {
  return (
    a.enabled === b.enabled &&
    a.hosts.length === b.hosts.length &&
    a.hosts.every((h, i) => sameHost(h, b.hosts[i]))
  );
}
