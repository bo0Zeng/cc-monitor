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
 * 2. `parts()` —— 交出「连接 / 组件」两块，详情页据此分栏（S4b-3b-2）；〔ST2〕外加「工具」栏那一块（别名）。
 * 3. `setPageMode()` —— 进入独占一页的形态（去折叠箭头与删除按钮）。
 */
import { listen } from "@tauri-apps/api/event";
import { commands } from "../ipc/commands";
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir, join } from "@tauri-apps/api/path";
import { openFileWindow } from "../file-window";
import { buildAliasManager } from "./machine-aliases"; // 〔AL2〕② 别名：远端卡与本机同一个组件（`origin` = 这台）
import { recordFacet, type MachineFacet } from "./machine-status";
import { hostKey, readRemoteConfig, resolveRemoteConfigByOrigin, type RemoteHostConfig } from "../remote-config";
import { parseAddressLines } from "../remote-config";
// E80：`ConnectStage` 直连生成物，不再绕道 `remote-section`（那条绕道是 import 环的一半）。
import type { ConnectStage } from "../generated/ConnectStage";
import { AGENT_PROFILE } from "../agent-profile";
// 〔FE1〕铸名口（列名单 ＋ 避让 ＋ 「列不出 ⇒ 不起」）本机远端同一个家。
import { mintFreshTmuxName, refuseUnmintable } from "../tmux-name-mint";
import { isSelectable, currentWorkingAccount } from "../accounts";
import { fetchAccounts } from "../account-reads";
import { withAccount } from "../launch-account";
import { runRemoteLauncher } from "../remote-launch-run";
import { probeMachine, ProbeStalled, type ConnTestResult, type ProbeStop } from "../remote-probe";
import { pushPublicKey } from "../pubkey-push";
import type { ResolvedHost } from "../ssh-config-reads";
import { askConfirm } from "../ask-dialog";
import { copyText } from "../copy-table";

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
  mark.textContent = ok ? copyText("machineCard.statusLine.ok") : copyText("machineCard.statusLine.fail");
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

/**
 * F46：阶段事件 → 泳道行的图标 + 文案。纯函数便于单测。
 *
 * **E80：从 `remote-section.ts` 搬来。** 它此前住在那边而唯一的消费者在这边，于是
 * 「从 remote-section 抽出去的 machine-card」回头 import 它的**值** ⇒ 一条真的运行期
 * import 环。搬到唯一消费者身边，环就没了。
 *
 * **`ConnectStage` 用生成物不是「顺手」**：下面有 `const _never: never = st` 穷尽性兜底，
 * 而**手写类型时 Rust 新增一个 variant 并不会让它红** —— 那条 `never` 会一直在守一个
 * TS 侧自己造的联合，不是 Rust 的真实形状。换成生成物它才真正对 Rust 的改动有牙。
 */
/** 〔MIG-1 收尾〕测试连接到点没等到结局时「停在哪一段」的那句话（最后收到的那一格；握手中那一段带上最后一行阶段）。 */
export function describeStop(stop: ProbeStop): string {
  switch (stop.at) {
    case "start":
      return copyText("machineCard.stall.start");
    case "handshake":
      return copyText("machineCard.stall.handshake", { step: describeStage(stop.last).text });
    case "hello":
      return copyText("machineCard.stall.hello");
    case "control":
      return copyText("machineCard.stall.control");
  }
}

export function describeStage(st: ConnectStage): {
  icon: string;
  text: string;
} {
  switch (st.kind) {
    case "dialing":
      return { icon: copyText("machineCard.stage.dialIcon"), text: copyText("machineCard.stage.dial", { endpoint: st.endpoint }) };
    case "hostKey":
      return { icon: copyText("machineCard.stage.fingerprintIcon"), text: copyText("machineCard.stage.fingerprint", { endpoint: st.endpoint, fingerprint: st.fingerprint }) };
    case "failed":
      return { icon: copyText("machineCard.stage.failIcon"), text: copyText("machineCard.stage.failed", { endpoint: st.endpoint, reason: st.reason }) };
    case "won":
      return { icon: copyText("machineCard.stage.okIcon"), text: copyText("machineCard.stage.won", { endpoint: st.endpoint }) };
    case "auth":
      return st.ok
        ? { icon: copyText("machineCard.stage.okIcon"), text: copyText("machineCard.stage.authOk") }
        : { icon: copyText("machineCard.stage.failIcon"), text: copyText("machineCard.stage.authFailed", { detail: st.detail ?? "" }) };
    case "established":
      return { icon: copyText("machineCard.stage.readyIcon"), text: copyText("machineCard.stage.ready") };
    default: {
      // F46 建议 E：穷尽性兜底——未来新增 ConnectStage 变体时编译期(never)即报错。
      const _never: never = st;
      return {
        icon: copyText("machineCard.stage.otherIcon"),
        text: String((_never as { kind?: string }).kind ?? ""),
      };
    }
  }
}

