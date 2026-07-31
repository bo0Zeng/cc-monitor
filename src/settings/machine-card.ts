/**
 * S4b-3b-3（settings-ia）：单台远端机器的编辑卡片 —— **机器详情页的主体**。
 *
 * 从 `remote-section.ts` 提出来（那个文件此前 2100 行、住着两个类）。**接缝是天然的**：
 * `MachineCard` 早就是「一个类 = 一台机器」，S4b-1 起它已经独占一页、S4b-3b-2 起
 * 它的 body 被拆成「连接 / 组件」两栏交给详情页。这次只是让文件边界追上早已成型的职责边界。
 *
 * # 对外的三个契约（搬家时逐条保住，都有测试钉着）
 *
 * 1. `persistedKey` —— **卡片身份**（这张卡对应盘上哪一条）。S1 加的，不是渲染细节：
 *    origin 可被用户编辑，没有它改个名就会变成「新增一台 + 留下孤儿」。
 * 2. `parts()` —— 交出「连接 / 组件」两块，详情页据此分栏（S4b-3b-2）。
 * 3. `setPageMode()` —— 进入独占一页的形态（去折叠箭头与删除按钮）。
 */
import { Channel } from "@tauri-apps/api/core";
import { commands } from "../ipc/commands";
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir, join } from "@tauri-apps/api/path";
import { openSftpPanel } from "../sftp/panel";
import { makeInfoIcon } from "./info-icon";
import { invalidateCcmProbeCache } from "../ccm-probe";
import { recordFacet, type MachineFacet } from "./machine-status";
import { hostKey, type RemoteHostConfig } from "../remote-config";
import { parseAddressLines } from "../remote-config";
import { describeStage } from "./remote-section";
import type { ConnectStage } from "./remote-section";
import { AGENT_PROFILE } from "../agent-profile";
import { deriveTmuxName } from "../remote-launch";
import {
  fetchAccounts,
  isSelectable,
  currentWorkingAccount,
  withAccount,
} from "../accounts";
import { runRemoteLauncher } from "../remote-launch-run";
import type { ConnTestResult } from "../generated/ConnTestResult";
import type { ResolvedHost } from "../generated/ResolvedHost";

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
export interface MachineCardHooks {
  /** 任一字段变化 → 让 section 保存全部。 */
  onChange: () => void;
  /** 点删除 → 让 section 移除本卡片。 */
  onRemove: (card: MachineCard) => void;
  /** S4b：这张卡的状态/名字变了，宿主该刷新列表那一行。 */
  onStatusChanged?: (card: MachineCard) => void;
}
/**
 * daemonPath placeholder：必须是**绝对路径**。SSH exec 不经 shell，`~` 不会被展开，
 * 故用 `/home/<user>/...` 形式而非 `~/...`（避免误导用户以为 `~` 可用）。
 */
const DAEMON_PATH_PLACEHOLDER =
  "/home/<user>/.cc-monitor/bin/cc-monitor-remote";
/**
 * 按远端用户名生成 daemonPath 默认值（与自动部署的约定路径一致，
 * 见 doc/REMOTE-PHASE0-DEPLOY.md）。root 的 home 不在 /home 下，特判。
 * 只是预填——远端 home 不标准（如 macOS /Users）时用户可改，「测试连接」会暴露问题。
 */
export function defaultDaemonPathFor(user: string): string {
  const home = user === "root" ? "/root" : `/home/${user}`;
  return `${home}/.cc-monitor/bin/cc-monitor-remote`;
}
/**
 * F43：是否显示「重置为 TOFU」按钮——当且仅当当前已固化了非空指纹。
 * 抽成纯函数便于单测（trim 后非空 = 已固化严格校验）。
 */
export function shouldShowResetFingerprint(current: string): boolean {
  return current.trim().length > 0;
}

export class MachineCard {
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
  /** S4b-3（§5-1）：这台机器的 resume 启动命令（空 = 用全局默认）。 */
  private resumeCmdInput!: HTMLInputElement;
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
  /** S4b-3b-2：body 的两半 —— 详情页据此拆「连接 / 组件」两栏。 */
  private connectionPart!: HTMLElement;
  private componentsPart!: HTMLElement;
  /** legend 里承载机器名的 span（label || host）。 */
  private nameSpan!: HTMLElement;
  /** legend 左侧折叠指示符（▸ 折叠 / ▾ 展开）。 */
  private toggleIndicator!: HTMLElement;
  private collapsed = false;

