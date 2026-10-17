/**
 * 单台远端机器的编辑卡片 —— 机器详情页的主体（一个类 = 一台机器）。
 *
 * 对外三个契约（都有测试钉着）：
 * 1. `persistedKey` —— 卡片身份（对应盘上哪一条）：origin 可被用户编辑，没有它改个名就成了「新增一台 ＋ 留下孤儿」；
 * 2. `parts()` —— 交出「连接 / 组件」两块（详情页据此分栏），外加「终端」栏那一块（别名）；
 * 3. `setPageMode()` —— 进入独占一页的形态（去折叠箭头与删除按钮）。
 */
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir, join } from "@tauri-apps/api/path";
import { openFileWindow } from "../file-window";
import { buildConfigPage } from "./config-page"; // 别名与配置文件：远端卡与本机同一个组件（`origin` = 这台）
import { forgetHostKeys, hostKey, readRemoteConfig, type RemoteHostConfig } from "../remote-config";
import { parseAddressLines } from "../remote-config";
import type { MachineFault } from "../generated/MachineFault";
import { askInterrupts, interruptRows } from "./interrupts";
import { uninstallBackend, updateBackend } from "../backend-deploy";
// `ConnectStage` 直接用生成物（绕道 `remote-section` 会成 import 环）。
import type { ConnectStage } from "../../../comms/ui/generated/ConnectStage";
// 起新会话：全产品一个框（机器卡 ⋯「新建会话…」开它，机器锁定）。
import { openNewSession } from "../new-session";
import { probeMachine, ProbeStalled, type ConnTestResult, type ProbeStop } from "../remote-probe";
import { pushPublicKey } from "../pubkey-push";
import { confirmDialog } from "../kit/dialog";
import { copyText } from "../copy-table";
import { fold, setFoldSummary } from "../kit/fold";
import { button, setButtonLabel } from "../kit/button";
import { icon } from "../kit/icon";
import { toggleSwitch } from "../kit/switch";
import { detailOf, failSaid, sayWithDetail } from "../kit/detail";

/**
 * 连接设置的一格：标签（上）· 框（中，右侧可挂一颗按钮）· 下面一行说明（出错时错误句换在同一行）。
 * change 触发 onChange。
 */
function connField(
  parent: HTMLElement,
  labelText: string,
  opts: { placeholder?: string; hint?: string; type?: "text" | "number"; onChange: () => void },
): HTMLInputElement {
  const wrap = document.createElement("div");
  wrap.className = "machine-conn-field";
  const label = document.createElement("label");
  label.className = "settings-label";
  label.textContent = labelText;
  const box = document.createElement("div");
  box.className = "machine-conn-box";
  const input = document.createElement("input");
  input.type = opts.type ?? "text";
  input.className = "settings-input settings-input-wide";
  if (opts.type === "number") {
    input.min = "1";
    input.max = "65535";
    input.step = "1";
  }
  if (opts.placeholder) input.placeholder = opts.placeholder;
  // 路径 / 主机名，不是自然语言
  input.spellcheck = false;
  input.autocomplete = "off";
  input.addEventListener("change", opts.onChange);
  const id = `machine-conn-${++connFieldSeq}`;
  input.id = id;
  label.htmlFor = id;
  box.appendChild(input);
  const note = document.createElement("div");
  note.className = "machine-conn-note";
  note.dataset.hint = opts.hint ?? "";
  note.textContent = opts.hint ?? "";
  wrap.append(label, box, note);
  parent.appendChild(wrap);
  return input;
}
let connFieldSeq = 0;

/** 解析端口字符串：空 ⇒ 22（占位就是它）；不是 1–65535 的整数 ⇒ `null`（不替用户兜底）。 */
/** 端口框里的字 → 数（空 ⇒ 22；不是数字 ⇒ 0）。在不在 1–65535 只由后端判（机器表试算口）。 */
function parsePort(raw: string): number {
  const t = raw.trim();
  if (t === "") return 22;
  return /^\d+$/.test(t) ? Number(t) : 0;
}

/** 一格输入下面那一行：有错 ⇒ 错误句换在说明的位置；没错 ⇒ 还原说明。 */
function showFieldError(input: HTMLInputElement, text: string | null): void {
  const note = input.closest(".machine-conn-field")?.querySelector<HTMLElement>(".machine-conn-note");
  if (!note) return;
  if (text === null) {
    delete note.dataset.error;
    note.textContent = note.dataset.hint ?? "";
    input.removeAttribute("aria-invalid");
    return;
  }
  note.dataset.error = "true";
  const t = document.createElement("span");
  t.textContent = text;
  note.replaceChildren(icon("error", "compact"), t);
  input.setAttribute("aria-invalid", "true");
}

/** `SHA256:Aa3x…Q0`：指纹太长时留头尾、中间省略（悬停给全串）。 */
export function shortFingerprint(fp: string): string {
  const m = /^(SHA256:)(.+)$/.exec(fp);
  if (!m || m[2].length <= 10) return fp;
  return `${m[1]}${m[2].slice(0, 4)}…${m[2].slice(-2)}`;
}