export interface MachineCardHooks {
  /** 任一字段变化 → 让 section 保存全部。 */
  onChange: () => void;
  /** 点删除 → 让 section 移除本卡片。 */
  onRemove: (card: MachineCard) => void;
  /** S4b：这张卡的状态/名字变了，宿主该刷新列表那一行。 */
  onStatusChanged?: (card: MachineCard) => void;
}
// 〔E2 · V28 · `设计/01 §6.7b`〕「后端路径」那一格删了：落点恒是那台的 `~/.cc-monitor/bin/ccm`（它就是后端本身），
//   从前按用户名预填的 `defaultBackendPathFor`〔散文墓碑〕随之删。
/**
 * F43：是否显示「重置为 TOFU」按钮——当且仅当当前已固化了非空指纹。
 * 抽成纯函数便于单测（trim 后非空 = 已固化严格校验）。
 */
export function shouldShowResetFingerprint(current: string): boolean {
  return current.trim().length > 0;
}

/** 〔ST2〕一张机器卡交给详情页的三块：连接 / 组件 / 工具（别名）。 */
export interface MachineCardParts {
  connection: HTMLElement;
  components: HTMLElement;
  tools: HTMLElement;
}

/** 按钮结果写在哪一栏。 */
type ResultArea = "conn" | "comp";

/** 〔VIS2 · `设计/15 §3.4 ①`〕后端那边自动固化 / 各地址指纹不一 ⇒ 既有的 `remote-health` 上这两个 kind（`dial_host.rs`）。 */
export const HOST_KEY_NOTICE_KINDS: readonly string[] = ["host_key_pinned", "host_key_differs"];
export interface HostKeyNotice {
  origin: string;
  kind: string;
  message: string;
}
const liveCards = new Set<MachineCard>();
let hostKeyNoticesBound = false;
/** 整个设置窗只订一次，按 origin 分给各张卡。 */
function bindHostKeyNotices(): void {
  if (hostKeyNoticesBound) return;
  hostKeyNoticesBound = true;
  listen<HostKeyNotice>("remote-health", (e) => {
    for (const c of liveCards) void c.onHostKeyNotice(e.payload);
  }).catch((e: unknown) => console.warn("订不上 remote-health（host key 告知）", e));
}