  /**
   * S1：这张卡对应的记录**在盘上当前的 origin**。`null` = 还没落过盘（新增的卡）。
   *
   * 为什么需要它：机器的定位键是 origin（`label || host`），而 origin **可以被用户
   * 编辑**。整表覆盖时这问题被掩盖着（反正全写）；改成局部合并后，「这张卡对应盘上
   * 哪一条」必须有确定答案，否则改个名就会变成「新增一台 + 留下一条孤儿」。
   */
  persistedKey: string | null;

  constructor(
    initial: RemoteHostConfig,
    private hooks: MachineCardHooks,
    collapsed = false,
    persistedKey: string | null = null,
  ) {
    this.persistedKey = persistedKey;
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
      resumeCommand: this.resumeCmdInput.value.trim(),
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
    removeBtn.className =
      "settings-btn settings-btn-secondary remote-machine-remove";
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

    // body：折叠时整体隐藏（legend 始终在）。
    this.body = document.createElement("div");
    this.body.className = "remote-machine-body";
    card.appendChild(this.body);

    // ★ S4b-3b-2：body 内部再分成**两块**，供机器详情页拆成「连接 / 组件」两栏
    //（主计划 §2.3 / §2.4）。分界就在 resume 命令那一行：
    //   连接 = 怎么连上这台机（host/port/user/密钥/指纹/地址/跳板/daemonless…）
    //   组件 = 这台机上装了什么、怎么起（resume 命令 + 装卸 daemon/ccm + 测试）
    //
    // **顺带把 S4b-3a 摆错的位置纠正了**：那轮我把 resume 命令插在 daemonless 之后，
    // commit 里却说它「放在装/卸 ccm 按钮紧邻处」—— 实际隔着 installInfo 等约 120 行。
    // §5-1 要的正是这两者相邻（装完 ccm 就该顺手改 resume 命令），现在真的相邻了。
    this.connectionPart = document.createElement("div");
    this.connectionPart.className = "machine-part machine-part-connection";
    this.body.appendChild(this.connectionPart);
    this.componentsPart = document.createElement("div");
    this.componentsPart.className = "machine-part machine-part-components";
    this.body.appendChild(this.componentsPart);

    let body = this.connectionPart;

    const onChange = () => {
      this.updateLegend();
      this.hooks.onChange();
    };

    this.labelInput = buildTextRow(
      body,
      "名称 (label，可选)",
      "pi / nano（留空用主机名）",
      onChange,
    );
    this.hostInput = buildTextRow(
      body,
      "主机 (host)",
      "raspberrypi.local 或 192.168.1.10",
      onChange,
    );
    this.portInput = buildNumberRow(body, "端口 (port)", 22, onChange);
    // 占位符举**多个**例子，别只写一个 —— 只写 "pi" 会让人以为这里非填树莓派默认用户不可。
    this.userInput = buildTextRow(
      body,
      "用户 (user)",
      "如 ubuntu / pi / root",
      onChange,
    );
    this.daemonPathInput = buildTextRow(
      body,
      "daemon 路径 (daemonPath)",
      DAEMON_PATH_PLACEHOLDER,
      onChange,
    );
    const daemonHint = document.createElement("div");
    daemonHint.className = "settings-hint";
    daemonHint.textContent =
      "须为绝对路径（如 /home/<你的用户名>/.cc-monitor/bin/cc-monitor-remote）；SSH 直接 exec 不经 shell，`~` 不会被展开。";
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
    this.keyPathInput = buildTextRow(
      body,
      "私钥路径 (keyPath，可选)",
      "C:\\Users\\me\\.ssh\\id_ed25519",
      onChange,
    );
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
    resetFpBtn.title =
      "清除已固化的主机指纹，下次连接重新捕获（仅在你确知服务器合法换过 host key 时用）";
    resetFpBtn.addEventListener("click", () => this.onResetFingerprint());
    this.fingerprintInput.parentElement?.appendChild(resetFpBtn);
    const syncResetVisibility = (): void => {
      resetFpBtn.style.display = shouldShowResetFingerprint(
        this.fingerprintInput.value,
      )
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
    this.addressesInput.placeholder =
      "10.0.0.2\npi.example.com:2222\n[fe80::1]:22（首选地址填上方 host）";
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

    // ★ S4b-3（主计划 §5-1）：**这台机器**的 resume 启动命令。
    //
    // 刻意紧挨着下面的「装/卸 ccm」按钮：此前 resume 命令是全局单值、住在
    //「外观 → 行为」里，而装 ccm 是每台机器一个按钮 —— 两处隔着两个顶层组，
    // 于是「装完 ccm 却忘了改 resume 命令」是个**结构性陷阱**，不是用户粗心。
    // 空 = 沿用全局默认，所以没填过的机器行为一字不变。
    // ↓↓ 从这里起归「组件」栏 ↓↓
    body = this.componentsPart;

    this.resumeCmdInput = buildTextRow(
      body,
      "resume 命令（这台机器）",
      "留空 = 用全局默认",
      onChange,
    );
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
      mkBtn(
        "文件",
        "",
        "打开 SFTP 文件面板（浏览 / 上传 / 下载 / 管理远端文件）",
        () => {
          const cfg = this.collect();
          if (!cfg.host || !cfg.user) {
            this.renderTestResult(
              null,
              "请先填好 host / user 再打开文件面板。",
            );
            return;
          }
          openSftpPanel(cfg);
        },
      ),
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
    const host =
      this.hostInput.value.trim() || this.labelInput.value.trim() || "该主机";
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
    line.textContent =
      "已重置为 TOFU：下次连接将重新捕获主机指纹（记得测试连接后重新固化）。";
    this.testResult.appendChild(line);
  }

  /** legend 显示 label || host || 占位。 */
  private updateLegend(): void {
    this.nameSpan.textContent = this.displayName();
    this.renderStatusStrip();
  }

  /**
   * S4b：列表那一行的状态条要跟着动作结果刷新。卡片自己不再渲染状态
   *（状态是列表的一列，见 `RemoteSection.buildMachineRow` 的注释），
   * 所以这里只是把「该刷了」这件事转给宿主。
   */
  renderStatusStrip(): void {
    this.hooks.onStatusChanged?.(this);
  }

  /** S4b-3b-2：交出「连接 / 组件」两块，供宿主拆成两栏。 */
  parts(): { connection: HTMLElement; components: HTMLElement } {
    return { connection: this.connectionPart, components: this.componentsPart };
  }

  /** 这张卡在列表/导航上显示的名字。 */
  displayName(): string {
    return (
      this.labelInput.value.trim() ||
      this.hostInput.value.trim() ||
      "（未命名机器）"
    );
  }

  /**
   * S4b：进入「独占一页」形态 —— 去掉折叠（一页只有它，没有可折的必要）
   * 与删除按钮（删除入口在列表行上，那里才看得见「删的是哪一台」）。
   */
  setPageMode(): void {
    this.setCollapsed(false);
    this.toggleIndicator.remove();
    this.element
      .querySelector(".remote-machine-remove")
      ?.remove();
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
      const res = await commands.test_remote_connection({
        cfg,
        onStage,
      });
      this.renderTestResult(res, null, stageLog);
      // S3：记进账本 —— 列表行上那个「✓ 3 分钟前」就是这一次的结论。
      // 一次测试同时给出两格：`sshOk`（连得上吗）与 `daemonOk`（daemon 回 hello 了吗）。
      // **只在 SSH 通了的时候才记 daemon** —— SSH 都没通，daemon 那格是「不知道」，
      // 记成 `fail` 等于替用户断言「远端没装 daemon」，而事实可能只是网络不通。
      this.recordFacet("connection", { kind: res.sshOk ? "ok" : "fail" });
      if (res.sshOk) {
        this.recordFacet("daemon", {
          kind: res.daemonOk ? "ok" : "fail",
          detail: res.daemonOk ? "在跑" : "没响应",
        });
      }
    } catch (e) {
      console.warn("test_remote_connection failed:", e);
      this.renderTestResult(null, `测试失败：${String(e)}`, stageLog);
      this.recordFacet("connection", { kind: "fail", detail: "连不上" });
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
      const msg = await commands.install_remote_ccm_helper({
        cfg,
        profile: ".bashrc",
      });
      // F08：安装成功后立即失效探测缓存——免得用户装完还要等最多 5 分钟 TTL 才切到 CLI
      // 渲染器（`ccm-probe.ts` 早就为这一步预留了 `invalidateCcmProbeCache`，只是从未被调用）。
      invalidateCcmProbeCache(cfg.label);
      this.testResult.textContent = `✓ ${msg}`;
      this.recordFacet("ccm", { kind: "ok", detail: "已装" });
    } catch (e) {
      this.testResult.textContent = `✗ 安装失败：${String(e)}`;
      this.recordFacet("ccm", { kind: "fail", detail: "装失败" });
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
      const r = await commands.push_public_key({ cfg, pubKeyPath });
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

    const mkField = (
      labelText: string,
      placeholder: string,
    ): HTMLInputElement => {
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
    const cwdInput = mkField(
      "工作目录",
      "远端绝对路径，如 /home/<你的用户名>/proj（留空=登录默认目录）",
    );
    const nameInput = mkField("tmux 会话名", "留空则按工作目录名自动生成");
    const cmdInput = mkField(
      "启动命令",
      "claude（可自定义，如 claude --model opus）",
    );
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
        none.textContent =
          "不指定（用远端登录的基座账号，不注入 CLAUDE_CONFIG_DIR）";
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
      void withAccount(origin, accName || null, (mods) =>
        runRemoteLauncher(origin, cwd, name, command, mods),
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
    /**
     * S3：这次动作的结论记到列表行的哪个格子上。**在这里统一接线**而不是各处 handler
     * 里散着写——散着写的失效模式是「新加一个动作忘了记」，那台机器的那一格就永远
     * 停在旧结论上，而 UI 上看不出来。
     */
    ledger?: { facet: MachineFacet; ok: string; fail: string },
  ): Promise<void> {
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = `${busyLabel}…`;
    this.testResult.style.display = "block";
    this.testResult.textContent = `${busyLabel}…`;
    try {
      const msg = await fn();
      this.testResult.textContent = `✓ ${msg}`;
      if (ledger) this.recordFacet(ledger.facet, { kind: "ok", detail: ledger.ok });
    } catch (e) {
      this.testResult.textContent = `✗ ${String(e)}`;
      if (ledger)
        this.recordFacet(ledger.facet, { kind: "fail", detail: ledger.fail });
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
    }
  }

  /** S3：记一格状态并立刻重绘状态条（同一个 key 口径：盘上那条的 origin）。 */
  private recordFacet(
    facet: MachineFacet,
    state: { kind: "ok" | "fail"; detail?: string },
  ): void {
    recordFacet(this.persistedKey ?? hostKey(this.collect()), facet, state);
    this.renderStatusStrip();
  }

  /** F08c：点「安装 daemon」——把内嵌 daemon 按远端架构装到 daemonPath。 */
  private async onDeployDaemon(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user || !cfg.daemonPath) {
      this.showResultText("请先填好 host / user / daemonPath 再安装 daemon。");
      return;
    }
    await this.runRemoteAction(
      this.daemonInstallButton,
      "安装 daemon 中",
      () => commands.deploy_remote_daemon({ cfg }),
      { facet: "daemon", ok: "已装", fail: "装失败" },
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
    await this.runRemoteAction(
      this.daemonUninstallButton,
      "卸载 daemon 中",
      () => commands.uninstall_remote_daemon({ cfg }),
      // 卸载**成功**意味着这台机器现在没有 daemon —— 结论是 `fail`（缺组件），不是 `ok`。
      // 这里刻意不用 ledger 参数：它把「动作成功」映射成 `ok`，而本例正好相反。
    );
    this.recordFacet("daemon", { kind: "fail", detail: "已卸载" });
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
      commands.uninstall_remote_ccm_helper({
        cfg,
        profile: ".bashrc",
      }),
    );
    // 同 daemon 卸载：动作成功 = 组件不在了。
    this.recordFacet("ccm", { kind: "fail", detail: "已卸载" });
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
        saveBtn.textContent = current
          ? "更新为该指纹（严格校验）"
          : "保存为严格校验";
        const fp = res.fingerprint;
        saveBtn.addEventListener(
          "click",
          () => void this.onSaveFingerprint(fp),
        );
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