/** 连上那一刻的几段（测试连接那一行）：解析 · 连接 · 认证 · 就绪。 */
type ConnSeg = "resolve" | "connect" | "auth" | "ready";
const CONN_SEGS: readonly ConnSeg[] = ["resolve", "connect", "auth", "ready"];
const SEG_LABEL: Record<ConnSeg, () => string> = {
  resolve: () => copyText("machineCard.seg.resolve"),
  connect: () => copyText("machineCard.seg.connect"),
  auth: () => copyText("machineCard.seg.auth"),
  ready: () => copyText("machineCard.seg.ready"),
};

/**
 * 阶段事件 → 泳道行的图标 ＋ 文案。纯函数。
 * `ConnectStage` 用生成物：下面 `const _never: never = st` 的穷尽性兜底才真对 Rust 新增的 variant 有牙。
 */
/** 测试连接到点没等到结局时「停在哪一段」的那句话（最后收到的那一格；握手中那一段带上最后一行阶段）。 */
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

export function describeStage(st: ConnectStage): { text: string } {
  switch (st.kind) {
    case "dialing":
      return { text: copyText("machineCard.stage.dial", { endpoint: st.endpoint }) };
    case "hostKey":
      return { text: copyText("machineCard.stage.fingerprint", { endpoint: st.endpoint, fingerprint: st.fingerprint }) };
    case "failed":
      return { text: copyText("machineCard.stage.failed", { endpoint: st.endpoint, reason: st.reason }) };
    case "won":
      return { text: copyText("machineCard.stage.won", { endpoint: st.endpoint }) };
    case "auth":
      return st.ok ? { text: copyText("machineCard.stage.authOk") } : { text: copyText("machineCard.stage.authFailed", { said: st.detail ?? copyText("reason.io.unknown") }) };
    case "established":
      return { text: copyText("machineCard.stage.ready") };
    default: {
      // 穷尽性兜底：新增 ConnectStage 变体时编译期（never）报错。
      const _never: never = st;
      return { text: String((_never as { kind?: string }).kind ?? "") };
    }
  }
}

export interface MachineCardHooks {
  /** 任一字段变化 → 让 section 保存全部。 */
  onChange: () => void;
  /** 点删除 → 让 section 移除本卡片。 */
  onRemove: (card: MachineCard) => void;
  /** 这张卡的状态 / 名字变了，宿主该刷新列表那一行。 */
  onStatusChanged?: (card: MachineCard) => void;
  /** 列表里别的机器已经叫这个名字了没有（名字是机器的键，重名就存不进、也改不了删不了）。 */
  tryCells?: (card: MachineCard, next: RemoteHostConfig) => Promise<MachineFault | null>;
}
// 没有「后端路径」那一格：落点恒是那台的 `~/.cc-monitor/bin/ccm`（它就是后端本身）。
/** 是否显示［忘记…］：当且仅当库里记着这台的主机指纹。 */
export function shouldShowResetFingerprint(known: readonly string[]): boolean {
  return known.length > 0;
}

/** 一张机器卡交给详情页的三块：连接 / 组件 / 终端（别名）。 */
export interface MachineCardParts {
  connection: HTMLElement;
  /** 「这台上的 cc-monitor」里归这台连接配置的那几行（恢复命令 · 卸载的结果）。 */
  components: HTMLElement;
  terminal: HTMLElement;
  /** 「从 X 卸载…」那颗按钮（宿主摆在那一折底行右侧）。 */
  uninstall?: HTMLElement;
}

/** 按钮结果写在哪一栏。 */
type ResultArea = "conn" | "comp";

export class MachineCard {
  readonly element: HTMLElement;
  private legend!: HTMLElement;
  private labelInput!: HTMLInputElement;
  private hostInput!: HTMLInputElement;
  private portInput!: HTMLInputElement;
  private userInput!: HTMLInputElement;
  private keyPathInput!: HTMLInputElement;
  private addressesInput!: HTMLTextAreaElement;
  private jumpInput!: HTMLInputElement;
  /** 库里记着这台的主机指纹（`machines-read` 的 `hostKeys`；只读）。 */
  private hostKeys: string[] = [];
  /** 主机指纹那一行（与钥匙路径一起排成宽的那两格）。 */
  private fpRow!: HTMLElement;
  /** 名字 / 地址 / 端口最后一次被接受的值：输入框里不合法的那一格不存，交出去的是这一份。 */
  private accepted = { label: "", host: "", port: 22 };
  /** 连接这台（开关那一格的当前值）。 */
  private connect = true;
  private connectSwitch!: { root: HTMLLabelElement; set(on: boolean): void };
  /** 「更多：备用地址 · 跳板机」那一折（标题行右侧写现值）。 */
  private moreFold: HTMLElement | null = null;
  /** 依库里记着的指纹画那一行、显隐［忘记…］（读到 / 忘掉之后调用）。 */
  private syncResetFpVisibility!: () => void;
  private testButton!: HTMLButtonElement;
  private backendUninstallButton!: HTMLButtonElement;
  private testResult!: HTMLElement;
  /** 「组件」栏那几个动作的结果区（「连接」栏的结果仍在 `testResult`）。 */
  private actionResult!: HTMLElement;
  /** 折叠时隐藏的字段 + 测试/安装区（legend 始终可见）。 */
  private body!: HTMLElement;
  /** body 的两半 —— 详情页据此拆「连接 / 组件」两栏。 */
  private connectionPart!: HTMLElement;
  private componentsPart!: HTMLElement;
  /**
   * 「终端」栏里的那一块：② 别名（这台终端认识 `cc` / `cct` / 账号快捷命令）。本机远端同一个位置。
   */
  private terminalPart!: HTMLElement;
  /** legend 里承载机器名的 span（label || host）。 */
  private nameSpan!: HTMLElement;
  /** legend 左侧折叠指示符（▸ 折叠 / ▾ 展开）。 */
  private toggleIndicator!: HTMLElement;
  private collapsed = false;