export class MachineCard {
  readonly element: HTMLElement;
  private legend!: HTMLElement;
  private labelInput!: HTMLInputElement;
  private hostInput!: HTMLInputElement;
  private portInput!: HTMLInputElement;
  private userInput!: HTMLInputElement;
  private keyPathInput!: HTMLInputElement;
  private fingerprintInput!: HTMLInputElement;
  private addressesInput!: HTMLTextAreaElement;
  private jumpInput!: HTMLInputElement;
  /** S4b-3（§5-1）：这台机器的 resume 启动命令（空 = 用全局默认）。 */
  private resumeCmdInput!: HTMLInputElement;
  /** 依当前指纹值显隐「重置为 TOFU」按钮（load / 重置后调用）。 */
  private syncResetFpVisibility!: () => void;
  private testButton!: HTMLButtonElement;
  private backendInstallButton!: HTMLButtonElement;
  private backendUninstallButton!: HTMLButtonElement;
  private testResult!: HTMLElement;
  /** 〔MC1〕「组件」栏那几个动作的结果区（「连接」栏的结果仍在 `testResult`）。 */
  private actionResult!: HTMLElement;
  /** 折叠时隐藏的字段 + 测试/安装区（legend 始终可见）。 */
  private body!: HTMLElement;
  /** S4b-3b-2：body 的两半 —— 详情页据此拆「连接 / 组件」两栏。 */
  private connectionPart!: HTMLElement;
  private componentsPart!: HTMLElement;
  /**
   * 〔第四波 ST2 · 协调方转主会话裁〕「工具」栏里的那一块：② 别名。
   * 原来远端的别名住「组件」栏、本机的住「工具 → 别名」—— 同一个动作两个位置（`71 §5`：每张卡上同一个动作）。
   * ⇒ 统一放「工具」栏：本机远端同一个位置。
   */
  private toolsPart!: HTMLElement;
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
    liveCards.add(this);
    bindHostKeyNotices();
  }

  /**
   * 〔VIS2〕只认自己那台：固化了 ⇒ 从盘上把指纹同步进输入框（机器页保存是整台 upsert，不同步会用空值盖回去）；
   * 各地址不一 ⇒ 把那句话（带逐地址指纹）说在结果区，让人在指纹那一栏选一个填上。
   */
  async onHostKeyNotice(n: HostKeyNotice): Promise<void> {
    if (!HOST_KEY_NOTICE_KINDS.includes(n.kind)) return;
    if (n.origin !== (this.persistedKey ?? hostKey(this.collect()))) return;
    if (n.kind === "host_key_pinned") {
      const disk = await resolveRemoteConfigByOrigin(n.origin);
      if (disk?.hostKeyFingerprint) {
        this.fingerprintInput.value = disk.hostKeyFingerprint;
        this.syncResetFpVisibility();
      }
    }
    this.testResult.style.display = "block";
    const line = document.createElement("div");
    line.className = `remote-test-line ${n.kind === "host_key_pinned" ? "remote-test-ok" : "remote-test-caution"}`;
    line.textContent = n.message;
    this.testResult.appendChild(line);
  }

  /** 读出本卡片的 RemoteHostConfig（trim；port 兜底 22）。 */
  collect(): RemoteHostConfig {
    return {
      label: this.labelInput.value.trim(),
      host: this.hostInput.value.trim(),
      port: parsePort(this.portInput.value),
      user: this.userInput.value.trim(),
      keyPath: this.keyPathInput.value.trim(),
      hostKeyFingerprint: this.fingerprintInput.value.trim(),
      addresses: parseAddressLines(this.addressesInput.value),
      jump: this.jumpInput.value.trim(),
      resumeCommand: this.resumeCmdInput.value.trim(),
    };
  }

  /** 导入别名时填充连接参数（host/port/user/keyPath + label=别名）。 */
  applyResolved(resolved: ResolvedHost, alias: string): void {
    if (!this.labelInput.value.trim()) this.labelInput.value = alias;
    this.hostInput.value = resolved.host;
    this.portInput.value = resolved.port ? String(resolved.port) : "22";
    this.userInput.value = resolved.user;
    this.keyPathInput.value = resolved.keyPath ?? "";
    if (resolved.proxyJump) this.jumpInput.value = resolved.proxyJump; // F57 S-2:单别名也填跳板
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
    this.toggleIndicator.textContent = copyText("machineCard.build.expandedIcon");
    legend.appendChild(this.toggleIndicator);

    // 机器名（label || host）—— 独立 span（不靠脆弱的 firstChild 文本节点）。flex:1 把删除推到右侧。
    this.nameSpan = document.createElement("span");
    this.nameSpan.className = "remote-machine-name";
    legend.appendChild(this.nameSpan);

    // 删除按钮（legend 右侧）
    const removeBtn = document.createElement("button");
    removeBtn.type = "button";
    removeBtn.className =
      "settings-btn remote-machine-remove";
    removeBtn.textContent = copyText("machineCard.build.delete");
    removeBtn.title = copyText("machineCard.build.deleteHint");
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
    //   连接 = 怎么连上这台机（host/port/user/密钥/指纹/地址/跳板…）
    //   组件 = 这台机上装了什么、怎么起（resume 命令 + ① 部署后端 + ② 别名；〔MC1〕测试连接回了连接栏）
    //
    // **顺带把 S4b-3a 摆错的位置纠正了**：那轮我把 resume 命令插在那个降级开关之后，
    // commit 里却说它「放在装/卸 ccm 按钮紧邻处」—— 实际隔着那段安装位置说明等约 120 行。
    // §5-1 要的正是这两者相邻（装完 ccm 就该顺手改 resume 命令），现在真的相邻了。
    this.connectionPart = document.createElement("div");
    this.connectionPart.className = "machine-part machine-part-connection";
    this.body.appendChild(this.connectionPart);
    this.componentsPart = document.createElement("div");
    this.componentsPart.className = "machine-part machine-part-components";
    this.body.appendChild(this.componentsPart);
    // 〔ST2〕第三块：「工具」栏（别名）。不挂类名：它只负责装东西，样式沿用里面那几行自己的类。
    this.toolsPart = document.createElement("div");
    this.toolsPart.dataset.machinePart = "tools";
    this.body.appendChild(this.toolsPart);

    let body = this.connectionPart;

    const onChange = () => {
      this.updateLegend();
      this.hooks.onChange();
    };

    this.labelInput = buildTextRow(
      body,
      copyText("machineCard.field.label"),
      copyText("machineCard.field.labelHint"),
      onChange,
    );
    this.hostInput = buildTextRow(
      body,
      copyText("machineCard.field.host"),
      copyText("machineCard.field.hostHint"),
      onChange,
    );
    this.portInput = buildNumberRow(body, copyText("machineCard.field.port"), 22, onChange);
    // 占位符举**多个**例子，别只写一个 —— 只写 "pi" 会让人以为这里非填树莓派默认用户不可。
    this.userInput = buildTextRow(
      body,
      copyText("machineCard.field.user"),
      copyText("machineCard.field.userHint"),
      onChange,
    );
    this.keyPathInput = buildTextRow(
      body,
      copyText("machineCard.field.keyPath"),
      "C:\\Users\\me\\.ssh\\id_ed25519",
      onChange,
    );
    this.fingerprintInput = buildTextRow(
      body,
      copyText("machineCard.field.fingerprint"),
      copyText("machineCard.field.fingerprintHint"),
      onChange,
    );
    // F43：重置指纹入口——补 aterm 自曝的坑（服务器合法换 host key 后严格校验会永久
    // 拒连，此前只能手动清空输入框、无风险告知）。仅在已固化指纹时显示。
    const resetFpBtn = document.createElement("button");
    resetFpBtn.type = "button";
    resetFpBtn.className = "settings-btn";
    resetFpBtn.textContent = copyText("machineCard.field.resetFingerprint");
    resetFpBtn.title =
      copyText("machineCard.field.resetFingerprintHint");
    resetFpBtn.addEventListener("click", () => void this.onResetFingerprint());
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
    addrLabel.textContent = copyText("machineCard.field.addresses");
    addrRow.appendChild(addrLabel);
    this.addressesInput = document.createElement("textarea");
    this.addressesInput.className = "settings-input settings-input-wide";
    this.addressesInput.rows = 2;
    this.addressesInput.spellcheck = false;
    this.addressesInput.placeholder =
      copyText("machineCard.field.addressesHint");
    this.addressesInput.addEventListener("change", onChange);
    addrRow.appendChild(this.addressesInput);
    body.appendChild(addrRow);

    // F56：跳板 ProxyJump——填另一台已配置主机的 label（空=直连）。经该跳板机隧道连本机。
    this.jumpInput = buildTextRow(
      body,
      copyText("machineCard.field.jump"),
      copyText("machineCard.field.jumpHint"),
      onChange,
    );

    // 🔴 `K-R59`（09-11，定框 `K35`）：**这里原来是那个 `daemonless` 降级开关**
    //    （checkbox 逐字「daemonless 降级读取（无需后端）」→ `RemoteHostConfig.daemonless`）。
    //    `K35` 逐字：「不要有 daemonless。没有没有后端的情况。前端应该就是去调用远程后端的。」
    //    ⇒ 整格删掉：字段 · 顶层二选一 · 轮询段 · 这一格界面 · 那条本机豁免，五处一起走。
    //    用户盘上那份旧 `true` 由 `remote-config.ts` 的 `LEGACY_NO_BACKEND_KEY` 认出来，
    //    在「还差什么」清单上指名告知（`NO_BACKEND_GAP_CODE`），不静默吞掉。

    // 〔MC1 · 2026-09-24〕`设计/71 §13` · `设计/01 §6.7`：**机器卡上只有三个动作** ——
    //   ① 部署后端 · ② 别名 · ③ 后端代管的资产。从前这里是**一行 8 颗按钮**全挤在「组件」栏：
    //   测试连接 · 文件 · 推送公钥 · 开新 Claude · 安装 backend · 卸载 backend · 装 ccm 启动器 · 卸载 ccm。
    //   ⇒ 按「它动的是什么」分回去：
    //   · 前四颗动的是**这条连接**（验它 · 免密 · 用它看文件 / 起会话）⇒ 回「连接」栏，挨着它们用的那几格；
    //   · 后四颗是两件事的四个开关（装后端 · 「ccm 助手」），而「ccm 助手」自己又是两件事
    //     （`71 §13.1`：① 推入口 ② 写别名块）⇒ 推入口并进 ①（**一颗按钮**），写别名块归 ②；
    //   · ③ 不是按钮：skill / MCP / 插件 / 账号在这一页的「账号」「工具」两栏。
    //   「ccm 助手 / ccm 启动器」这个词整个删掉（用户 2026-09-17 逐字「装/卸 ccm 助手是假的」）。
    const mkBtn = (
      label: string,
      variant: string,
      title: string,
      onClick: () => void,
    ): HTMLButtonElement => {
      const b = document.createElement("button");
      b.type = "button";
      // 〔W5-AUX · AR1 拍板 3〕`variant` 空串 = 默认那一种（原先的 `settings-btn-secondary` 从没有过规则，已摘）。
      b.className = variant ? `settings-btn ${variant}` : "settings-btn";
      b.textContent = label;
      b.title = title;
      b.addEventListener("click", onClick);
      return b;
    };

    // ── 连接栏的动作：测试 · 推公钥 · 文件 · 开新 Claude ──
    const connRow = document.createElement("div");
    connRow.className = "settings-row settings-row-actions";
    this.testButton = mkBtn(
      copyText("machineCard.build.test"),
      "settings-btn-primary",
      copyText("machineCard.build.testHint"),
      () => void this.onTestConnection(),
    );
    connRow.appendChild(this.testButton);
    // F50：一键把本地公钥推到远端 authorized_keys（onboarding 免密）。
    const pushKeyBtn = mkBtn(
      copyText("machineCard.build.pushKey"),
      "",
      copyText("machineCard.build.pushKeyHint"),
      () => void this.onPushPubkey(pushKeyBtn),
    );
    connRow.appendChild(pushKeyBtn);
    // F48：在原生文件窗口里打开这一台（〔F7b〕老 SFTP 面板退役，终点换成 `file-window.ts`）。
    connRow.appendChild(
      mkBtn(
        copyText("machineCard.build.files"),
        "",
        copyText("machineCard.build.filesHint"),
        () => {
          const cfg = this.collect();
          if (!cfg.host || !cfg.user) {
            this.renderTestResult(null, copyText("machineCard.build.filesNeedHost"));
            return;
          }
          void openFileWindow(cfg);
        },
      ),
    );
    // F53：在这台机开新 Claude——填工作目录/tmux 名/命令，在远端 tmux 里启动全新会话。
    connRow.appendChild(
      mkBtn(
        copyText("machineCard.build.launch"),
        "",
        copyText("machineCard.build.launchHint"),
        () => this.openLauncherDialog(),
      ),
    );
    body.appendChild(connRow);
    this.testResult = document.createElement("div");
    this.testResult.className = "remote-test-result";
    this.testResult.style.display = "none";
    body.appendChild(this.testResult);

    // ↓↓ 从这里起归「组件」栏 ↓↓
    body = this.componentsPart;

    // ★ S4b-3（主计划 §5-1）：**这台机器**的 resume 启动命令。
    // 刻意挨着下面的动作：此前 resume 命令是全局单值、住在「外观 → 行为」里，而装东西是
    // 每台机器一个按钮 —— 两处隔着两个顶层组，「装完却忘了改 resume 命令」是个结构性陷阱。
    // 空 = 沿用全局默认，所以没填过的机器行为一字不变。
    this.resumeCmdInput = buildTextRow(
      body,
      copyText("machineCard.field.resumeCmd"),
      copyText("machineCard.field.resumeCmdHint"),
      onChange,
    );

    // ── ① 部署后端 ──
    const deployTitle = document.createElement("div");
    deployTitle.className = "settings-label";
    deployTitle.textContent = copyText("machineCard.deploy.title");
    body.appendChild(deployTitle);
    const deployHint = document.createElement("div");
    deployHint.className = "settings-hint remote-install-info";
    deployHint.textContent =
      copyText("machineCard.deploy.intro");
    body.appendChild(deployHint);
    const deployRow = document.createElement("div");
    deployRow.className = "settings-row settings-row-actions";
    this.backendInstallButton = mkBtn(
      copyText("machineCard.deploy.install"),
      "settings-btn-primary",
      // K-W4 §0c：跳过的条件是两条（版本已是最新、且落点那个文件在），两个事实各自说话。
      copyText("machineCard.deploy.installHint"),
      () => void this.onDeployBackend(),
    );
    deployRow.appendChild(this.backendInstallButton);
    this.backendUninstallButton = mkBtn(
      copyText("machineCard.deploy.uninstall"),
      "",
      copyText("machineCard.deploy.uninstallHint"),
      () => void this.onUninstallBackend(),
    );
    deployRow.appendChild(this.backendUninstallButton);
    body.appendChild(deployRow);

    this.actionResult = document.createElement("div");
    this.actionResult.className = "remote-test-result";
    this.actionResult.style.display = "none";
    body.appendChild(this.actionResult);

    // ↓↓ 从这里起归「工具」栏 ↓↓〔ST2：原来在「组件」栏，与本机那一格不在同一个位置〕
    body = this.toolsPart;

    // ── ② 别名 ──〔AL2 · 第四波 4D〕与本机同一个组件（`设计/71 §5`）：清单在这台读、在这台写，别名块装 / 卸 / 预览都在里面。
    const aliasTitle = document.createElement("div");
    aliasTitle.className = "settings-label";
    aliasTitle.textContent = copyText("machineCard.aliases.title");
    body.appendChild(aliasTitle);
    const origin = (): string => this.persistedKey ?? hostKey(this.collect());
    body.appendChild(
      buildAliasManager({
        // 远端恒 POSIX：`设计/01 §6.7b` 表 B 只承诺远端 Linux（`第四波记录/W5-ALIAS.md §2.2`）。
        platform: "posix",
        origin,
        loadAccounts: async () => {
          const st = await fetchAccounts(origin());
          if (!st.available) throw new Error(st.error ?? "");
          return st.accounts.map((a) => a.name);
        },
        // 机器列表那一格（`ccm`）照旧记装 / 卸的结论。〔MIG-2〕原先装完还清一次界面的 ccm 探针缓存：渲染进了那台后端、
        //   能力问它自己，界面不再缓存那一份（探针与缓存一起删了）。
        onBlockDone: (verb, error) => {
          if (verb === "install") {
            this.recordFacet("ccm", error
              ? { kind: "fail", detail: copyText("machineCard.status.installFailed") }
              : { kind: "ok", detail: copyText("machineCard.status.installed") });
          } else if (!error) {
            this.recordFacet("ccm", { kind: "fail", detail: copyText("machineCard.status.uninstalled") });
          }
        },
      }),
    );

    return card;
  }

  /** 折叠/展开本卡片：折叠时只剩 legend（机器名行），隐藏全部字段 + 测试/安装。 */
  private setCollapsed(next: boolean): void {
    this.collapsed = next;
    this.element.classList.toggle("is-collapsed", next);
    this.toggleIndicator.textContent = next ? copyText("machineCard.collapsed.icon") : copyText("machineCard.build.expandedIcon");
    this.legend.setAttribute("aria-expanded", next ? "false" : "true");
  }

  private syncInputs(cfg: RemoteHostConfig): void {
    this.labelInput.value = cfg.label;
    this.hostInput.value = cfg.host;
    this.portInput.value = cfg.port ? String(cfg.port) : "";
    this.userInput.value = cfg.user;
    this.keyPathInput.value = cfg.keyPath;
    this.fingerprintInput.value = cfg.hostKeyFingerprint;
    this.addressesInput.value = cfg.addresses.join("\n");
    this.jumpInput.value = cfg.jump ?? "";
    this.syncResetFpVisibility();
  }

  /**
   * F43：重置主机指纹 → 回到 TOFU（清空固化指纹）。LOUD 二次确认——清除后下次连接会
   * 接受新主机密钥;若此刻正被中间人攻击,会信任攻击者的密钥。仅当确知服务器合法换过
   * host key 时才该重置。
   */
  private async onResetFingerprint(): Promise<void> {
    const host =
      this.hostInput.value.trim() || this.labelInput.value.trim() || copyText("machineCard.resetFingerprint.thisHost");
    if (
      !(await askConfirm(
        copyText("machineCard.resetFingerprint.confirm", { host }),
      ))
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
      copyText("machineCard.resetFingerprint.done");
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

  /** S4b-3b-2：交出「连接 / 组件」两块，供宿主拆成两栏。〔ST2〕外加「工具」栏那一块（别名）。 */
  parts(): MachineCardParts {
    return { connection: this.connectionPart, components: this.componentsPart, tools: this.toolsPart };
  }

  /** 结果区：哪一栏的按钮，结果就写在哪一栏里。 */
  private resultArea(where: ResultArea): HTMLElement {
    if (where === "comp") return this.actionResult;
    return this.testResult;
  }

  /** 这张卡在列表/导航上显示的名字。 */
  displayName(): string {
    return (
      this.labelInput.value.trim() ||
      this.hostInput.value.trim() ||
      copyText("machineCard.displayName.unnamed")
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

  /** 点「测试连接」：组本卡片 → 本机后端 `remote-probe`（〔MIG-1 续〕原 Tauri 命令 `test_remote_connection`）→ 渲染结果。 */
  private async onTestConnection(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.renderTestResult(null, copyText("machineCard.test.needFields"));
      return;
    }
    this.testButton.disabled = true;
    const prevLabel = this.testButton.textContent;
    this.testButton.textContent = copyText("machineCard.test.running");
    // F46：连接分阶段事件泳道——测试开始即清空日志区。〔MIG-1 收尾〕本机后端边拨边推（进度流 `probe-progress/<票>`），收一行画一行。
    this.testResult.innerHTML = "";
    this.testResult.style.display = "block";
    const stageLog = document.createElement("div");
    stageLog.className = "remote-stage-log";
    this.testResult.appendChild(stageLog);
    try {
      // 〔MIG-1 续 · ⑬〕表单里这一台（可能没保存）＋ 已保存的那几台（同名那一份的指纹 · 跳板）交给本机后端，它组请求、拨一次。
      const res = await probeMachine(cfg, (await readRemoteConfig()).hosts, (st) => this.appendStageLine(stageLog, st));
      this.renderTestResult(res, null, stageLog);
      // S3：记进账本 —— 列表行上那个「✓ 3 分钟前」就是这一次的结论。
      // 一次测试同时给出两格：`sshOk`（连得上吗）与 `backendOk`（backend 回 hello 了吗）。
      // **只在 SSH 通了的时候才记 backend** —— SSH 都没通，backend 那格是「不知道」，
      // 记成 `fail` 等于替用户断言「远端没装后端」，而事实可能只是网络不通。
      this.recordFacet("connection", { kind: res.sshOk ? "ok" : "fail" });
      if (res.sshOk) {
        this.recordFacet("backend", {
          kind: res.backendOk ? "ok" : "fail",
          detail: res.backendOk ? copyText("machineCard.test.alive") : copyText("machineCard.test.silent"),
        });
      }
    } catch (e) {
      console.warn("remote-probe failed:", e);
      // 〔MIG-1 收尾〕到点没等到结局 ⇒ 说出停在哪一段（最后收到的那一格）。
      const said =
        e instanceof ProbeStalled
          ? copyText("machineCard.test.stalled", { said: e.message, where: describeStop(e.stop) })
          : copyText("machineCard.test.failed", { e: String(e) });
      this.renderTestResult(null, said, stageLog);
      this.recordFacet("connection", { kind: "fail", detail: copyText("machineCard.test.unreachable") });
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

  /** F50：一键推送本地公钥到远端 authorized_keys。已填私钥 → 取同名 .pub；否则弹框选 .pub。 */
  private async onPushPubkey(btn: HTMLButtonElement): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.showResultText(copyText("machineCard.pushKey.needHost"));
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
        title: copyText("machineCard.pushKey.dialogTitle"),
        multiple: false,
        directory: false,
        defaultPath,
        filters: [{ name: copyText("machineCard.pushKey.filterName"), extensions: ["pub"] }],
      });
      if (typeof picked !== "string") return; // 取消 / 多选保护
      pubKeyPath = picked;
    }
    await this.runRemoteAction(btn, copyText("machineCard.pushKey.pushing"), async () => {
      // 〔MIG-3b 续 · ⑬〕表单里这一台 ＋ 已保存的那几台交给本机后端：它读 `.pub`、组请求、经那台后端写或一次 exec。
      const r = await pushPublicKey(cfg, (await readRemoteConfig()).hosts, pubKeyPath);
      return r.outcome === "added"
        ? copyText("machineCard.pushKey.added", { path: r.pubPath })
        : copyText("machineCard.pushKey.already", { path: r.pubPath });
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
      this.showResultText(copyText("machineCard.launch.needHost"));
      return;
    }
    const origin = cfg.label.trim() || cfg.host;

    const back = document.createElement("div");
    back.className = "launcher-back";
    const box = document.createElement("div");
    box.className = "launcher-box";
    const title = document.createElement("div");
    title.className = "launcher-title";
    title.textContent = copyText("machineCard.launch.title", { machine: origin });
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
      copyText("machineCard.launch.cwd"),
      copyText("machineCard.launch.cwdHint"),
    );
    const nameInput = mkField(copyText("machineCard.launch.tmuxName"), copyText("machineCard.launch.tmuxNameHint"));
    const cmdInput = mkField(
      copyText("machineCard.launch.command"),
      copyText("machineCard.launch.commandHint"),
    );
    // 工作目录填定 → 预览留空时将用的名字(placeholder)。〔FIX4 · J7〕名字问那台后端铸（与点「开始」时同一问）；
    //   问不到 / 目录又改了 ⇒ 退回「自动生成」那一句。按 `change`（填完离开）问、不按每个键问。
    cwdInput.addEventListener("change", () => {
      const cwd = cwdInput.value.trim();
      nameInput.placeholder = copyText("machineCard.launch.tmuxNameAuto");
      if (!cwd) return;
      void mintFreshTmuxName(origin, cwd).then((m) => {
        if (m.ok && cwdInput.value.trim() === cwd) {
          nameInput.placeholder = copyText("machineCard.launch.tmuxNameDerived", { name: m.name });
        }
      });
    });

    // A4：账号下拉。异步填充——账号库不可用（旧 backend / 未启用）则整行不显 → 不注入
    // configDir → 行为与旧版逐字节一致（§7 降级）。选中某账号 = 起会话时注入其 CLAUDE_CONFIG_DIR。
    const acctRow = document.createElement("label");
    acctRow.className = "launcher-field";
    acctRow.style.display = "none";
    const acctSpan = document.createElement("span");
    acctSpan.textContent = copyText("machineCard.launch.account");
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
        //
        // ⚠ audit-0805 F12：这段注释一直是对的，**它下面那句给用户看的文案却是错的** ——
        // 原文写「用远端已登录的那个，不注入 CLAUDE_CONFIG_DIR」，两处都不准：
        //   ① 「不注入」**弱于事实**：CLI 路会发 `--base`，而 ccm 收到 `--base` 是
        //      **`unset CLAUDE_CONFIG_DIR`**（`shared/ccm:674` 送进 tmux 的载荷行 + `:709`
        //      会话级 env，两处都 unset）。远端 shell 里若有 `cc-acct-iso shellinit` 生成的
        //      `export CLAUDE_CONFIG_DIR=<某账号>`，「不注入」会继承它，「unset」则落回基座
        //      —— **两者落到的是不同的账号**。
        //   ② 「已登录的那个」**在主路径上是假的**：它落 `~/.claude`，而基座常常没凭据。
        // ⇒ 改成描述事实。判据 `account-base-semantics.vitest.ts` 钉住它不许说回去。
// ⚠ **不许写「基座」**：那是内部叫法，`settings/base-wording-guard.vitest.ts`（S8）明令禁止。
//   两条判据是互补的 —— S8 钉**词汇**（别用内部黑话），本轮这条钉**真伪**（别说假话）。
        // ⚠ 兜底渲染路（不发 `--base`）才是**真的不注入**（继承 rc / tmux server 的值）；
        //   文案按**主路径**（CLI 渲染，`ACCOUNT_DIMENSION.applies` 恒真 ⇒ 必发 flag）写。
        none.textContent =
          copyText("machineCard.launch.accountBase");
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
    cancel.textContent = copyText("machineCard.launch.cancel");
    cancel.addEventListener("click", () => back.remove());
    const start = document.createElement("button");
    start.type = "button";
    start.className = "settings-btn settings-btn-primary";
    start.textContent = copyText("machineCard.launch.start");
    start.addEventListener("click", () => {
      void (async () => {
      const cwd = cwdInput.value.trim();
      // F13（用户 2026-08-03：「为什么会撞名? 要撞名检查」）：**默认名必须过铸名口。**
      // 同一个 cwd 点两次「开始」会派生出同名 ⇒ 撞上远端 `create-or-attach` 的幂等闸
      // ⇒ **静默接进第一个会话，而用户以为开了新的**（issue #76 那一族）。
      //
      // 用户显式填的名字**不动**（那是他的意思，撞了也是他要的复用）；
      // 只有**我们替他派生**的那个默认名才过铸名口。
      // 〔FE1〕铸名收进 `tmux-name-mint.ts`（与 `remote-launch-run.ts::runNewSessionRemote` 先前是逐字副本）；
      // 先前「列不出名单 ⇒ 空集铸名、不避让」正是 #76 的形状 ⇒ 列不出就不起、说清。
      const typed = nameInput.value.trim();
      let name = typed;
      if (!name) {
        const minted = await mintFreshTmuxName(origin, cwd);
        if (!minted.ok) {
          refuseUnmintable(origin, minted.why);
          return;
        }
        name = minted.name;
      }
      const command = cmdInput.value.trim() || AGENT_PROFILE.defaultLauncher;
      const accName = acctSelect.value; // "" = 不指定
      back.remove();
      // A4：新会话无 sid → 不记 lastAccount；withAccount 统一解析注入（〔FE1 · D-h〕选的号不可选 ⇒ 不起、说清、给显式选择）。
      await withAccount(origin, accName || null, (mods) =>
        runRemoteLauncher(origin, cwd, name, command, mods),
      );
      })();
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
  private showResultText(text: string, where: ResultArea = "conn"): void {
    const out = this.resultArea(where);
    out.style.display = "block";
    out.textContent = text;
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
    /** 〔MC1〕结果写到哪一栏：「连接」栏的动作写 `testResult`，「组件」栏的写 `actionResult`（〔AL2〕「工具」栏的别名那一块自带结果区）。 */
    where: ResultArea = "conn",
  ): Promise<void> {
    const out = this.resultArea(where);
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = `${busyLabel}…`;
    out.style.display = "block";
    out.textContent = `${busyLabel}…`;
    try {
      const msg = await fn();
      out.textContent = `✓ ${msg}`;
      if (ledger) this.recordFacet(ledger.facet, { kind: "ok", detail: ledger.ok });
    } catch (e) {
      out.textContent = `✗ ${String(e)}`;
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

  /** ①「部署后端」—— 后端本体 ＋ `ccm` 入口，一颗按钮、一次调用（〔MC1〕从前是两颗）。 */
  private async onDeployBackend(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.showResultText(copyText("machineCard.deploy.needFields"), "comp");
      return;
    }
    await this.runRemoteAction(
      this.backendInstallButton,
      copyText("machineCard.deploy.running"),
      () => commands.deploy_remote_backend({ cfg }),
      { facet: "backend", ok: copyText("machineCard.status.installed"), fail: copyText("machineCard.status.installFailed") },
      "comp",
    );
    // 〔MIG-2〕原先这里清界面的 ccm 探针缓存；渲染进了那台后端、能力问它自己，那份缓存删了。
  }

  /** F08c：点「卸载后端」——删远端后端二进制（二次确认；〔DP1〕旁挂的版本标记退役了，不再删它）。 */
  private async onUninstallBackend(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.showResultText(copyText("machineCard.uninstall.needFields"), "comp");
      return;
    }
    if (
      !(await askConfirm(
        copyText("machineCard.uninstall.confirm", { host: cfg.host }),
      ))
    ) {
      return;
    }
    await this.runRemoteAction(
      this.backendUninstallButton,
      copyText("machineCard.uninstall.running"),
      () => commands.uninstall_remote_backend({ cfg }),
      // 卸载**成功**意味着这台机器现在没有 backend —— 结论是 `fail`（缺组件），不是 `ok`。
      // 这里刻意不用 ledger 参数：它把「动作成功」映射成 `ok`，而本例正好相反。
      undefined,
      "comp",
    );
    this.recordFacet("backend", { kind: "fail", detail: copyText("machineCard.status.uninstalled") });
  }

  /** 渲染测试结果：SSH ✓/✗、指纹（+可固化）、backend ✓/✗（+hello）。
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
          ? copyText("machineCard.test.sshOk", { via: res.endpoint ? copyText("machineCard.test.via", { endpoint: res.endpoint }) : "" })
          : copyText("machineCard.test.sshFailed"),
      ),
    );

    if (res.fingerprint) {
      const fpLine = document.createElement("div");
      fpLine.className = "remote-test-line";
      const fpText = document.createElement("span");
      fpText.className = "remote-test-fp";
      fpText.textContent = copyText("machineCard.test.fingerprint", { fingerprint: res.fingerprint });
      fpLine.appendChild(fpText);

      const current = this.fingerprintInput.value.trim();
      if (current !== res.fingerprint) {
        const saveBtn = document.createElement("button");
        saveBtn.type = "button";
        saveBtn.className = "settings-btn";
        saveBtn.textContent = current
          ? copyText("machineCard.test.pinUpdate")
          : copyText("machineCard.test.pinSave");
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
            copyText("machineCard.test.unverified");
          fpLine.appendChild(caution);
        }
      } else {
        const ok = document.createElement("span");
        ok.className = "remote-test-ok";
        ok.textContent = copyText("machineCard.test.pinned");
        fpLine.appendChild(ok);
      }
      this.testResult.appendChild(fpLine);
    }

    const backendText = res.backendOk
      ? copyText("machineCard.test.backendOk", { hello: res.backendHello ? `（${res.backendHello}）` : "" })
      : copyText("machineCard.test.backendDown");
    this.testResult.appendChild(makeStatusLine(res.backendOk, backendText));

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
      ok.textContent = copyText("machineCard.test.pinned");
      fpLine.parentElement.appendChild(ok);
    }
  }
}
