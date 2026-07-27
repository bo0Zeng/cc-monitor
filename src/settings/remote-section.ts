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

import { invoke, Channel } from "@tauri-apps/api/core";
import { AGENT_PROFILE } from "../agent-profile";
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir, join } from "@tauri-apps/api/path";
import { openSftpPanel } from "../sftp/panel";
import { openPortForwardPanel } from "../views/port-forward";
import { deriveTmuxName } from "../remote-launch";
import { runRemoteLauncher } from "../remote-launch-run";
import { fetchAccounts, isSelectable, currentWorkingAccount, withAccount } from "../accounts";
// F12：配置数据层已抽到 src/remote-config.ts（治分层倒挂）——UI 从数据模块 import，不再自持 CRUD。
import {
  HOST_DEFAULTS,
  parseAddressLines,
  readRemoteConfig,
  writeRemoteConfig,
  type RemoteHostConfig,
  type RemoteConfig,
} from "../remote-config";
import { makeInfoIcon } from "./info-icon";

/** `resolve_ssh_host` 的返回（Rust ResolvedHost，camelCase）。 */
interface ResolvedHost {
  host: string;
  port: number;
  user: string;
  keyPath: string | null;
  proxyJump: string | null; // F57
}

/** F57：批量导入预览的一台（Rust ImportGroup/ImportMember，camelCase）。 */
interface ImportMember {
  alias: string;
  host: string;
  port: number;
  proxyJump: string | null;
}
interface ImportGroup {
  label: string;
  host: string;
  port: number;
  user: string;
  keyPath: string | null;
  addresses: string[];
  jump: string | null;
  members: ImportMember[];
}

/** F46：连接分阶段事件（Rust ConnectStage，serde tag=kind camelCase）。 */
export type ConnectStage =
  | { kind: "dialing"; endpoint: string }
  | { kind: "hostKey"; endpoint: string; fingerprint: string }
  | { kind: "failed"; endpoint: string; reason: string }
  | { kind: "won"; endpoint: string }
  | { kind: "auth"; ok: boolean; detail: string | null }
  | { kind: "established" };

/** F46：阶段事件 → 泳道行的图标 + 文案。纯函数便于单测。 */
export function describeStage(st: ConnectStage): { icon: string; text: string } {
  switch (st.kind) {
    case "dialing":
      return { icon: "→", text: `拨号 ${st.endpoint}` };
    case "hostKey":
      return { icon: "🔑", text: `${st.endpoint} 主机指纹 ${st.fingerprint}` };
    case "failed":
      return { icon: "✗", text: `${st.endpoint} 失败：${st.reason}` };
    case "won":
      return { icon: "✓", text: `${st.endpoint} 胜出（其余地址已取消）` };
    case "auth":
      return st.ok
        ? { icon: "✓", text: "鉴权通过" }
        : { icon: "✗", text: `鉴权失败：${st.detail ?? ""}` };
    case "established":
      return { icon: "●", text: "连接就绪" };
    default: {
      // F46 建议 E：穷尽性兜底——未来新增 ConnectStage 变体时编译期(never)即报错。
      const _never: never = st;
      return { icon: "·", text: String((_never as { kind?: string }).kind ?? "") };
    }
  }
}

/** `test_remote_connection` 的返回（Rust ConnTestResult，camelCase）。 */
interface ConnTestResult {
  sshOk: boolean;
  fingerprint: string | null;
  /** F45：竞发胜出的地址（host:port）。多地址 TOFU 首连时告知固化的是哪条路径的指纹。 */
  endpoint: string | null;
  daemonOk: boolean;
  daemonHello: string | null;
  message: string;
}

// F12：`RemoteHostConfig` / `RemoteConfig` / `parseAddressLines` / `sftpEligibleHosts` 已移入
// `src/remote-config.ts`（数据层），本文件从那里 import（见顶部）。

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
 *   **F02 起这些实现搬进了 `~/.local/bin/ccm`（可执行文件）**，本文件只 import 别名块。
 *   为什么不再装成 shell 函数：函数**优先于 PATH**，与用户已有同名函数硬冲突且必然遮蔽
 *   （实测：共存时新 CLI 一次都跑不到，且是静默的）；且远端是 zsh/fish 时 `.bashrc`
 *   根本不被 source，函数形态拿不到。别名块只做**组合**（`cct() { ccm --tmux "$@"; }`），
 *   不含任何实现——自定义在组合层，不在实现层。
 *
 * `ccm-rbind-%s` 标记必须与后端 `bind.rs` 的 `format!("ccm-rbind-{sid}")` 完全一致。
 *
 * tmux 自适配（Batch7 真机排查实证）：tmux 默认 `set-titles off`——OSC 标题转义
 * 只落到 pane title、到不了外层 ssh 终端窗口标题，marker 被截住导致绑定必然
 * 失败，而 tmux 恰是远端最常见形态。原语内自动对**当前 session** 开直通
 * （session 级选项，不写 tmux.conf、不影响其它 session）。
 */