  /**
   * 这张卡对应的记录在盘上当前的 origin；`null` = 还没落过盘（新增的卡）。
   * 机器的定位键是 origin（`label || host`），而它可以被用户编辑 ⇒ 局部合并要靠它认出盘上那一条，否则改个名就成了「新增 ＋ 孤儿」。
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

  /** 这张卡作废了（机器表重读 · 删掉且没撤销）：退出全窗那张名单，不再收告知、不再被名单拽着（不然每次重读机器表都留下一整批卡和它们的 DOM）。 */
  dispose(): void {}

  /** 库里记着这台的主机指纹（设置页读机器表时交来）。 */
  setHostKeys(fps: readonly string[]): void {
    this.hostKeys = [...fps];
    this.syncResetFpVisibility();
  }

  /** 读出本卡片的 RemoteHostConfig（trim）。名字 / 地址 / 端口取最后一次被接受的值（不合法的那一格不交）。 */
  collect(): RemoteHostConfig {
    return {
      label: this.accepted.label,
      host: this.accepted.host,
      port: this.accepted.port,
      user: this.userInput.value.trim(),
      keyPath: this.keyPathInput.value.trim(),
      addresses: parseAddressLines(this.addressesInput.value),
      jump: this.jumpInput.value.trim(),
      connect: this.connect,
    };
  }

  /** 拨「连接这台」：存盘（宿主随后当场对齐那条流）。 */
  setConnect(on: boolean): void {
    if (this.connect === on) return;
    this.connect = on;
    this.connectSwitch.set(on);
    this.hooks.onChange();
  }

  /** 把输入框里此刻的名字 / 地址 / 端口记成「被接受的」（初值与导入时用；用户改的走各自的校验）。 */
  private acceptInputs(): void {
    this.accepted = {
      label: this.labelInput.value.trim(),
      host: this.hostInput.value.trim(),
      port: parsePort(this.portInput.value),
    };
  }

  private build(): HTMLElement {
    const card = document.createElement("fieldset");
    card.className = "remote-machine";

    const legend = document.createElement("legend");
    legend.className = "remote-machine-legend";
    this.legend = legend;
    card.appendChild(legend);

    // 折叠指示符（右指折叠 · 转 90° 下指展开，同 kit 折叠块）。点 legend（非删除按钮）切换折叠。
    this.toggleIndicator = document.createElement("span");
    this.toggleIndicator.className = "remote-machine-toggle";
    this.toggleIndicator.appendChild(icon("caretRight", "compact"));
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

    // body 分两块，供机器详情页拆成「连接 / 组件」两栏：
    //   连接 = 怎么连上这台机（host / port / user / 密钥 / 指纹 / 地址 / 跳板 …，测试连接也在这栏）
    //   组件 = 这台机上装了什么、怎么起（resume 命令 ＋ ① 部署后端 ＋ ② 别名）；resume 命令紧挨着部署（装完顺手改）。
    this.connectionPart = document.createElement("div");
    this.connectionPart.className = "machine-part machine-part-connection";
    this.body.appendChild(this.connectionPart);
    this.componentsPart = document.createElement("div");
    this.componentsPart.className = "machine-part machine-part-components";
    this.body.appendChild(this.componentsPart);
    // 第三块：「终端」栏（别名）。不挂类名：它只负责装东西，样式沿用里面那几行自己的类。
    this.terminalPart = document.createElement("div");
    this.terminalPart.dataset.machinePart = "terminal";
    this.body.appendChild(this.terminalPart);

    let body = this.connectionPart;

    const onChange = () => {
      this.updateLegend();
      this.hooks.onChange();
    };

    // 认人的那几格（名字 · 地址 · 端口）改了先问后端那一道（与写口同一份规则）：不过 ⇒ 就地说、不存。
    let asking = 0;
    const tryCells = async (next: RemoteHostConfig): Promise<MachineFault | null> => {
      try {
        return (await this.hooks.tryCells?.(this, next)) ?? null;
      } catch {
        return null; // 问不到 ⇒ 不挡，写口自己会判
      }
    };
    // 名字（空着就用地址）是这台的键：与别台撞了 ⇒ 就地说、不存。
    const onNameChange = async (): Promise<void> => {
      const label = this.labelInput.value.trim();
      const host = this.hostInput.value.trim();
      const mine = ++asking;
      const fault = label || host ? await tryCells({ ...this.collect(), label, host }) : null;
      if (mine !== asking) return;
      if (fault?.code === "name_taken") {
        showFieldError(this.labelInput, fault.said);
        this.updateLegend();
        return;
      }
      showFieldError(this.labelInput, null);
      this.accepted.label = label;
      this.accepted.host = host;
      onChange();
    };
    this.labelInput = connField(body, copyText("machineForm.field.name"), {
      placeholder: copyText("machineCard.field.labelHint"),
      onChange: () => void onNameChange(),
    });
    this.hostInput = connField(body, copyText("machineForm.field.host"), {
      hint: copyText("machineCard.field.hostHint"),
      onChange: () => void onNameChange(),
    });
    // 端口不是 1–65535 ⇒ 就地说、不存（不悄悄存成 22）。
    const onPortChange = async (): Promise<void> => {
      const port = parsePort(this.portInput.value);
      const mine = ++asking;
      const fault = await tryCells({ ...this.collect(), port });
      if (mine !== asking) return;
      if (fault?.code === "port") {
        showFieldError(this.portInput, fault.said);
        return;
      }
      showFieldError(this.portInput, null);
      this.accepted.port = port;
      onChange();
    };
    this.userInput = connField(body, copyText("machineForm.field.user"), {
      placeholder: copyText("machineCard.field.userHint"),
      onChange,
    });
    this.portInput = connField(body, copyText("machineForm.field.port"), { type: "number", placeholder: "22", onChange: () => void onPortChange() });
    this.portInput.closest(".machine-conn-field")?.classList.add("machine-conn-port");
    this.keyPathInput = connField(body, copyText("machineForm.field.key"), {
      hint: copyText("addMachine.field.keyHelp"),
      onChange,
    });
    // 私钥框右侧［选…］：系统选文件框，默认落在 ~/.ssh。
    const pickKey = button({
      label: copyText("machineCard.field.keyPick"),
      onClick: () => void this.onPickKey(),
    });
    this.keyPathInput.parentElement?.appendChild(pickKey);

    // 主机指纹：只读一行（库里记着的那几枚）＋［忘记…］；值住拨号核心那份库，不随整台存。
    const fpRow = document.createElement("div");
    this.fpRow = fpRow;
    fpRow.className = "machine-conn-field machine-conn-fp";
    const fpLabel = document.createElement("span");
    fpLabel.className = "settings-label";
    fpLabel.textContent = copyText("machineCard.field.fingerprint");
    const fpLine = document.createElement("div");
    fpLine.className = "machine-conn-fp-line";
    const fpText = document.createElement("span");
    fpText.className = "machine-fp";
    const resetFpBtn = document.createElement("button");
    resetFpBtn.type = "button";
    resetFpBtn.className = "machine-conn-link";
    resetFpBtn.textContent = copyText("machineCard.field.resetFingerprint");
    resetFpBtn.title = copyText("machineCard.field.resetFingerprintHint");
    resetFpBtn.addEventListener("click", () => void this.onResetFingerprint());
    fpLine.append(fpText, resetFpBtn);
    fpRow.append(fpLabel, fpLine);
    body.appendChild(fpRow);
    const syncResetVisibility = (): void => {
      resetFpBtn.hidden = !shouldShowResetFingerprint(this.hostKeys);
      fpText.title = this.hostKeys.join("\n");
      fpText.textContent =
        this.hostKeys.length === 0
          ? copyText("machineCard.fp.none")
          : this.hostKeys.map(shortFingerprint).join(copyText("kit.text.sep"));
    };
    this.syncResetFpVisibility = syncResetVisibility;
    syncResetVisibility();

    // 备用地址（多行，每行一个 host / host:port / [IPv6]:port）：首选 host 字段、其余并发拨号，首个握手成功者胜。
    const addrRow = document.createElement("div");
    addrRow.className = "machine-conn-field";
    const addrLabel = document.createElement("span");
    addrLabel.className = "settings-label";
    addrLabel.textContent = copyText("machineForm.field.addresses");
    addrRow.appendChild(addrLabel);
    this.addressesInput = document.createElement("textarea");
    this.addressesInput.className = "settings-input settings-input-wide";
    this.addressesInput.rows = 2;
    this.addressesInput.spellcheck = false;
    this.addressesInput.placeholder =
      copyText("machineCard.field.addressesHint");
    this.addressesInput.addEventListener("change", () => {
      this.paintMoreSummary();
      onChange();
    });
    addrRow.appendChild(this.addressesInput);
    body.appendChild(addrRow);

    // 跳板 ProxyJump：填另一台已配置主机的 label（空 = 直连），经那台隧道连这台。
    this.jumpInput = connField(body, copyText("machineForm.field.jump"), {
      placeholder: copyText("machineCard.field.jumpHint"),
      onChange: () => {
        this.paintMoreSummary();
        onChange();
      },
    });

    // ── 连接设置的动作：测试连接 · 推送公钥（左对齐，在「连接这台」下面）──
    const connRow = document.createElement("div");
    connRow.className = "machine-conn-actions";
    this.testButton = button({
      label: copyText("machineCard.build.test"),
      hint: copyText("machineCard.build.testHint"),
      onClick: () => void this.onTestConnection(),
    });
    connRow.appendChild(this.testButton);
    // 一键把本地公钥推到远端 authorized_keys（免密）。
    const pushKeyBtn: HTMLButtonElement = button({
      label: copyText("machineCard.build.pushKey"),
      hint: copyText("machineCard.build.pushKeyHint"),
      onClick: () => void this.onPushPubkey(pushKeyBtn),
    });
    connRow.appendChild(pushKeyBtn);
    body.appendChild(connRow);
    this.testResult = document.createElement("div");
    this.testResult.className = "remote-test-result";
    this.testResult.style.display = "none";
    body.appendChild(this.testResult);
    this.layoutConnection(body);

    // ↓↓ 从这里起归「组件」栏 ↓↓
    body = this.componentsPart;

    // 从这台卸载（红字，摆在那一折底行右侧）；结果落在这一折里。
    this.backendUninstallButton = button({
      label: copyText("machineCard.uninstall.open", { machine: this.displayName() }),
      kind: "danger-text",
      size: "compact",
      hint: copyText("machineCard.deploy.uninstallHint"),
      onClick: () => void this.onUninstallBackend(),
    });

    this.actionResult = document.createElement("div");
    this.actionResult.className = "remote-test-result";
    this.actionResult.style.display = "none";
    body.appendChild(this.actionResult);

    // ↓↓ 从这里起归「终端」栏 ↓↓
    body = this.terminalPart;

    // ── 别名与配置文件 ──与本机同一个组件：清单在这台读、在这台写，别名块装 / 卸 / 预览、共用 MCP、扩展都在里面。
    const origin = (): string => this.persistedKey ?? hostKey(this.collect());
    body.appendChild(
      buildConfigPage({
        machine: () => this.displayName(),
        // 远端恒 POSIX：只承诺远端 Linux。
        platform: "posix",
        origin,
      }),
    );

    return card;
  }