// 单一来源：shared/ccm-aliases.sh（后端 sftp.rs include_str! 同一文件，杜绝漂移）
import CCM_WRAPPER_SNIPPET from "../../shared/ccm-aliases.sh?raw";

export interface RemoteSectionOptions {
  /** 被 CollapsibleGroup 包起来时传 headless: true，不渲染自己的小标题。 */
  headless?: boolean;
}

// === 共享 DOM 小工具 ===

/**
 * F43：是否显示「重置为 TOFU」按钮——当且仅当当前已固化了非空指纹。
 * 抽成纯函数便于单测（trim 后非空 = 已固化严格校验）。
 */
export function shouldShowResetFingerprint(current: string): boolean {
  return current.trim().length > 0;
}

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
  private addressesInput!: HTMLTextAreaElement;
  private jumpInput!: HTMLInputElement;
  /** F59：daemonless 降级读取开关（无 daemon 时纯 tail 轮询读）。 */
  private daemonlessInput!: HTMLInputElement;
  /** 依当前指纹值显隐「重置为 TOFU」按钮（load / 重置后调用）。 */
  private syncResetFpVisibility!: () => void;
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
      addresses: parseAddressLines(this.addressesInput.value),
      jump: this.jumpInput.value.trim(),
      daemonless: this.daemonlessInput.checked,
    };
  }

  /** 导入别名时填充连接参数（host/port/user/keyPath + daemon 兜底 + label=别名）。 */
  applyResolved(resolved: ResolvedHost, alias: string): void {
    if (!this.labelInput.value.trim()) this.labelInput.value = alias;
    this.hostInput.value = resolved.host;
    this.portInput.value = resolved.port ? String(resolved.port) : "22";
    this.userInput.value = resolved.user;
    this.keyPathInput.value = resolved.keyPath ?? "";
    if (resolved.proxyJump) this.jumpInput.value = resolved.proxyJump; // F57 S-2:单别名也填跳板
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
    // F43：重置指纹入口——补 aterm 自曝的坑（服务器合法换 host key 后严格校验会永久
    // 拒连，此前只能手动清空输入框、无风险告知）。仅在已固化指纹时显示。
    const resetFpBtn = document.createElement("button");
    resetFpBtn.type = "button";
    resetFpBtn.className = "settings-btn settings-btn-secondary";
    resetFpBtn.textContent = "重置为 TOFU";
    resetFpBtn.title = "清除已固化的主机指纹，下次连接重新捕获（仅在你确知服务器合法换过 host key 时用）";
    resetFpBtn.addEventListener("click", () => this.onResetFingerprint());
    this.fingerprintInput.parentElement?.appendChild(resetFpBtn);
    const syncResetVisibility = (): void => {
      resetFpBtn.style.display = shouldShowResetFingerprint(this.fingerprintInput.value)
        ? "inline-block"
        : "none";
    };
    this.fingerprintInput.addEventListener("input", syncResetVisibility);
    this.syncResetFpVisibility = syncResetVisibility;
    syncResetVisibility();

    // F45：备用地址（多行，每行一个 host / host:port / [IPv6]:port）。竞发时首选 host
    // 字段、其余并发拨号，首个握手成功者胜——内网 IP 死了公网顶上。
    const addrRow = document.createElement("div");
    addrRow.className = "settings-row settings-row-stack";
    const addrLabel = document.createElement("span");
    addrLabel.className = "settings-label";
    addrLabel.textContent = "备用地址 (addresses，可选，每行一个)";
    addrRow.appendChild(addrLabel);
    this.addressesInput = document.createElement("textarea");
    this.addressesInput.className = "settings-input settings-input-wide";
    this.addressesInput.rows = 2;
    this.addressesInput.spellcheck = false;
    this.addressesInput.placeholder = "10.0.0.2\npi.example.com:2222\n[fe80::1]:22（首选地址填上方 host）";
    this.addressesInput.addEventListener("change", onChange);
    addrRow.appendChild(this.addressesInput);
    body.appendChild(addrRow);

    // F56：跳板 ProxyJump——填另一台已配置主机的 label（空=直连）。经该跳板机隧道连本机。
    this.jumpInput = buildTextRow(
      body,
      "跳板 (jump，可选)",
      "另一台已配置主机的 label（空=直连；经该跳板隧道连本机）",
      onChange,
    );

    // F59：daemonless 降级读取——该机不部署/不连 daemon，纯 SSH exec tail 轮询读会话 jsonl。
    const dlRow = document.createElement("label");
    dlRow.className = "settings-row settings-row-checkbox";
    this.daemonlessInput = document.createElement("input");
    this.daemonlessInput.type = "checkbox";
    this.daemonlessInput.className = "settings-checkbox";
    this.daemonlessInput.addEventListener("change", onChange);
    dlRow.appendChild(this.daemonlessInput);
    const dlLabel = document.createElement("span");
    dlLabel.className = "settings-checkbox-label";
    dlLabel.textContent = "daemonless 降级读取（无需 daemon）";
    dlRow.appendChild(dlLabel);
    dlRow.appendChild(
      makeInfoIcon(
        "勾选后该机**不部署 / 不连 daemon**，改用纯 SSH exec `find`+`tail` 轮询读会话 jsonl。\n" +
          "适合装不了 daemon 的主机（异构架构 / 无权限 / BSD·macOS）。\n" +
          "⚠ 能力子集（降级）：无后台会话跟踪 / 无运行状态灯 / 无拥塞信号 / 仅显示最近 30 分钟活跃的会话。\n" +
          "会话内容照常可读。需重启 monitor 才生效。",
      ),
    );
    body.appendChild(dlRow);

    // 安装位置提示：明确告诉用户「在哪里装什么」。
    const installInfo = document.createElement("div");
    installInfo.className = "settings-hint remote-install-info";
    installInfo.textContent =
      "安装位置：① daemon（远端数据源，必需）→ 上方「daemon 路径」填的位置" +
      "（默认 ~/.cc-monitor/bin/cc-monitor-remote）+ 同目录 .build_id；启用远端后连接时会自动安装，" +
      "下面按钮供手动装 / 卸。② ccm 启动器（可选）→ 两部分：CLI 本体装到远端 " +
      "~/.local/bin/ccm（可执行文件），别名块写进 ~/.bashrc 的 cc-monitor BEGIN/END 标记块" +
      "（先备份原文件、只动标记块内）。装好后终端可用：ccm（起会话）/ ccm --tmux（tmux 里起）/ " +
      "ccm --account <名>（指定账号），`ccm --help` 看全部修饰。别名（cc / cct / cch）不覆盖你" +
      "已有的同名函数。这条路径与 cc-monitor 自己起会话**是同一套实现**——终端起的会话 app 认得出、" +
      "能 attach、能换号重启。";
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

    // F48：打开该台的 SFTP 文件面板（独立 overlay）。
    actionRow.appendChild(
      mkBtn("文件", "", "打开 SFTP 文件面板（浏览 / 上传 / 下载 / 管理远端文件）", () => {
        const cfg = this.collect();
        if (!cfg.host || !cfg.user) {
          this.renderTestResult(null, "请先填好 host / user 再打开文件面板。");
          return;
        }
        openSftpPanel(cfg);
      }),
    );

    // F50：一键把本地公钥推到远端 authorized_keys（onboarding 免密）。
    const pushKeyBtn = mkBtn(
      "推送公钥",
      "settings-btn-secondary",
      "把本地公钥追加到远端 ~/.ssh/authorized_keys（免密登录）；已填私钥则取同名 .pub，否则弹框选文件",
      () => void this.onPushPubkey(pushKeyBtn),
    );
    actionRow.appendChild(pushKeyBtn);

    // F53：在这台机开新 Claude——填工作目录/tmux 名/命令,在远端 tmux 里启动全新会话。
    actionRow.appendChild(
      mkBtn(
        "开新 Claude",
        "settings-btn-secondary",
        "在这台远端机的 tmux 会话里启动一个全新 Claude（填工作目录 + 会话名 + 启动命令）",
        () => this.openLauncherDialog(),
      ),
    );

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
      "装 ccm 启动器",
      "settings-btn-secondary",
      "部署 ccm CLI 到远端 ~/.local/bin 并把别名块写进 ~/.bashrc；先备份原文件、幂等可重装",
      () => void this.onInstallCcm(),
    );
    actionRow.appendChild(this.installButton);

    this.ccmUninstallButton = mkBtn(
      "卸载 ccm",
      "settings-btn-secondary",
      "从远端 ~/.bashrc 删掉 cc-monitor 的别名块（先备份；块外内容不动。CLI 本体 ~/.local/bin/ccm 需手动删）",
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
    this.addressesInput.value = cfg.addresses.join("\n");
    this.jumpInput.value = cfg.jump ?? "";
    this.daemonlessInput.checked = cfg.daemonless ?? false;
    this.syncResetFpVisibility();
  }

  /**
   * F43：重置主机指纹 → 回到 TOFU（清空固化指纹）。LOUD 二次确认——清除后下次连接会
   * 接受新主机密钥;若此刻正被中间人攻击,会信任攻击者的密钥。仅当确知服务器合法换过
   * host key 时才该重置。
   */
  private onResetFingerprint(): void {
    const host = this.hostInput.value.trim() || this.labelInput.value.trim() || "该主机";
    if (
      !window.confirm(
        `确认重置 ${host} 的主机指纹？\n\n` +
          "清除后下次连接将以 TOFU 重新捕获并接受主机密钥。\n" +
          "⚠ 若此刻网络正被中间人攻击，重置会让 monitor 信任攻击者的密钥。\n" +
          "仅在你确知服务器合法更换过 host key（重装系统 / 轮换密钥）时才重置。",
      )
    ) {
      return;
    }
    this.fingerprintInput.value = "";
    this.syncResetFpVisibility();
    this.hooks.onChange(); // 触发 section 保存（写回 config，指纹置空 = 严格校验解除）
    this.showResetFeedback();
  }

  /** 重置后就地反馈（复用测试结果区显示一行提示）。 */
  private showResetFeedback(): void {
    this.testResult.innerHTML = "";
    this.testResult.style.display = "block";
    const line = document.createElement("div");
    line.className = "remote-test-line remote-test-caution";
    line.textContent = "已重置为 TOFU：下次连接将重新捕获主机指纹（记得测试连接后重新固化）。";
    this.testResult.appendChild(line);
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
    // F46：连接分阶段事件泳道——测试开始即清空日志区、随 Channel 事件实时追加。
    this.testResult.innerHTML = "";
    this.testResult.style.display = "block";
    const stageLog = document.createElement("div");
    stageLog.className = "remote-stage-log";
    this.testResult.appendChild(stageLog);
    const onStage = new Channel<ConnectStage>();
    onStage.onmessage = (st) => this.appendStageLine(stageLog, st);
    try {
      const res = await invoke<ConnTestResult>("test_remote_connection", { cfg, onStage });
      this.renderTestResult(res, null, stageLog);
    } catch (e) {
      console.warn("test_remote_connection failed:", e);
      this.renderTestResult(null, `测试失败：${String(e)}`, stageLog);
    } finally {
      this.testButton.disabled = false;
      this.testButton.textContent = prevLabel;
    }
  }

  /** F46：把一条阶段事件渲染进「连接过程」泳道日志。 */
  private appendStageLine(log: HTMLElement, st: ConnectStage): void {
    const line = document.createElement("div");
    line.className = "remote-stage-line";
    const { icon, text } = describeStage(st);
    line.textContent = `${icon} ${text}`;
    log.appendChild(line);
    log.scrollTop = log.scrollHeight; // F46 建议 D：新事件自动滚到底,最新阶段始终可见
  }

  /** F10：点「装 ccm 助手」——把 CCM_WRAPPER_SNIPPET 经 SFTP 装进这台远端的 ~/.bashrc。 */
  private async onInstallCcm(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.testResult.style.display = "block";
      this.testResult.textContent = "请先填好 host / user 再安装 ccm 启动器。";
      return;
    }
    this.installButton.disabled = true;
    const prev = this.installButton.textContent;
    this.installButton.textContent = "安装中…";
    this.testResult.style.display = "block";
    this.testResult.textContent = "安装 ccm 启动器中…";
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

  /** F50：一键推送本地公钥到远端 authorized_keys。已填私钥 → 取同名 .pub；否则弹框选 .pub。 */
  private async onPushPubkey(btn: HTMLButtonElement): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.showResultText("请先填好 host / user 再推送公钥。");
      return;
    }
    let pubKeyPath: string | null = null;
    if (!cfg.keyPath) {
      // 没配私钥路径 → 让用户挑一个 .pub（后端无从推断）。默认落在 ~/.ssh
      // （`~` 不会被 dialog 展开,须经 homeDir() 拼绝对路径;拿不到则不设）。
      let defaultPath: string | undefined;
      try {
        defaultPath = await join(await homeDir(), ".ssh");
      } catch {
        /* 拿不到 home → 不设 defaultPath,dialog 用系统默认起点 */
      }
      const picked = await open({
        title: "选择要推送的公钥 (.pub)",
        multiple: false,
        directory: false,
        defaultPath,
        filters: [{ name: "公钥", extensions: ["pub"] }],
      });
      if (typeof picked !== "string") return; // 取消 / 多选保护
      pubKeyPath = picked;
    }
    await this.runRemoteAction(btn, "推送公钥中", async () => {
      const r = await invoke<{ outcome: string; pubPath: string }>("push_public_key", {
        cfg,
        pubKeyPath,
      });
      return r.outcome === "added"
        ? `公钥已推送（ADDED）：${r.pubPath}`
        : `公钥已存在，无需重复（ALREADY）：${r.pubPath}`;
    });
  }

  /**
   * F53：「开新 Claude」即席弹框——填工作目录 / tmux 会话名 / 启动命令,在远端 tmux 里启动
   * 一个全新 Claude 会话。不存预设(不动 config)。origin 用 label(空则 host,后端按 origin_label
   * 选台);host 未保存时 launch 会失败→runRemoteLauncher 回退复制命令(仍可用)。
   */
  private openLauncherDialog(): void {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.showResultText("请先填好 host / user 再开新 Claude。");
      return;
    }
    const origin = cfg.label.trim() || cfg.host;

    const back = document.createElement("div");
    back.className = "launcher-back";
    const box = document.createElement("div");
    box.className = "launcher-box";
    const title = document.createElement("div");
    title.className = "launcher-title";
    title.textContent = `在 ${origin} 开新 Claude`;
    box.appendChild(title);

    const mkField = (labelText: string, placeholder: string): HTMLInputElement => {
      const row = document.createElement("label");
      row.className = "launcher-field";
      const span = document.createElement("span");
      span.textContent = labelText;
      const input = document.createElement("input");
      input.type = "text";
      input.placeholder = placeholder;
      input.spellcheck = false;
      row.append(span, input);
      box.appendChild(row);
      return input;
    };
    const cwdInput = mkField("工作目录", "/home/pi/proj（留空=登录默认目录）");
    const nameInput = mkField("tmux 会话名", "留空则按工作目录名自动生成");
    const cmdInput = mkField("启动命令", "claude（可自定义，如 claude --model opus）");
    // 工作目录变化 → 实时预览留空时将用的派生名(placeholder)。
    cwdInput.addEventListener("input", () => {
      nameInput.placeholder = cwdInput.value.trim()
        ? `留空则用 ${deriveTmuxName(cwdInput.value)}`
        : "留空则按工作目录名自动生成";
    });

    // A4：账号下拉。异步填充——账号库不可用（daemonless / 旧 / 未启用）则整行不显 → 不注入
    // configDir → 行为与旧版逐字节一致（§7 降级）。选中某账号 = 起会话时注入其 CLAUDE_CONFIG_DIR。
    const acctRow = document.createElement("label");
    acctRow.className = "launcher-field";
    acctRow.style.display = "none";
    const acctSpan = document.createElement("span");
    acctSpan.textContent = "账号（默认＝当前账号）";
    const acctSelect = document.createElement("select");
    acctSelect.className = "launcher-acct-select";
    acctRow.append(acctSpan, acctSelect);
    box.appendChild(acctRow);
    void (async () => {
      try {
        const state = await fetchAccounts(origin);
        if (!state.available) return;
        const sel = state.accounts.filter(isSelectable);
        if (sel.length < 1) return;
        const none = document.createElement("option");
        none.value = "";
        // U8：说清后果——「不指定」= 用远端 ~/.claude 那套基座凭据，**不受当前账号影响**。
        none.textContent = "不指定（用远端登录的基座账号，不注入 CLAUDE_CONFIG_DIR）";
        acctSelect.appendChild(none);
        for (const a of sel) {
          const opt = document.createElement("option");
          opt.value = a.name;
          opt.textContent = a.email ? `${a.name} · ${a.email}` : a.name;
          acctSelect.appendChild(opt);
        }
        // 预选当前账号（若它可选）——用户可改或选「不指定」。
        const def = currentWorkingAccount(state);
        if (def && isSelectable(def)) acctSelect.value = def.name;
        acctRow.style.display = "";
      } catch {
        /* 账号库拿不到 → 不显账号行，默认起会话仍可用 */
      }
    })();

    const foot = document.createElement("div");
    foot.className = "launcher-foot";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "settings-btn";
    cancel.textContent = "取消";
    cancel.addEventListener("click", () => back.remove());
    const start = document.createElement("button");
    start.type = "button";
    start.className = "settings-btn settings-btn-primary";
    start.textContent = "开始";
    start.addEventListener("click", () => {
      const cwd = cwdInput.value.trim();
      const name = nameInput.value.trim() || deriveTmuxName(cwd);
      const command = cmdInput.value.trim() || AGENT_PROFILE.defaultLauncher;
      const accName = acctSelect.value; // "" = 不指定
      back.remove();
      // A4：新会话无 sid → 不记 lastAccount；withAccount 统一解析注入（不可选则退化默认起）。
      void withAccount(origin, accName || null, (cd, an, mo) =>
        runRemoteLauncher(origin, cwd, name, command, cd, an, mo),
      );
    });
    foot.append(cancel, start);
    box.appendChild(foot);

    // 点遮罩空白 / Esc 取消(不冒泡到设置面板)。
    back.addEventListener("click", (e) => {
      if (e.target === back) back.remove();
    });
    back.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        back.remove();
      }
    });
    back.appendChild(box);
    document.body.appendChild(back);
    cwdInput.focus();
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

  /** 渲染测试结果：SSH ✓/✗、指纹（+可固化）、daemon ✓/✗（+hello）。
   * F46：`keepLog` 传入时保留其上方的「连接过程」阶段泳道（清空其余旧结果）。 */
  private renderTestResult(
    res: ConnTestResult | null,
    hardError: string | null,
    keepLog?: HTMLElement,
  ): void {
    // 清空旧结果但保留阶段泳道日志（若有）。
    for (const child of Array.from(this.testResult.children)) {
      if (child !== keepLog) child.remove();
    }
    this.testResult.style.display = "block";

    if (hardError !== null) {
      const line = document.createElement("div");
      line.className = "remote-test-line remote-test-err";
      line.textContent = hardError;
      this.testResult.appendChild(line);
      return;
    }
    if (res === null) return;

    this.testResult.appendChild(
      makeStatusLine(
        res.sshOk,
        res.sshOk
          ? `SSH 连接成功${res.endpoint ? `（经 ${res.endpoint}）` : ""}`
          : "SSH 连接失败",
      ),
    );

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
    this.syncResetFpVisibility(); // F43：程序化赋值不触发 input 事件，手动同步重置按钮显隐
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

    // F58：端口转发管理台入口。
    const pfRow = document.createElement("div");
    pfRow.className = "settings-row settings-row-actions";
    const pfBtn = document.createElement("button");
    pfBtn.type = "button";
    pfBtn.className = "settings-btn settings-btn-secondary";
    pfBtn.textContent = "端口转发…";
    pfBtn.title = "本地端口转发(-L)管理台:把远端机(或其内网)端口映到本机,经已配置的 SSH 连接隧道";
    pfBtn.addEventListener("click", () => openPortForwardPanel());
    pfRow.appendChild(pfBtn);
    group.appendChild(pfRow);

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

    // F57：批量导入——一次导入全部主机,智能聚合同机多地址,预览可拆分。
    const batchBtn = document.createElement("button");
    batchBtn.type = "button";
    batchBtn.className = "settings-btn settings-btn-secondary";
    batchBtn.textContent = "批量导入…";
    batchBtn.title =
      "一次导入 ~/.ssh/config 全部主机；同密钥+同用户+同基名前缀的别名智能聚合成一台多地址主机（预览可拆分/勾选）";
    batchBtn.addEventListener("click", () => void this.onBatchImport());
    row.appendChild(batchBtn);

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

  /** F57：批量导入——import_ssh_hosts（智能聚合）→ 预览弹框（可拆分/勾选）→ 建卡。 */
  private async onBatchImport(): Promise<void> {
    let groups: ImportGroup[];
    try {
      groups = await invoke<ImportGroup[]>("import_ssh_hosts");
    } catch (e) {
      this.showBanner(`批量导入失败：${String(e)}`);
      return;
    }
    if (groups.length === 0) {
      this.showBanner("~/.ssh/config 里没有可导入的主机。");
      return;
    }
    this.showImportPreview(groups);
  }

  /** F57：聚合组 → RemoteHostConfig（label 可被预览编辑覆盖）。 */
  private groupToCfg(g: ImportGroup, label: string): RemoteHostConfig {
    return {
      label: label.trim() || g.label,
      host: g.host,
      port: g.port || 22,
      user: g.user,
      keyPath: g.keyPath ?? "",
      daemonPath: g.user ? defaultDaemonPathFor(g.user) : "",
      hostKeyFingerprint: "",
      addresses: g.addresses,
      jump: g.jump ?? "",
      daemonless: false, // F59：从 ssh config 导入的主机默认走 daemon 路径
    };
  }

  /** F57：拆分——组内单个成员 → 一台独立机（label=别名,用成员级 port/proxyJump 精确还原,无备用地址）。 */
  private memberToCfg(g: ImportGroup, m: ImportMember): RemoteHostConfig {
    return {
      label: m.alias,
      host: m.host,
      port: m.port || 22,
      user: g.user,
      keyPath: g.keyPath ?? "",
      daemonPath: g.user ? defaultDaemonPathFor(g.user) : "",
      hostKeyFingerprint: "",
      addresses: [],
      jump: m.proxyJump ?? "",
      daemonless: false, // F59：从 ssh config 导入的主机默认走 daemon 路径
    };
  }

  /** F57：批量导入预览弹框——列各聚合组,勾选包含 / 拆分成独立机 / 改 label,确认建卡。 */
  private showImportPreview(groups: ImportGroup[]): void {
    type Row = { g: ImportGroup; include: boolean; split: boolean; label: string };
    const state: Row[] = groups.map((g) => ({ g, include: true, split: false, label: g.label }));

    const back = document.createElement("div");
    back.className = "import-preview-back";
    const box = document.createElement("div");
    box.className = "import-preview-box";
    const title = document.createElement("div");
    title.className = "import-preview-title";
    title.textContent = `从 ~/.ssh/config 导入（检测到 ${groups.length} 台）`;
    box.appendChild(title);

    const list = document.createElement("div");
    list.className = "import-preview-list";
    for (const s of state) {
      const src = s.g.members.map((m) => m.alias).join(", ");
      const addrHint = s.g.addresses.length ? ` +${s.g.addresses.length} 备用地址` : "";
      const jumpHint = s.g.jump ? ` · 跳板 ${s.g.jump}` : "";
      const aggLine = `${s.g.host}${addrHint} · ${s.g.user || "(无 user)"}${jumpHint} · 来源: ${src}`;

      const item = document.createElement("div");
      item.className = "import-preview-item";
      const inc = document.createElement("input");
      inc.type = "checkbox";
      inc.checked = true;
      inc.addEventListener("change", () => (s.include = inc.checked));
      item.appendChild(inc);

      const body = document.createElement("div");
      body.className = "import-preview-body";
      const labelInput = document.createElement("input");
      labelInput.type = "text";
      labelInput.className = "import-preview-label";
      labelInput.value = s.label;
      labelInput.addEventListener("input", () => (s.label = labelInput.value));
      body.appendChild(labelInput);
      const info = document.createElement("div");
      info.className = "import-preview-info";
      info.textContent = aggLine;
      body.appendChild(info);
      item.appendChild(body);

      if (s.g.members.length > 1) {
        const splitWrap = document.createElement("label");
        splitWrap.className = "import-preview-split";
        const split = document.createElement("input");
        split.type = "checkbox";
        split.addEventListener("change", () => {
          s.split = split.checked;
          info.textContent = split.checked
            ? `拆成 ${s.g.members.length} 台独立机: ${src}`
            : aggLine;
        });
        splitWrap.append(split, document.createTextNode("拆分"));
        item.appendChild(splitWrap);
      }
      list.appendChild(item);
    }
    box.appendChild(list);

    const foot = document.createElement("div");
    foot.className = "import-preview-foot";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "settings-btn";
    cancel.textContent = "取消";
    cancel.addEventListener("click", () => back.remove());
    const confirm = document.createElement("button");
    confirm.type = "button";
    confirm.className = "settings-btn settings-btn-primary";
    confirm.textContent = "导入";
    confirm.addEventListener("click", () => {
      back.remove();
      void this.applyImportPreview(state);
    });
    foot.append(cancel, confirm);
    box.appendChild(foot);

    back.addEventListener("click", (e) => {
      if (e.target === back) back.remove();
    });
    back.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        back.remove();
      }
    });
    back.appendChild(box);
    document.body.appendChild(back);
    box.querySelector("input")?.focus();
  }

  /** F57：把预览里勾选的组建成机器卡（拆分组建多台;同名 label 已存在则跳过）。 */
  private async applyImportPreview(
    state: Array<{ g: ImportGroup; include: boolean; split: boolean; label: string }>,
  ): Promise<void> {
    // 已存在的卡（导入前）→ 撞到就跳过（不重复导入）。批内新机同名 → 加后缀消歧（不丢机,F57-1）。
    const preExisting = new Set(
      this.cards.map((c) => {
        const cfg = c.collect();
        return cfg.label.trim() || cfg.host;
      }),
    );
    const usedInBatch = new Set<string>();
    let added = 0;
    let skipped = 0;
    const push = (cfg: RemoteHostConfig): void => {
      const base = cfg.label.trim() || cfg.host;
      if (preExisting.has(base)) {
        skipped++;
        return;
      }
      let key = base;
      let n = 2;
      while (usedInBatch.has(key)) key = `${base}-${n++}`;
      if (key !== base) cfg.label = key; // 批内同基名不同机 → 后缀消歧,绝不丢机
      usedInBatch.add(key);
      this.appendCard(cfg);
      added++;
    };
    for (const s of state) {
      if (!s.include) continue;
      if (s.split) {
        for (const m of s.g.members) push(this.memberToCfg(s.g, m));
      } else {
        push(this.groupToCfg(s.g, s.label));
      }
    }
    if (added > 0) await this.save();
    this.showBanner(
      `批量导入完成：新增 ${added} 台${skipped ? `，跳过 ${skipped} 台（同名已存在）` : ""}。`,
    );
  }

  /**
   * Feature ②：远端 ↗ 拉前的只读 `ccm` wrapper 片段。纯 DOM/信息展示，不读写 config。
   */
  private buildWrapperSnippetRow(parent: HTMLElement): void {
    // F81（#40）：默认折叠——原生 <details> 不带 open 属性即收起；点标题（<summary>）展开看片段。
    // 片段占位大、多数人配一次不再看，故默认收起。**不加 .settings-row（display:flex）**——flex 的
    // <details> 在部分浏览器折叠会失效（闭合仍渲染全部子元素）；用块流 + 子元素自身外边距排布。
    const row = document.createElement("details");
    row.className = "remote-wrapper-details";

    const label = document.createElement("summary");
    label.className = "settings-label remote-wrapper-summary";
    label.textContent = "远端 ↗ 拉前（可选）";
    label.appendChild(
      makeInfoIcon(
        "用 `ccm` 起会话（而非直接 `claude`），远端会周期性把 ssh 窗口标题设成\n" +
          "`ccm-rbind-<sid>`，本地 monitor 扫到即绑定该窗口；同时给 tmux 打上 @ccm_sid，\n" +
          "于是终端起的会话 app 也认得出、能 attach、能换号重启。\n\n" +
          "✅ 每台机器卡片上的「装 ccm 启动器」按钮一键装好（CLI 到 ~/.local/bin/ccm，\n" +
          "别名块到 ~/.bashrc，先备份、幂等可重装）；下面片段是别名块，可手动复制\n" +
          "（zsh / 自定义 profile 用；CLI 本体仍需用按钮部署）。\n\n" +
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
        (e) => console.warn("copy ccm aliases failed:", e),
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
// F12：`coerceAddresses` / `coerceHost` / `readRemoteConfig` / `findHostByOrigin` /
// `resolveRemoteConfigByOrigin` / `writeRemoteConfig` 已移入 `src/remote-config.ts`（数据层）。
// `sameHost` / `sameRemote`（下方）是 UI dirty-check，留本文件。

function sameHost(a: RemoteHostConfig, b: RemoteHostConfig): boolean {
  return (
    a.label === b.label &&
    a.host === b.host &&
    a.port === b.port &&
    a.user === b.user &&
    a.keyPath === b.keyPath &&
    a.daemonPath === b.daemonPath &&
    a.hostKeyFingerprint === b.hostKeyFingerprint &&
    a.jump === b.jump && // F56（D-I3）:仅改跳板也算变更，触发「需重启生效」提示
    a.daemonless === b.daemonless && // F59:仅改 daemonless 开关也算变更（触发「需重启生效」）
    // F45（Phase G 补）:仅改「备用地址」也算变更。此前独漏 addresses（jump/daemonless 都比了）
    // → 只改多地址、其它不动时「需重启生效」横幅被静默抑制，用户可能不重启、新地址不生效。
    a.addresses.length === b.addresses.length &&
    a.addresses.every((x, i) => x === b.addresses[i])
  );
}

function sameRemote(a: RemoteConfig, b: RemoteConfig): boolean {
  return (
    a.enabled === b.enabled &&
    a.hosts.length === b.hosts.length &&
    a.hosts.every((h, i) => sameHost(h, b.hosts[i]))
  );
}