  /**
   * 连接设置的排法：名称 · 地址 / 用户 · 端口 两列，私钥 · 主机指纹通栏，
   * 备用地址 · 跳板机收进「更多」（行右写现值），下面「连接这台」，再下面测试连接 · 推送公钥与结果。
   */
  private layoutConnection(body: HTMLElement): void {
    const fieldOf = (el: HTMLElement): HTMLElement => el.closest<HTMLElement>(".machine-conn-field") ?? el;
    const grid = document.createElement("div");
    grid.className = "machine-conn-grid";
    grid.append(fieldOf(this.labelInput), fieldOf(this.hostInput), fieldOf(this.userInput), fieldOf(this.portInput));
    const wide = [fieldOf(this.keyPathInput), this.fpRow];
    for (const w of wide) w.classList.add("machine-conn-wide");
    grid.append(...wide);
    const moreBody = document.createElement("div");
    moreBody.className = "machine-conn-more-body";
    moreBody.append(fieldOf(this.addressesInput), fieldOf(this.jumpInput));
    this.moreFold = fold({ title: copyText("machineCard.conn.more"), summary: "", open: false, body: moreBody });
    this.moreFold.classList.add("machine-conn-more");
    this.connectSwitch = toggleSwitch({
      label: copyText("machineCard.conn.connect"),
      help: copyText("machineCard.conn.connectHint"),
      on: this.connect,
      onChange: (on) => {
        this.connect = on;
        this.hooks.onChange();
      },
    });
    this.connectSwitch.root.classList.add("machine-conn-switch");
    body.prepend(grid, this.moreFold, this.connectSwitch.root);
  }

  /** 「更多」行右的现值：`跳板 X` ／ `跳板 —`，有备用地址再加 `备用地址 n`。 */
  private paintMoreSummary(): void {
    if (!this.moreFold) return;
    const jump = this.jumpInput.value.trim();
    const parts = [copyText("machineCard.conn.moreJump", { jump: jump || copyText("machineCard.conn.none") })];
    const n = parseAddressLines(this.addressesInput.value).length;
    if (n > 0) parts.push(copyText("machineCard.conn.moreAddrs", { n }));
    setFoldSummary(this.moreFold, parts.join(copyText("kit.text.sep")));
  }

  /** 私钥［选…］：系统选文件框（默认在 ~/.ssh），选了就填进去并存。 */
  private async onPickKey(): Promise<void> {
    let defaultPath: string | undefined;
    try {
      defaultPath = await join(await homeDir(), ".ssh");
    } catch {
      /* 拿不到 home ⇒ 用系统默认起点 */
    }
    const picked = await open({ title: copyText("machineCard.field.keyPickTitle"), multiple: false, directory: false, defaultPath });
    if (typeof picked !== "string") return;
    this.keyPathInput.value = picked;
    this.keyPathInput.dispatchEvent(new Event("change"));
  }

  /** 折叠/展开本卡片：折叠时只剩 legend（机器名行），隐藏全部字段 + 测试/安装。 */
  private setCollapsed(next: boolean): void {
    this.collapsed = next;
    this.element.classList.toggle("is-collapsed", next);
    this.legend.setAttribute("aria-expanded", next ? "false" : "true");
  }

  private syncInputs(cfg: RemoteHostConfig): void {
    this.labelInput.value = cfg.label;
    this.hostInput.value = cfg.host;
    this.portInput.value = cfg.port ? String(cfg.port) : "";
    this.userInput.value = cfg.user;
    this.keyPathInput.value = cfg.keyPath;
    this.addressesInput.value = cfg.addresses.join("\n");
    this.jumpInput.value = cfg.jump ?? "";
    this.connect = cfg.connect;
    this.connectSwitch.set(cfg.connect);
    this.acceptInputs();
    this.syncResetFpVisibility();
    this.paintMoreSummary();
  }

  /**
   * 忘掉这台的主机指纹（拨号核心那份库里删这台的行，`host-keys-forget`）。醒目的二次确认：忘掉后下次连接会接受新主机密钥，
   * 此刻若正被中间人攻击就信了攻击者的密钥 —— 只在确知服务器合法换过 host key 时才该忘掉。
   */
  private async onResetFingerprint(): Promise<void> {
    const host =
      this.hostInput.value.trim() || this.labelInput.value.trim() || copyText("machineCard.resetFingerprint.thisHost");
    if (
      !(await confirmDialog({
        title: copyText("machineCard.resetFingerprint.title", { host }),
        action: copyText("machineCard.resetFingerprint.action"),
        danger: true,
        body: copyText("machineCard.resetFingerprint.confirm"),
      }))
    ) {
      return;
    }
    let said: string;
    try {
      await forgetHostKeys(this.persistedKey ?? hostKey(this.collect()));
      this.setHostKeys([]);
      said = copyText("machineCard.resetFingerprint.done");
    } catch (e) {
      said = e instanceof Error ? e.message : String(e);
    }
    this.testResult.innerHTML = "";
    this.testResult.style.display = "block";
    const line = document.createElement("div");
    line.className = "remote-test-line remote-test-caution";
    line.textContent = said;
    this.testResult.appendChild(line);
  }

  /** legend 显示 label || host || 占位。 */
  private updateLegend(): void {
    const name = this.displayName();
    this.nameSpan.textContent = name;
    setButtonLabel(this.backendUninstallButton, copyText("machineCard.uninstall.open", { machine: name }));
    this.renderStatusStrip();
  }

  /** 列表那一行的状态条要跟着动作结果刷新：卡片自己不画状态（那是列表的一列），只把「该刷了」转给宿主。 */
  renderStatusStrip(): void {
    this.hooks.onStatusChanged?.(this);
  }

  /** 交出「连接 / 组件」两块，供宿主拆成两栏；外加「终端」栏那一块（别名）。 */
  parts(): MachineCardParts {
    return {
      connection: this.connectionPart,
      components: this.componentsPart,
      terminal: this.terminalPart,
      uninstall: this.backendUninstallButton,
    };
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

  /** 进入「独占一页」形态：去掉折叠（一页只有它）与删除按钮（删除入口在列表行上，那里才看得见删的是哪一台）。 */
  setPageMode(): void {
    this.setCollapsed(false);
    this.toggleIndicator.remove();
    this.element
      .querySelector(".remote-machine-remove")
      ?.remove();
  }

  /** 问题行［更新］：把这一版换上去（先问会打断什么，部署住 `backend-deploy.ts`，与主窗口 ↗ 浮层同一处）。 */
  async update(): Promise<void> {
    const cfg = this.collect();
    await updateBackend(cfg, this.persistedKey ?? hostKey(cfg), this.displayName());
  }

  /** 问题行［推送公钥…］。 */
  pushKey(): void {
    void this.onPushPubkey(this.testButton);
  }

  /** ⋯ →「新建会话…」：开起新会话框（机器锁定在这一台；没填地址 / 用户 ⇒ 在连接设置下面说一句）。 */
  openLauncher(): void {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.showResultText(copyText("machineCard.launch.needHost"));
      return;
    }
    void openNewSession({ origin: cfg.label.trim() || cfg.host, lockMachine: true });
  }

  /** ⋯ →「打开文件」：没填地址 / 用户 ⇒ 在连接设置下面说一句。 */
  openFiles(): void {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.renderTestResult(null, copyText("machineCard.build.filesNeedHost"));
      return;
    }
    void openFileWindow(cfg);
  }

  /** 点「测试连接」：组本卡片 → 本机后端 `remote-probe`→ 渲染结果。 */
  private async onTestConnection(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.renderTestResult(null, copyText("machineCard.test.needFields"));
      return;
    }
    this.testButton.disabled = true;
    setButtonLabel(this.testButton, copyText("machineCard.test.running"));
    // 本机后端边拨边推（进度流 `probe-progress/<票>`）：几段逐格亮起来。
    this.testResult.replaceChildren();
    this.testResult.style.display = "block";
    this.testResult.dataset.tested = "true";
    const segs = this.segLine();
    this.testResult.appendChild(segs.el);
    try {
      // 表单里这一台（可能没保存）＋ 已保存的那几台（同名那一份的指纹 · 跳板）交给本机后端，它组请求、拨一次。
      const res = await probeMachine(cfg, (await readRemoteConfig()).hosts, (st) => segs.stage(st));
      segs.end(res);
      this.renderTestResult(res, null, segs.el);
    } catch (e) {
      console.warn("remote-probe failed:", e);
      // 到点没等到结局 ⇒ 说出停在哪一段（最后收到的那一格）。
      const said =
        e instanceof ProbeStalled
          ? copyText("machineCard.test.stalled", { said: e.message, where: describeStop(e.stop) })
          : failSaid(copyText("machineCard.test.failed"), e);
      segs.stop();
      this.renderTestResult(null, said, segs.el, e instanceof ProbeStalled ? "" : detailOf(e));
    } finally {
      this.testButton.disabled = false;
      setButtonLabel(this.testButton, copyText("machineCard.build.test"));
    }
  }

  /**
   * 测试连接下面那一行：`✓ 解析 ✓ 连接 ✓ 认证 ✓ 就绪 38ms · 版本 · 功能完整`。
   * 按收到的阶段逐格亮；结局到了再定没亮的那几格（第一处没过的打 ✗，后面的留灰）。
   */
  private segLine(): {
    el: HTMLElement;
    stage(st: ConnectStage): void;
    end(res: ConnTestResult): void;
    stop(): void;
  } {
    const el = document.createElement("div");
    el.className = "machine-conn-segs";
    const state = new Map<ConnSeg, "ok" | "fail" | "wait">(CONN_SEGS.map((k) => [k, "wait"]));
    const cells = new Map<ConnSeg, HTMLElement>();
    for (const k of CONN_SEGS) {
      const c = document.createElement("span");
      c.className = "machine-conn-seg";
      cells.set(k, c);
      el.appendChild(c);
    }
    const tail = document.createElement("span");
    tail.className = "machine-conn-seg-tail";
    el.appendChild(tail);
    const paint = (): void => {
      for (const k of CONN_SEGS) {
        const c = cells.get(k)!;
        const st = state.get(k)!;
        c.dataset.state = st;
        const label = document.createElement("span");
        label.textContent = SEG_LABEL[k]();
        c.replaceChildren(...(st === "wait" ? [] : [icon(st === "ok" ? "check" : "failed", "compact")]), label);
      }
    };
    const ok = (...ks: ConnSeg[]): void => {
      for (const k of ks) state.set(k, "ok");
      paint();
    };
    /** 第一处还没过的那一格打 ✗。 */
    const failFirst = (): void => {
      const k = CONN_SEGS.find((x) => state.get(x) !== "ok");
      if (k) state.set(k, "fail");
      paint();
    };
    paint();
    return {
      el,
      stage(st) {
        if (st.kind === "hostKey" || st.kind === "won") ok("resolve", "connect");
        else if (st.kind === "auth" && st.ok) ok("resolve", "connect", "auth");
        else if (st.kind === "auth") {
          ok("resolve", "connect");
          state.set("auth", "fail");
          paint();
        }
      },
      end(res) {
        if (res.sshOk) ok("resolve", "connect", "auth");
        if (res.sshOk && res.backendOk) {
          ok("ready");
          tail.textContent = res.backendHello ?? "";
        } else if (![...state.values()].includes("fail")) failFirst();
      },
      stop() {
        if (![...state.values()].includes("fail")) failFirst();
      },
    };
  }

  /** 一键推送本地公钥到远端 authorized_keys。已填私钥 → 取同名 .pub；否则弹框选 .pub。 */
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
      // 表单里这一台 ＋ 已保存的那几台交给本机后端：它读 `.pub`、组请求、经那台后端写或一次 exec。
      const r = await pushPublicKey(cfg, (await readRemoteConfig()).hosts, pubKeyPath);
      return r.outcome === "added"
        ? copyText("machineCard.pushKey.added", { path: r.pubPath })
        : copyText("machineCard.pushKey.already", { path: r.pubPath });
    });
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
    /** 结果写到哪一栏：「连接」栏的动作写 `testResult`，「组件」栏的写 `actionResult`（「终端」栏的别名那一块自带结果区）。 */
    where: ResultArea = "conn",
  ): Promise<boolean> {
    const out = this.resultArea(where);
    btn.disabled = true;
    const prev = btn.textContent;
    btn.textContent = `${busyLabel}…`;
    out.style.display = "block";
    out.textContent = `${busyLabel}…`;
    try {
      const msg = await fn();
      out.replaceChildren(resultLine("remote-test-ok", msg));
      return true;
    } catch (e) {
      out.replaceChildren(resultLine("remote-test-err", String(e)));
      return false;
    } finally {
      btn.disabled = false;
      btn.textContent = prev;
    }
  }

  /** 点「卸载后端」—— 删远端后端二进制（二次确认）。 */
  private async onUninstallBackend(): Promise<void> {
    const cfg = this.collect();
    if (!cfg.host || !cfg.user) {
      this.showResultText(copyText("machineCard.uninstall.needFields"), "comp");
      return;
    }
    const rows = interruptRows(await askInterrupts(this.persistedKey ?? hostKey(cfg)), this.displayName(), "uninstall");
    if (
      !(await confirmDialog({
        title: copyText("machineCard.uninstall.title", { host: cfg.host }),
        action: copyText("machineCard.uninstall.action"),
        danger: true,
        body: copyText("machineCard.uninstall.confirm"),
        rows,
      }))
    ) {
      return;
    }
    await this.runRemoteAction(this.backendUninstallButton, copyText("machineCard.uninstall.running"), () => uninstallBackend(cfg), "comp");
  }

  /**
   * 测试结果下面那几行：结局一句（没成时）· 指纹没记下时那一枚 ＋［记录］· 做不到的那几类 ［查看］。
   * `keep` 传入时保留那一行逐段（清空其余旧结果）。
   */
  private renderTestResult(
    res: ConnTestResult | null,
    hardError: string | null,
    keep?: HTMLElement,
    hardDetail = "",
  ): void {
    for (const child of Array.from(this.testResult.children)) {
      if (child !== keep) child.remove();
    }
    this.testResult.style.display = "block";

    if (hardError !== null) {
      const line = document.createElement("div");
      line.className = "remote-test-line remote-test-err";
      sayWithDetail(line, hardError, hardDetail);
      this.testResult.appendChild(line);
      return;
    }
    if (res === null) return;

    if (!res.sshOk || !res.backendOk) {
      const msg = document.createElement("div");
      msg.className = "remote-test-line remote-test-msg";
      sayWithDetail(msg, res.message, res.detail ?? "");
      this.testResult.appendChild(msg);
    }

    // 这台说做不到的那几类：点开看全表（一类一句；码的人话与置灰那一句同一个家）。
    if (res.backendGaps.length > 0) {
      const gaps = document.createElement("details");
      gaps.className = "remote-test-line";
      const summary = document.createElement("summary");
      summary.textContent = copyText("machineCard.test.gapsSummary");
      gaps.appendChild(summary);
      for (const g of res.backendGaps) {
        const row = document.createElement("div");
        row.textContent = copyText("machineCard.test.gapRow", {
          reason: g.said,
          n: String(g.count),
        });
        gaps.appendChild(row);
      }
      this.testResult.appendChild(gaps);
    }
  }

  /** 已连着（会话在流）：那一行几段直接打 ✓，不要求先点「测试连接」。测过之后以测的为准。 */
  showLive(on: boolean): void {
    if (this.testResult.dataset.tested === "true") return;
    if (!on) {
      this.testResult.replaceChildren();
      this.testResult.style.display = "none";
      return;
    }
    const segs = this.segLine();
    segs.end({ sshOk: true, backendOk: true, backendHello: "", fingerprint: null, endpoint: null, backendGaps: [], message: "", detail: null });
    this.testResult.replaceChildren(segs.el);
    this.testResult.style.display = "block";
  }

  /**
   * 指纹变了：两枚并排（记下的 · 现在的 · 记于何时）。［信任新的指纹］⇒ 记下现在那一枚并存盘（重连跟着走）；
   * ［不连接］⇒ 什么都不写。`seen` 是那台这一次出示的那一枚（状态成品带来；没带 ⇒ 不给信任）。
   */
  async compareFingerprint(expected: string | null, seen: string | null): Promise<void> {
    const ok = await confirmDialog({
      title: copyText("hostKey.compare.title", { machine: this.displayName() }),
      action: copyText("hostKey.compare.trust"),
      cancel: copyText("hostKey.compare.dontConnect"),
      danger: true,
      body: copyText("hostKey.compare.caution"),
      rows: [
        { label: copyText("hostKey.compare.pinned"), items: [expected || copyText("hostKey.compare.none")] },
        { label: copyText("hostKey.compare.seen"), items: [seen ?? copyText("hostKey.compare.none")] },
      ],
    });
    if (!ok || seen === null) return;
    // 信这一枚 = 忘掉库里那一枚：下回连上鉴权过了照首见记下这一枚（记在拨号核心那一处，界面不写指纹）。
    await this.forgetNow();
  }

  /** 不再问一遍地忘掉（比对框里已经确认过）。 */
  private async forgetNow(): Promise<void> {
    try {
      await forgetHostKeys(this.persistedKey ?? hostKey(this.collect()));
      this.setHostKeys([]);
    } catch (e) {
      console.warn("忘掉主机指纹没成", e);
    }
  }
}

/** 结果区的一行：成败由样式类表达（图标由代码画），字只是那句原话。 */
function resultLine(cls: "remote-test-ok" | "remote-test-err", text: string): HTMLElement {
  const line = document.createElement("div");
  line.className = `remote-test-line ${cls}`;
  line.textContent = text;
  return line;
}
