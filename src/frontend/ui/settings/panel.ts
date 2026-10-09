/**
 * 设置窗：机器（＋ 各台）· 文件与数据 · 扩展 · 外观 · 通用（＋ 日志）。
 * 外观只调 `theme.ts`、数据目录只调 `paths.ts`；打开时只 `loadConfig()` 一次，几格各经自己那一家的 `*In(cfg)` 派生。
 * 两种承载（`windowMode`）：独立设置窗口（今天唯一的用法：关 ＝ 藏窗口，保存 / 行为开关 / 重置后
 * `emit(SETTINGS_APPLIED_EVENT)` 让主窗口重读）；主窗口内浮层（`windowMode:false`，行为改动经 `onBehaviorChange` 同窗直连）。
 */

import { onMachineState } from "../machine-feed";
import { buildTerminalRow } from "./terminal-row";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  applyTheme,
  applyThemeToken,
  saveTheme,
  themeIn,
  type ThemeConfig,
} from "../theme";
import { claudeDirIn, setClaudeDirOverride } from "../paths";
import { loadConfig } from "../config";
import { AccountsSection } from "./accounts-section";
import { openRuleEditor, RulesSection } from "./rules-section";
import { ExtSection } from "./ext-section"; // 顶层「扩展」：跨机器的 skill / MCP，一张表 ＋ 一个抽屉
import { DiagnosticsSection } from "./diagnostics-section";
import { makeSkeleton } from "./skeleton";
import { SettingsRouter } from "./router";
import { buildMachinePage, localMeta, type MachinePage } from "./machine-page";
import { icon, type IconName } from "../kit/icon";
// 「重启才生效」的改动给常驻条供货（`markRestartNeeded`）。
import { createRestartBar, markRestartNeeded } from "./restart-notice";
import { restartNowButton } from "./restart-now";
import { ResumeSelect } from "./resume-select";
import { FirstRun } from "./first-run";
import { claudeDirProblem } from "./claude-dir-check";
import { createUnknownKeysBar, rerenderUnknownKeys } from "./unknown-keys-notice"; // 🔴 P12：未知键要出声
import { setCurrentMachine } from "./machine-context";
import { LOCAL_ORIGIN, isLocalOrigin } from "../ipc/origin";
import { failToast } from "../kit/toast";
import { toggleSwitch } from "../kit/switch";

type SwitchHandle = ReturnType<typeof toggleSwitch>;
import { machineFace, type MachineFix, type MachineState } from "./machine-state";
import {
  LOCAL_MACHINE_PAGE_ID,
  MACHINE_PAGE_PREFIX,
} from "./remote-section";
import { DataSection } from "./data-section";
import { DataPage } from "./data-page";
import { ContextLimitsSection } from "./context-limits-section"; // `contextLimits` 的入口
import { RemoteSection } from "./remote-section";
import type { MachineCardParts } from "./machine-card";
import { BackendSection } from "./backend-section"; // P2s（C8）：每台机一个后端开关
import {
  behaviorIn,
  setBehavior,
  withResumePreset,
  type BehaviorConfig,
} from "../behavior";
import { diagnoseRemoteLauncher } from "../launcher-diagnostics";
import { buildUnknownOsAliasBlock, localShell, rereadAliases } from "./machine-aliases"; // 机器页 ②「别名」（两个平台一份，含 PowerShell 的终端集成）
import { buildConfigPage } from "./config-page";
import { localResumeRow } from "./local-resume-row"; // 机器页「别名与配置文件」那一栏
import { dispatcher } from "../keybindings/registry";
import { KeybindingsEditor } from "../keybindings/editor";
// 跨 OS 窗口回调够不到 ⇒ 保存后广播 `settings-applied`，主窗口 listen 后重读（事件名在中立模块 events.ts）。
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { BEHAVIOR_TOGGLED_EVENT, SETTINGS_APPLIED_EVENT, SETTINGS_GO_EVENT, type BehaviorToggled } from "./events";
import { confirmDialog } from "../kit/dialog";
import { copyText } from "../copy-table";
import { parseSettingsTarget, type SettingsTarget } from "./open-settings";
import { detailOf, failSaid, sayFailure, sayWithDetail, writtenSaid } from "../kit/detail";
import { ControlError } from "../control-said";

/**
 * 字段控件类型：
 * - `color`/`number`/`text`：HTML `<input>`
 * - `font-base` / `font-mono`：`<select>`，options 来自 BASE_FONT_PRESETS / MONO_FONT_PRESETS
 *   + 自定义项（弹出文本输入）
 */
type FieldType = "color" | "number" | "text" | "font-base" | "font-mono";

interface FieldSpec {
  key: keyof ThemeConfig;
  label: string;
  type: FieldType;
  group: "font" | "color";
}

/**
 * 字体预设。value 是完整 CSS font-family 字符串；label 是给用户看的名字。
 * 第一个永远是"默认"（value 留空 → 删除覆盖，回到 styles.css :root）。
 */
// 下面这几张表与说明都做成函数、用到时才取文：模块顶层留一句取文调用，Rollup 就会把整个设置面板挪进主窗也加载的共享 chunk。
const BASE_FONT_PRESETS = (): ReadonlyArray<{ label: string; value: string }> => [
  { label: copyText("settingsPanel.font.default"), value: "" },
  { label: "Inter", value: "Inter, 'Segoe UI', system-ui, sans-serif" },
  {
    label: "Microsoft YaHei UI",
    value: "'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif",
  },
  { label: "Segoe UI", value: "'Segoe UI', system-ui, sans-serif" },
  { label: copyText("settingsPanel.font.system"), value: "system-ui, sans-serif" },
];

const MONO_FONT_PRESETS = (): ReadonlyArray<{ label: string; value: string }> => [
  { label: copyText("settingsPanel.font.default"), value: "" },
  { label: "JetBrains Mono", value: "'JetBrains Mono', Consolas, monospace" },
  { label: "Cascadia Code", value: "'Cascadia Code', Consolas, monospace" },
  { label: "Fira Code", value: "'Fira Code', Consolas, monospace" },
  { label: "Source Code Pro", value: "'Source Code Pro', Consolas, monospace" },
  { label: "Consolas", value: "Consolas, monospace" },
  { label: copyText("settingsPanel.font.systemMono"), value: "monospace" },
];

/** 基础字号的可读区间（px）：越界不存（0、1 能把字全弄没，连设置窗一起）。 */
const FONT_SIZE_MIN = 10;
const FONT_SIZE_MAX = 24;

const FIELDS = (): ReadonlyArray<FieldSpec> => [
  { key: "font-base", label: copyText("settingsPanel.field.fontBase"), type: "font-base", group: "font" },
  { key: "font-mono", label: copyText("settingsPanel.field.fontMono"), type: "font-mono", group: "font" },
  {
    key: "font-size-base",
    label: copyText("settingsPanel.field.fontSize"),
    type: "number",
    group: "font",
  },
  { key: "bg", label: copyText("settingsPanel.field.bg"), type: "color", group: "color" },
  { key: "bg-2", label: copyText("settingsPanel.field.bg2"), type: "color", group: "color" },
  { key: "card", label: copyText("settingsPanel.field.card"), type: "color", group: "color" },
  { key: "text", label: copyText("settingsPanel.field.text"), type: "color", group: "color" },
  { key: "text-2", label: copyText("settingsPanel.field.text2"), type: "color", group: "color" },
  // 「用户色」「Claude 色」不设取色器：没有 CSS 读那两个变量，拖了界面不会变。
  { key: "success", label: copyText("settingsPanel.field.success"), type: "color", group: "color" },
  { key: "warn", label: copyText("settingsPanel.field.warn"), type: "color", group: "color" },
  { key: "error", label: copyText("settingsPanel.field.error"), type: "color", group: "color" },
];

/** 行为类开关改了通知外部（TabManager 同步）。 */
export interface SettingsPanelOptions {
  onBehaviorChange?: (cfg: BehaviorConfig) => void;
  /** 窗口模式：面板是独立设置窗口的全部内容（不压弹层栈；关 ＝ 关窗口；保存后 `emit(SETTINGS_APPLIED_EVENT)`）。 */
  windowMode?: boolean;
}

// 各模块 ? 图标的悬停说明

/** 顶层页的图标。 */
const NAV_ICON: Record<"machines" | "data" | "ext" | "appearance" | "general", IconName> = {
  machines: "machine",
  data: "files",
  ext: "plug",
  appearance: "text",
  general: "sliders",
};

/** 顶层页的 id（目的地的 `page` 用同一套）。 */
const PAGE = {
  machines: "machines",
  data: "data",
  ext: "ext",
  appearance: "appearance",
  general: "general",
  logs: "logs",
} as const;

const SETTINGS_LANDING_ROUTE = PAGE.machines;

/** 目的地的高亮停留多久。 */
export const SETTINGS_HIGHLIGHT_MS = 1500;

/** 壳把目的地交给已开着的设置窗用的事件。 */
const SETTINGS_TARGET_EVENT = "settings-target";

/** 目的地 `tab` → 机器页里的栏。 */
const MACHINE_TAB_OF_TARGET: Record<string, string> = { acct: "acct", rot: "rot", config: "config" };
/** 顶层「扩展」页的路由 id。 */

/** 带目的地跳到一节时发给那一节的事件：折着的那一节据此展开。 */
const REVEAL_EVENT = "settings-reveal";

export class SettingsPanel {
  private el: HTMLElement;
  /** 页面路由器。 */
  private router!: SettingsRouter;
  /** 跟着当前机器页走的那几块分节（整块搬 DOM，不每台各起一份）。 */
  private perMachineSlot!: HTMLElement;
  /** 机器列表页上的加载态（不是兜底态）；机器页注册上来就撤掉。 */
  private perMachineFallbackHint!: HTMLElement;
  /** 机器页注册上来过没有：兜底态的判别式。 */
  private machinePageRegistered = false;
  private perMachineBlocks: {
    appliesTo: "local" | "remote" | "both";
    /** 这块归详情页的哪一栏。 */
    tab: "acct" | "rot" | "term" | "data";
    el: HTMLElement;
    /** 这一块的第一发 I/O：某台机器的子页第一次可见时才调。构造失败（`safeBlock` 收住）的块没有它。 */
    load?: () => void;
  }[] = [];
  /** 本次打开以来按机器那几块放过 I/O 没有。`open()` 清零（重开要看新读数）。 */
  private perMachineLoaded = false;
  /**
   * 页 → 它首次可见时才发的 I/O。块照常在 `buildBody` 里构造（构造时机不变、仍被 `safeBlock` 包着），
   * 只把那一发往返推到用户真看见那一页时；落地页之外的页打开设置时零 I/O。
   * 钩子在所有 `addRoute` 之后才挂：第一页注册的那一瞬它是 active 的，挂早了会被当成可见过而当场放行。
   */
  private readonly pageLoaders = new Map<string, Array<() => void>>();
  /** 本次打开以来，哪几页已经放过 I/O 了。`open()` 会清空它（重开要看新读数）。 */
  private readonly pagesLoaded = new Set<string>();
  /** pageId → 该页「账号 / 终端 / 足迹」三栏的容器。 */
  private machineTabSlots = new Map<string, { acct: HTMLElement; rot: HTMLElement; term: HTMLElement }>();
  /** 机器页 id → 它的卡头与两栏。 */
  private readonly machinePages = new Map<string, MachinePage>();
  /** 机器页 id → 它的分栏。 */
  /** 还没落地的目的地（那台的页还没注册上来时留着，注册了再落）。 */
  private pendingTarget: SettingsTarget | null = null;
  /** 各台最近一次问到的连接（导航点照它画；机器页后注册的也照它补）。 */
  private readonly channelOf = new Map<string, boolean | null>();
  /** 机器表那一趟读完了（成或败）：还没注册的机器页不会再来了。 */
  private machinePagesSettled = false;
  /** 当前编辑中的 theme（实时预览用）。 */
  private current: ThemeConfig = {};
  /** 盘上那一份（打开时读的，之后每次落盘跟着更新）—— 判「还有没落的那一格」用。 */
  private original: ThemeConfig = {};
  private inputs = new Map<
    keyof ThemeConfig,
    HTMLInputElement | HTMLSelectElement
  >();
  /** 各格下面那一行错误（用到时才建）。 */
  private fieldErrors = new Map<HTMLInputElement | HTMLSelectElement, HTMLElement>();
  private isOpen = false;

  /** Claude 数据目录输入框（改了要重启才生效）。 */
  private claudeDirInput!: HTMLInputElement;
  /** 机器页顶上「开始用」那一块。 */
  private firstRun?: FirstRun;
  /** Claude 目录那一行下：「重启 cc-monitor 后生效 ［现在重启］」（存了一个新的目录之后出现）。 */
  private claudeDirRestart!: HTMLElement;
  /** 打开时 claudeDir 的快照（变了才提示重启）。 */
  private claudeDirOriginal: string = "";
  /** 打开设置时 `showBgSessions` 的值（真改了才供货「重启后生效」）。 */
  private showBgOriginal = true;

  /** 顶部状态提示行（保存成功 / 需重启 等）。 */
  private banner!: HTMLElement;
  /** 「数据位置」展示区。 */
  private dataSection?: DataSection;
  /** 「文件与数据」两栏那一页。 */
  private dataPage?: DataPage;
  /** 「日志」块（「应用」页首次可见时叫醒它）。 */
  private logsSection?: DiagnosticsSection;
  /** 顶层「扩展」页那一块（构造失败 ⇒ 留 `undefined`，同 `remoteSection` 那一格的约定）。 */
  private extSection?: ExtSection;
  /** 远端（SSH）配置区。 */
  private remoteSection?: RemoteSection;
  /** 后端开关区（并进机器列表各行）。 */
  private backendSection?: BackendSection;
  /** 后端报来的各台状态成品（按后端那套名字）。 */
  private readonly machineOf = new Map<string, MachineState>();
  /** 「终端」栏「直接敲的 claude 也走中转」那一块：回到机器页时展开过就重问。 */

  // 通用页「行为」那几个开关与「恢复」那一组（拨了马上存；存失败拇指退回 ＋ 行下一句）
  private autoFollowSw!: SwitchHandle;
  private bringFrontSw!: SwitchHandle;
  private showBgSw!: SwitchHandle;
  private notifyTurnEndSw!: SwitchHandle;
  private notifyNeedsSw!: SwitchHandle;
  private resumeInTmuxSw!: SwitchHandle;
  /** 打开时读回的那一份行为（开关拨动按它改一格再存）。 */
  private behaviorNow: BehaviorConfig | null = null;
  /** 恢复命令那一格（各台的默认）与它的预设条 · 存失败时的那一句。 */
  private resumeSelect!: ResumeSelect;
  private resumeError!: HTMLElement;
  /** 「终端」那一行的位置（恢复组里，恢复命令下面；只在要挑终端的平台上画）。 */
  private readonly terminalSlot = document.createElement("div");
  private remoteLauncherWarning!: HTMLElement; // F08：越层启动器诊断提示（只诊断，不代改）
  private onBehaviorChange?: (cfg: BehaviorConfig) => void;
  /** 见 `SettingsPanelOptions.windowMode`。 */
  private readonly windowMode: boolean;

  /** 窗口模式下关 ＝ 藏（不销毁）：本面板藏起来之后窗口再拿到焦点 ＝ 被重新 show 出来 ⇒ 重跑一遍 `open()`。 */
  private hiddenByUs = false;

  // 快捷键编辑器（首次打开时才建）
  private kbEditor?: KeybindingsEditor;
  private kbOverrideChip?: HTMLElement;

  constructor(opts: SettingsPanelOptions = {}) {
    this.onBehaviorChange = opts.onBehaviorChange;
    this.windowMode = opts.windowMode ?? false;
    this.el = this.build();
    // 窗里一节要跳到别处（如账号页表下「共用 MCP」⇒ 同一台的「别名与配置文件」）：冒泡上来的目的地照样落。
    this.el.addEventListener(SETTINGS_GO_EVENT, (ev) => this.goTo(parseSettingsTarget((ev as CustomEvent<unknown>).detail)));
    document.body.appendChild(this.el);
    if (this.windowMode) this.installWindowLifecycle();
    // 那台的状态一变，壳推一帧：点 · 词 · 问题行原位换（不轮询）。
    onMachineState((origin, m) => this.paintMachine(origin, m));
    // 新建的窗：壳把目的地放在初始化脚本里。
    this.goTo(parseSettingsTarget((window as { __CCM_SETTINGS_TARGET__?: unknown }).__CCM_SETTINGS_TARGET__));
    // Esc 由 KeybindingDispatcher 统一调度：open 时压栈、close 时弹出，多弹层按 LIFO 关。
  }

  /** 弹层栈底：设置窗是窗口不是浮层，Esc 不关它（只关压在它上面的那一层）；关窗用 × 或 Ctrl+W。 */
  handleEsc(): void {}

  /**
   * 打开。好几个字段由 `safeBlock` 内部赋值，某块构造失败时它们仍是 undefined ⇒ 这里包一层：
   * 任何一步失败都照常打开并在 banner 里说明，不让 `open()` reject（隔离要覆盖整个生命周期，不只构造）。
   */
  async open(): Promise<void> {
    try {
      await this.openInner();
    } catch (e) {
      // 面板必须能打开：它是用户唯一的逃生口
      sayFailure(this.banner, copyText("settingsPanel.open.partialFailed"), e);
      this.banner.classList.add("settings-banner-show");
      this.el.classList.add("open");
      this.isOpen = true;
      dispatcher.pushOverlay(this);
    }
  }

  private async openInner(): Promise<void> {
    // 先开窗，再读一次配置派生外观 · 数据目录 · 行为三格。
    this.banner.textContent = "";
    this.banner.classList.remove("settings-banner-show");
    // 重开设置 ＝ 每一页的「首次可见」重新算，但仍只拉用户此刻看得见的那一页。
    this.pagesLoaded.clear();
    this.perMachineLoaded = false;
    // 每次打开重拉远端配置，跟外部改动对齐。`refresh()` 失败会画在那一块自己的 banner 上；这里 catch 掉，免得同一件事在状态栏再说一遍。
    // 「开始用」那一块：机器表读回来之后问一次（台数随问带上，不另读一遍配置）。
    void this.remoteSection
      ?.refresh()
      .then(() => this.firstRun?.loadNow(this.remoteSection?.machineCount()))
      .catch(() => {});
    // 后端状态是运行期的东西，每次打开都重拉；它住落地页（「机器」），不在延后那一档。
    void this.backendSection?.refresh();
    // 同步快捷键覆盖数（编辑器关闭时也可能改了）
    this.refreshKbChip();
    // 同一次运行里再打开：停在上次离开的那一页；带目的地的直达那里。
    this.applyPendingTarget();
    // `navigate()` 在「已经在这一页」时提前返回、不通知订阅者 ⇒ 第二次打开时落地页的首次可见必须在这里补一刀。
    this.flushPage(this.router.activeId);
    // 读回之前三格的控件是 pending（禁用）：勾选框此刻显示的是上一次的值，点上去就点在旧值上。
    this.setConfigCellsPending(true);
    this.el.classList.add("open");
    this.isOpen = true;
    // 面板始终是弹层栈底（窗口模式也是）：设置窗里的快捷键编辑器等压在上面时 Esc 逐层关，最后才关面板 → 关窗。
    dispatcher.pushOverlay(this);
    try {
      const cfg = await loadConfig();
      this.fillConfigCells(cfg);
      // 「config.json 里有认不出的项」那条跟着这一份重算（窗口是藏起来再开，那条不会自己重建）。
      rerenderUnknownKeys(cfg);
    } catch (e) {
      // 读不回来 ⇒ 三格各回落到缺省，外观不重刷。
      console.warn("设置窗读配置失败，外观 / 数据目录 / 行为按缺省显示：", e);
      this.fillConfigCells({}, false);
    } finally {
      this.setConfigCellsPending(false);
    }
  }

  /** 外观 · 数据目录 · 行为三格读回之前的 pending 态：这几格的控件一律禁用。 */
  private setConfigCellsPending(pending: boolean): void {
    const controls: HTMLInputElement[] = [
      ...[...this.inputs.values()].filter((c): c is HTMLInputElement => c instanceof HTMLInputElement),
      this.claudeDirInput,
    ];
    for (const c of controls) c.disabled = pending;
    this.resumeSelect.disabled = pending;
    for (const sw of this.behaviorSwitches()) sw.input.setAttribute("aria-disabled", String(pending));
    if (!pending) this.updateBringFrontEnabled();
  }

  /** 从**一份**配置派生三格（派生规则各住 `theme.ts` / `paths.ts` / `behavior.ts`，这里不另写）。 */
  private fillConfigCells(cfg: Record<string, unknown>, applyLoadedTheme = true): void {
    this.original = themeIn(cfg);
    if (applyLoadedTheme) applyTheme(this.original);
    this.current = { ...this.original };
    this.claudeDirOriginal = claudeDirIn(cfg) ?? "";
    this.claudeDirInput.value = this.claudeDirOriginal;
    // 每次打开拉最新的行为设置
    const behavior = behaviorIn(cfg);
    this.behaviorNow = behavior;
    this.autoFollowSw.set(behavior.autoFollowUserActive);
    this.bringFrontSw.set(behavior.bringMonitorToFrontOnUserActive);
    this.showBgSw.set(behavior.showBgSessions);
    // 记下打开设置时的值：真改了才供货（每次拨动都标会变成噪音）。
    this.showBgOriginal = behavior.showBgSessions;
    this.notifyTurnEndSw.set(behavior.notifyTurnEnd);
    this.notifyNeedsSw.set(behavior.notifyNeeds);
    this.resumeInTmuxSw.set(behavior.resumeInTmux);
    this.resumeSelect.set(behavior.resumeCommand, behavior.resumeCommandPresets);
    this.updateRemoteLauncherWarning();
    this.syncInputs();
  }

  /** 自动跟随关 ⇒ 「同时提到前台」灰显（依赖前者）。 */
  private updateBringFrontEnabled(): void {
    const on = this.behaviorNow?.autoFollowUserActive ?? true;
    this.bringFrontSw.input.setAttribute("aria-disabled", String(!on));
  }

  /** 行为那几个开关（读配置时整组禁用）。 */
  private behaviorSwitches(): SwitchHandle[] {
    return [this.autoFollowSw, this.bringFrontSw, this.showBgSw, this.notifyTurnEndSw, this.notifyNeedsSw, this.resumeInTmuxSw];
  }

  /** 越层启动器诊断：只读提示 ＋ 引导，不自动降级、不改恢复命令那一格。 */
  private updateRemoteLauncherWarning(): void {
    const msg = diagnoseRemoteLauncher(this.resumeSelect.value);
    this.remoteLauncherWarning.textContent = msg ?? "";
    this.remoteLauncherWarning.style.display = msg ? "block" : "none";
  }

  /** 恢复命令那一格选了 / 自定义那格失焦：存，并记进用过的那几条（空 ＝ 用默认那一家的启动器，不记）。 */
  private async saveResumeCommand(cmd: string): Promise<void> {
    const b = this.behaviorNow;
    if (!b) return;
    this.updateRemoteLauncherWarning();
    if (!(await this.saveBehavior({ ...b, resumeCommand: cmd, resumeCommandPresets: withResumePreset(b.resumeCommandPresets, cmd) }))) {
      this.resumeSelect.set(b.resumeCommand, b.resumeCommandPresets);
      this.updateRemoteLauncherWarning();
    }
  }

  /** 拨一个开关：按读回的那份改这一格再存；没读回 / 存失败 ⇒ `false`（拇指退回）。 */
  private async flipBehavior(patch: Partial<BehaviorConfig>, errorAt: HTMLElement): Promise<boolean> {
    const b = this.behaviorNow;
    if (!b) return false;
    return this.saveBehavior({ ...b, ...patch }, errorAt);
  }

  /**
   * 存一份行为（开关 · 恢复命令 · 删预设都走这里）。存成了才换内存里那份、重画预设、通知主窗口；
   * 存失败 ⇒ 回 `false`、行下那一句说为什么，盘上与界面都还是原来那份。
   */
  private async saveBehavior(next: BehaviorConfig, errorAt: HTMLElement = this.resumeError): Promise<boolean> {
    errorAt.hidden = true;
    try {
      await setBehavior(next);
    } catch (e) {
      console.warn("save behavior failed:", e);
      sayWithDetail(errorAt, copyText("settings.behavior.saveFailedLine", { why: String(e) }), detailOf(e));
      errorAt.hidden = false;
      return false;
    }
    this.behaviorNow = next;
    this.resumeSelect.set(next.resumeCommand, next.resumeCommandPresets);
    this.updateBringFrontEnabled();
    // `showBgSessions` 重启才生效（后端启动时读一次）⇒ 真改了才供货「重启后生效」。
    if (next.showBgSessions !== this.showBgOriginal) {
      markRestartNeeded(copyText("settingsPanel.behavior.hiddenTasks"));
      this.showBgOriginal = next.showBgSessions;
    }
    this.onBehaviorChange?.(next); // 同窗（主窗口浮层）直接同步 TabManager
    this.broadcastApplied(); // 窗口模式：广播让主窗口 applyBehavior
    return true;
  }

  /**
   * 窗口模式下接管两件事：系统标题栏的 X（`onCloseRequested` 一律 `preventDefault`，交给 [`requestClose`]）
   * 与藏起来之后又被 show 出来（focus ⇒ 重跑 `open()`）。挂了监听 Tauri 就不替我们关窗，放行的只剩 `hide()`；
   * 本面板从不 destroy / close（主窗销毁时后端把本窗一起 destroy，否则藏着的窗口会吊住进程）。
   * 挂监听失败不许把面板炸穿：那时系统 X 退回默认行为（销毁），只是少了拦截。
   */
  private installWindowLifecycle(): void {
    window.addEventListener("keydown", (ev) => {
      if ((ev.ctrlKey || ev.metaKey) && !ev.altKey && !ev.shiftKey && ev.key.toLowerCase() === "w") {
        ev.preventDefault();
        this.requestClose();
      }
    });
    void Promise.resolve()
      .then(() => listen<unknown>(SETTINGS_TARGET_EVENT, (e) => this.goTo(parseSettingsTarget(e.payload))))
      .catch((e: unknown) => console.warn("[settings] 挂目的地监听失败：", e));
    try {
      const w = getCurrentWindow();
      void w
        .onCloseRequested((e) => {
          e.preventDefault();
          this.requestClose();
        })
        .catch((e: unknown) => console.warn("[settings] 挂关窗拦截失败：", e));
      void w
        .onFocusChanged(({ payload: focused }) => {
          if (!focused || !this.hiddenByUs) return;
          this.hiddenByUs = false;
          void this.open();
        })
        .catch((e: unknown) => console.warn("[settings] 挂重新打开监听失败：", e));
      // 主窗口用快捷键翻了「自动跟随 / 自动切到前台」⇒ 这一页的两个开关跟着变（不然下一次在这页改别的会把旧值写回去）。
      void listen<BehaviorToggled>(BEHAVIOR_TOGGLED_EVENT, (e) => this.showBehaviorToggled(e.payload)).catch((e: unknown) =>
        console.warn("[settings] 挂行为开关同步失败：", e),
      );
    } catch (e) {
      console.warn("[settings] 窗口生命周期接管失败：", e);
    }
  }

  /** 主窗口翻过的那两格照它说的值摆上。 */
  private showBehaviorToggled(b: BehaviorToggled): void {
    if (this.behaviorNow) {
      this.behaviorNow = { ...this.behaviorNow, autoFollowUserActive: b.autoFollowUserActive, bringMonitorToFrontOnUserActive: b.bringMonitorToFrontOnUserActive };
    }
    this.autoFollowSw.set(b.autoFollowUserActive);
    this.bringFrontSw.set(b.bringMonitorToFrontOnUserActive);
    if (this.autoFollowSw.input.getAttribute("aria-disabled") !== "true") this.updateBringFrontEnabled(); // 还在读配置（整组禁用）时不提前放开
  }

  /** 带目的地打开：直达那一页（那一栏），高亮那一节 1.5 秒。认不出的格忽略。 */
  goTo(target: SettingsTarget | null): void {
    if (!target) return;
    this.pendingTarget = target;
    this.applyPendingTarget();
  }

  /** 「这台上的 cc-monitor」折叠头右侧：`版本 x · 在跑` ／ `未连接`（没问到 ⇒ 空）。 */
  private ccSummary(origin: string, connected: boolean | null): string {
    if (connected === null) return "";
    const state = connected ? copyText("machinePage.cc.summaryUp") : copyText("machinePage.cc.summaryDown");
    const ver = this.machineOf.get(origin)?.version ?? null;
    return ver ? copyText("machinePage.cc.summaryVer", { ver, state }) : state;
  }

  /**
   * 照后端的状态成品画那台（导航点 · 列表那一行 · 卡头 · 第二行的系统与版本）。
   * 收不下 / 没问到 ⇒ 不动（「连上没有」那一路照旧画）。
   */
  private paintMachine(origin: string, m: MachineState | null): void {
    if (m === null) return;
    this.machineOf.set(origin, m);
    const local = isLocalOrigin(origin);
    const pageId = local ? LOCAL_MACHINE_PAGE_ID : this.remoteSection?.pageIdOfMachine(origin);
    if (!pageId) return;
    const machine = local ? copyText("remote.cards.local") : origin;
    const face = machineFace(m, machine);
    this.remoteSection?.setFacts(origin, { os: m.os, version: m.version, dev: m.versionRelation === "incomparable" });
    this.remoteSection?.setMachine(pageId, m);
    this.router.setNavDot(pageId, face.dot, face.word ? `${machine} · ${face.word}` : copyText("settingsNav.dot.up", { machine }));
    const page = this.machinePages.get(pageId);
    if (!page) return;
    page.setDisabled(m.state === "disabled");
    if (m.state !== "disabled") page.setMachine(face);
    page.setCcSummary(this.ccSummary(origin, this.channelOf.get(origin) ?? null));
    const meta = this.remoteSection?.metaOfPage(pageId);
    if (meta !== undefined && meta !== null) page.setMeta(meta);
  }

  /** 问题行上交到宿主的修法：重试（叫那条流当场重拨）· 更新（换上这一版，再重拨）。 */
  private async runFix(pageId: string, fix: MachineFix): Promise<void> {
    const origin = pageId === LOCAL_MACHINE_PAGE_ID ? LOCAL_ORIGIN : this.remoteSection?.originOfPage(pageId);
    if (!origin) return;
    if (fix === "compare_fingerprint") {
      await this.remoteSection?.compareFingerprint(pageId);
      return;
    }
    try {
      if (fix === "update") await this.remoteSection?.updateMachine(pageId);
      await this.backendSection?.reconnect(origin);
    } catch (e) {
      failToast(copyText("machineState.fix.failed"), e);
    }
  }

  /** 导航里那台机器前的点：连着 · 离线 · 没问到。 */
  private paintNavDot(origin: string, connected: boolean | null): void {
    this.channelOf.set(origin, connected);
    const local = isLocalOrigin(origin);
    const pageId = local ? LOCAL_MACHINE_PAGE_ID : this.remoteSection?.pageIdOfMachine(origin);
    if (!pageId) return;
    const machine = local ? copyText("remote.cards.local") : origin;
    this.remoteSection?.setConnected(pageId, connected);
    if (!local && this.remoteSection?.isDisabledPage(pageId)) {
      this.machinePages.get(pageId)?.setDisabled(true);
      this.router.setNavDot(pageId, "exited", copyText("settingsNav.dot.disabled", { machine }));
      return;
    }
    this.machinePages.get(pageId)?.setDisabled(false);
    this.machinePages.get(pageId)?.setConnected(connected);
    this.machinePages.get(pageId)?.setCcSummary(this.ccSummary(origin, connected));
    if (connected === true) this.router.setNavDot(pageId, "up", copyText("settingsNav.dot.up", { machine }));
    else if (connected === false) this.router.setNavDot(pageId, "failed", copyText("settingsNav.dot.down", { machine }));
    else this.router.setNavDot(pageId, "unknown", copyText("settingsNav.dot.unknown", { machine }));
  }

  private applyPendingTarget(): void {
    const t = this.pendingTarget;
    if (!t) return;
    let pageId: string | null = null;
    if (t.machine) {
      pageId = isLocalOrigin(t.machine) ? LOCAL_MACHINE_PAGE_ID : (this.remoteSection?.pageIdOfMachine(t.machine) ?? null);
      if (!pageId || !this.router.has(pageId)) {
        // 机器页还没注册上来（读机器表是异步的）⇒ 留着，注册了再落；列表都读完了还没有 ⇒ 落在机器列表。
        if (!this.machinePagesSettled) return;
        pageId = PAGE.machines;
      }
    } else if (t.page && (Object.values(PAGE) as string[]).includes(t.page)) {
      pageId = t.page;
    }
    this.pendingTarget = null;
    if (!pageId) return;
    this.router.navigate(pageId);
    // 文件与数据那一页的锚点带着机器（`chores:<origin>` · `placed:<origin>`）：交给那一页落（那一段可能还没读回来）。
    if (pageId === PAGE.data && t.anchor && /^(chores|placed):/.test(t.anchor)) {
      this.dataPage?.focus(t.anchor);
      return;
    }
    // 面板「编辑规则…」带 `rule:<id>`：「轮换」栏开那条的编辑器（那一栏读到那台的表时开）。
    if (t.machine && t.anchor?.startsWith("rule:")) openRuleEditor(isLocalOrigin(t.machine) ? LOCAL_ORIGIN : t.machine, t.anchor.slice("rule:".length));
    const tabs = this.machinePages.get(pageId)?.tabs;
    const tabId = t.tab ? MACHINE_TAB_OF_TARGET[t.tab] : undefined;
    if (tabs && tabId) tabs.navigate(`${pageId}#${tabId}`);
    const page = this.router.pageOf(pageId);
    if (!page) return;
    const spot =
      (t.anchor ? [...page.querySelectorAll<HTMLElement>("[data-anchor]")].find((e) => e.dataset.anchor === t.anchor) : null) ??
      (tabs && tabId ? tabs.navButtonOf(`${pageId}#${tabId}`) : null) ??
      page.querySelector<HTMLElement>(":scope > .settings-page-head");
    if (!spot) return;
    spot.dispatchEvent(new Event(REVEAL_EVENT));
    spot.scrollIntoView?.({ block: "nearest" });
    spot.classList.add("settings-highlight");
    // 调度：一次性 —— 带目的地打开：1.5 秒后撤掉那一节的高亮
    window.setTimeout(() => spot.classList.remove("settings-highlight"), SETTINGS_HIGHLIGHT_MS);
  }

  /** 还有没落盘的那一格没有：输入框里改了、焦点还没离开（`change` 还没触发）就关窗 —— 由 `requestClose` 顺手落掉。 */
  isDirty(): boolean {
    const norm = (t: ThemeConfig): string =>
      JSON.stringify(
        Object.entries(t)
          .filter(([, v]) => v !== undefined && v !== null && v !== "")
          .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
      );
    return (
      norm(this.current) !== norm(this.original) ||
      this.claudeDirInput.value.trim() !== this.claudeDirOriginal
    );
  }

  /** 系统 X、页头 ×、Esc 都走这里：关之前把还没落的那一格落掉，落不下就不关。 */
  requestClose(): void {
    if (!this.isDirty()) {
      this.close();
      return;
    }
    void this.flushPending()
      .then(() => {
        if (this.isOpen) this.close();
      })
      .catch((e: unknown) => {
        // 落不下就不关：窗口留着、说出原因，用户的改动还在输入框里。
        sayFailure(this.banner, copyText("settingsPanel.close.saveFailed"), e);
        this.banner.classList.add("settings-banner-show");
      });
  }

  /** 把还没触发 `change` 的那一格落掉（外观 ＋ Claude 数据目录，各自只在真的改了时写）。 */
  private async flushPending(): Promise<void> {
    await this.persistTheme();
    await this.persistClaudeDir();
  }

  close(): void {
    // 关之前 blur 掉面板里仍聚焦的输入框：面板只是藏起来、焦点不会自己释放，单键快捷键守卫会以为还在打字、把单键全吞掉。
    const active = document.activeElement;
    if (active instanceof HTMLElement && this.el.contains(active))
      active.blur();
    // 窗口模式：关 ＝ 藏窗口，面板状态照样收好；下一次 show 出来由 focus 那一路重跑 `open()`。
    if (this.windowMode) {
      this.el.classList.remove("open");
      this.isOpen = false;
      dispatcher.popOverlay(this);
      this.hiddenByUs = true;
      void getCurrentWindow()
        .hide()
        .catch((e: unknown) => {
          // 藏不掉就别假装藏了：窗口还在屏幕上，面板得回到能用的样子并说出原因。
          this.hiddenByUs = false;
          void this.open().then(() => {
            sayFailure(this.banner, copyText("settingsPanel.close.failed"), e);
            this.banner.classList.add("settings-banner-show");
          });
        });
      return;
    }
    this.el.classList.remove("open");
    this.isOpen = false;
    dispatcher.popOverlay(this);
  }

  /** 窗口模式下广播「设置已应用」，主窗口重读并应用；浮层模式同窗直接生效，不广播。 */
  private broadcastApplied(): void {
    if (this.windowMode) void emit(SETTINGS_APPLIED_EVENT);
  }

  /**
   * 外观改了就落：`input` 只做实时预览（拖取色器 ~60Hz，不逐帧写盘），
   * `change`（松手 / 失焦 / 选中）才调这里。没变就不写。
   */
  private async persistTheme(): Promise<void> {
    if (JSON.stringify(this.current) === JSON.stringify(this.original)) return;
    await saveTheme(this.current);
    this.original = { ...this.current };
    this.broadcastApplied(); // 主窗口重读主题并应用
  }

  /** Claude 数据目录改了就落（`change` / 浏览… / 重置）。要重启才生效 ⇒ 给常驻条供货 ＋ 当场一条 banner。 */
  private async persistClaudeDir(): Promise<void> {
    const nextDir = this.claudeDirInput.value.trim();
    if (nextDir === this.claudeDirOriginal) return;
    // 不在的目录不存：照收的话重启后会被悄悄忽略。那一句（「{path} 不存在」这一族）抛出去，调用方接在「Claude 数据目录未保存 · 」后面。
    const problem = nextDir === "" ? null : await claudeDirProblem(nextDir);
    if (problem !== null) throw new ControlError(problem, "");
    await setClaudeDirOverride(nextDir === "" ? null : nextDir);
    this.claudeDirOriginal = nextDir;
    // 当场的 banner 关窗即没，而「还没生效」一直为真到重启为止 ⇒ 两样都要。
    markRestartNeeded(copyText("settingsPanel.save.claudeDir"));
    this.claudeDirRestart.hidden = false;
    this.banner.textContent =
      copyText("settingsPanel.claudeDir.updated");
    this.banner.classList.add("settings-banner-show");
  }

  /** 落盘失败时说出来（全即时的每一格都走它，不许静默吞）。 */
  private reportSaveFailure(what: string, e: unknown): void {
    // 「哪一项」只有界面知道 ⇒ 打头；出错那端写好的那一句跟在后面一格。JS 自己抛的 ⇒「{what}保存失败」，原文进控制台。
    const said = writtenSaid(e);
    const line = said === null ? failSaid(copyText("settingsPanel.save.failed", { what }), e) : copyText("settingsPanel.save.notSaved", { what, said });
    sayWithDetail(this.banner, line, detailOf(e));
    this.banner.classList.add("settings-banner-show");
  }

  private async resetAll(): Promise<void> {
    if (
      !(await confirmDialog({
        title: copyText("settingsPanel.appearance.resetTitle"),
        action: copyText("settingsPanel.appearance.resetAction"),
        body: copyText("settingsPanel.appearance.resetConfirm"),
      }))
    ) {
      return;
    }
    this.current = {};
    applyTheme({});
    await saveTheme({});
    this.original = {};
    this.syncInputs();
    this.broadcastApplied(); // 窗口模式：主窗口也回默认主题（否则停在旧自定义配色）
  }

  private async pickClaudeDir(): Promise<void> {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: copyText("settingsPanel.claudeDir.pickTitle"),
      });
      if (typeof selected === "string" && selected) {
        this.claudeDirInput.value = selected;
        // 选完就落（全即时）。
        await this.persistClaudeDir().catch((e: unknown) =>
          this.reportSaveFailure(copyText("settingsPanel.save.claudeDir"), e),
        );
      }
    } catch (e) {
      console.warn("dialog open failed:", e);
      // 点了「选择…」却什么都没发生 ⇒ 说出来（落在同一块 banner 上）。
      sayFailure(this.banner, copyText("settingsPanel.claudeDir.pickFailed"), e);
      this.banner.classList.add("settings-banner-show");
    }
  }

  private resetClaudeDir(): void {
    this.claudeDirInput.value = "";
    // 重置也是改了就落。
    void this.persistClaudeDir().catch((e: unknown) =>
      this.reportSaveFailure(copyText("settingsPanel.save.claudeDir"), e),
    );
  }

  // === DOM 构建 ===

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-panel";
    root.appendChild(this.buildBody());
    // 两条常驻条与保存提示住内容区顶（导航不受影响），排在窄窗下拉之后。
    const content = this.router.contentElement;
    const anchor = content.querySelector(":scope > .settings-nav-select");
    const top = [createUnknownKeysBar(), createRestartBar(), this.banner];
    if (anchor) anchor.after(...top);
    else content.prepend(...top);
    return root;
  }

  /** 把按机器的那几块分节搬到某一页上（`appendChild` 即搬，同一时刻只在一页上），并按对本机 / 远端有没有意义显隐。 */
  private movePerMachineTo(
    page: HTMLElement,
    isLocal: boolean,
    pageId?: string,
  ): void {
    const movable = this.perMachineBlocks.filter((b) => b.tab !== "data");
    for (const b of movable) {
      b.el.hidden =
        b.appliesTo !== "both" && b.appliesTo !== (isLocal ? "local" : "remote");
    }
    const slots = pageId ? this.machineTabSlots.get(pageId) : undefined;
    if (slots) {
      // 分栏页：每块按 `tab` 归到「账号 / 终端 / 足迹」栏里。
      for (const b of movable) slots[b.tab as "acct" | "rot" | "term"].appendChild(b.el);
      return;
    }
    // 没有分栏（本机页 · 兜底落点）⇒ 整块搬。
    for (const b of movable) this.perMachineSlot.appendChild(b.el);
    page.appendChild(this.perMachineSlot);
  }

  /** 一台机器的页（本机远端同形）：卡头（连接设置 · 这台上的 cc-monitor）＋ 账号 · 别名与配置文件。 */
  private buildMachinePage(pageId: string, title: string, parts?: MachineCardParts): MachinePage {
    const local = pageId === LOCAL_MACHINE_PAGE_ID;
    const origin = local ? LOCAL_ORIGIN : (this.remoteSection?.originOfPage(pageId) ?? "");
    const cc: HTMLElement[] = [];
    if (origin && this.backendSection) {
      try {
        const rows = parts ? [parts.components] : local ? [localResumeRow()] : [];
        cc.push(this.backendSection.cellsFor(origin, { rows, trailing: parts?.uninstall }));
      } catch (e) {
        // 那几格没建起来不许把这一页带走（那台退到后端清单的尾巴里）。
        console.warn("[settings] 这台上的 cc-monitor 那几格没建起来：", e);
        if (parts) cc.push(parts.components);
      }
    } else if (parts) {
      cc.push(parts.components);
    }
    const page = buildMachinePage({
      pageId,
      name: title,
      meta: (local ? localMeta("head") : this.remoteSection?.metaOfPage(pageId)) ?? "",
      connection: parts?.connection,
      ccMonitor: cc,
      menu: () => this.remoteSection?.menuFor(pageId) ?? [],
    });
    if (parts) page.slots.config.appendChild(parts.terminal);
    this.machinePages.set(pageId, page);
    this.machineTabSlots.set(pageId, { acct: page.slots.acct, rot: page.slots.rot, term: page.slots.config });
    if (origin) page.setConnected(this.channelOf.get(origin) ?? null);
    return page;
  }

  private buildBody(): HTMLElement {
    const body = document.createElement("div");
    body.className = "settings-body";

    this.banner = document.createElement("div");
    this.banner.className = "settings-banner";

    const router = new SettingsRouter({ landingId: SETTINGS_LANDING_ROUTE });
    this.router = router;

    // 页按导航顺序注册：机器（＋ 各台）· 文件与数据 · 扩展 · 外观 · 通用（＋ 日志）。
    //   各页内容先建好，注册集中在机器页之后（机器子页异步注册，插在「机器」后面）。
    const appearancePage = document.createElement("div");
    appearancePage.appendChild(this.buildGroup(copyText("settingsPanel.group.fonts"), FIELDS().filter((f) => f.group === "font")));
    appearancePage.appendChild(this.buildGroup(copyText("settingsPanel.group.colors"), FIELDS().filter((f) => f.group === "color")));
    const kbBlock = this.safeBlock(copyText("settingsPanel.group.keybindings"), () => this.buildKeybindingsGroup());
    // 主窗口快捷键一览的「改快捷键…」开 `{page: "appearance", anchor: "keybindings"}`：落到这一节。
    kbBlock.dataset.anchor = "keybindings";
    appearancePage.appendChild(kbBlock);
    const resetRow = document.createElement("div");
    resetRow.className = "settings-row settings-row-end";
    resetRow.appendChild(
      this.makeBtn(copyText("settingsPanel.appearance.reset"), "secondary", () => void this.resetAll()),
    );
    appearancePage.appendChild(resetRow);

    const generalPage = document.createElement("div");
    generalPage.appendChild(this.safeBlock(copyText("settingsPanel.group.behavior"), () => this.buildBehaviorGroup()));
    generalPage.appendChild(this.safeBlock(copyText("settingsPanel.group.resume"), () => this.buildResumeGroup()));
    let limits: ContextLimitsSection | null = null;
    const limitsBlock = this.safeBlock(
      copyText("contextLimits.editor.title"),
      () => {
        limits = new ContextLimitsSection();
        return limits.element;
      },
      { untitled: true },
    );
    limitsBlock.addEventListener(REVEAL_EVENT, () => limits?.reveal());
    // 主窗口［设上限］开 `{page: "general", anchor: "context-limits"}`：落到这一节（展开）并高亮。
    limitsBlock.dataset.anchor = "context-limits";
    generalPage.appendChild(limitsBlock);

    const logsPage = document.createElement("div");
    logsPage.appendChild(
      this.safeBlock(
        copyText("settingsPanel.group.logs"),
        () => {
          const sec = new DiagnosticsSection();
          this.logsSection = sec;
          return sec.element;
        },
        { untitled: true },
      ),
    );

    // 文件与数据：两栏（要你动手 · cc-monitor 放了什么）；本机那两块（Claude 目录 · cc-monitor 的文件）由这里建好交进去。
    const dataPage = document.createElement("div");
    const ownFiles = this.safeBlock(
      copyText("settingsPanel.group.dataPlaces"),
      () => {
        const sec = new DataSection({ headless: true });
        this.dataSection = sec;
        return sec.element;
      },
      { untitled: true },
    );
    dataPage.appendChild(
      this.safeBlock(
        copyText("settingsPanel.nav.data"),
        () => {
          const page = new DataPage({
            claudeDir: this.buildDataGroup(),
            ownFiles,
            goTo: (t) => this.goTo(t),
            setBadge: (n) => this.router.setBadge(PAGE.data, n),
          });
          this.dataPage = page;
          return page.element;
        },
        { untitled: true },
      ),
    );

    // ---- 机器：改某一台机器的状态 ----
    const machinesPage = document.createElement("div");
    // 后端开关并进机器列表那一行上（`BackendSection.cellsFor`）。它构造失败不许把机器列表带走：
    // 失败时行上不挂那几格，这里亮一块「此区块加载失败」。
    let backend: BackendSection | undefined;
    try {
      backend = new BackendSection({
        headless: true,
        hosted: true,
        onChannel: (origin, connected) => this.paintNavDot(origin, connected),
        onMachine: (origin, machine) => this.paintMachine(origin, machine),
      });
      this.backendSection = backend;
    } catch (e) {
      machinesPage.appendChild(
        this.safeBlock(copyText("settingsPanel.group.backend"), () => {
          throw e;
        }),
      );
    }
    // 「开始用」那一块：列表上方；三步由后端事实打勾，全做完 / 点过「跳过」不出现。机器页可见时才问。
    this.firstRun = new FirstRun({ go: (t) => this.goTo(t), addMachine: () => void this.remoteSection?.addMachine() });
    machinesPage.appendChild(this.firstRun.element);
    // 必须在 `safeBlock` 里：`RemoteSection` 构造会同步抛（远端分节里三句话必填的那个 `throw`），裸构造会让整个设置窗什么都上不了屏。
    // 失败时 `this.remoteSection` 留 `undefined`（`open()` 那边是 `?.refresh()`）。
    machinesPage.appendChild(
      this.safeBlock(copyText("settingsPanel.group.remote"), () => {
        // 每台机器的编辑表单去它自己那一页，列表里只留一行；分节只要「开页 / 收页 / 跳过去」。
        const sec = new RemoteSection({
          headless: true,
          rowExtras: backend && {
            // 后端清单里有、机器列表里没有的那几台（重名被后缀化 / 列表还没读出来）。
            tail: () => backend.element,
          },
          pages: {
            machinePagesSettled: () => this.onMachinePagesSettled(),
            addMachinePage: (id, title, element, parts) => {
              // 机器页真注册上来了 ⇒ 兜底态不该出现。（那两句 `.hidden =` 住在 `onMachinePageRegistered()` 里：
              // `css-conventions.vitest.ts` 那把尺子只往上游找类名赋值，写在这儿它就看不见了。）
              this.onMachinePageRegistered();
              // 登记表只能在这一刻填，用 `set` 不用 `push`（每次 refresh 都会重注册同一批页）。按机器那几块是单例、跟着子页搬 ⇒
              // 任意一台子页第一次可见时放一次（`loadPerMachineOnce` 自己去重）；之后切机器由各块自己的订阅重读。
              this.pageLoaders.set(id, [() => this.loadPerMachineOnce()]);
              // 机器页拆成横向几栏：复用 `SettingsRouter`（横向 ＋ 无页头），不另造一套 tab 原语。
              void element;
              const pageEl = this.buildMachinePage(id, title, parts).element;
              router.addRoute({ id, title, element: pageEl, parentId: PAGE.machines, hideHead: true });
              // 本机页一出现就让那几块分节落在它上面（与 `machine-context` 的初始值本机对齐），否则用户点进某台之前它们不在文档里。
              if (id === LOCAL_MACHINE_PAGE_ID) this.movePerMachineTo(pageEl, true, id);
              const origin = id === LOCAL_MACHINE_PAGE_ID ? LOCAL_ORIGIN : this.remoteSection?.originOfPage(id);
              if (origin) this.paintNavDot(origin, this.channelOf.get(origin) ?? null);
              this.applyPendingTarget();
            },
            removeMachinePage: (id) => {
              router.removeRoute(id);
              this.machinePages.delete(id);
              this.machineTabSlots.delete(id);
              // 机器没了 ⇒ 它那一页的登记与「放过了没有」一起清（同名机器再出现时不会被当成已放过）。
              this.pageLoaders.delete(id);
              this.pagesLoaded.delete(id);
            },
            navigateToMachinePage: (id) => router.navigate(id),
            renameMachinePage: (id, title) => {
              router.setTitle(id, title);
              this.machinePages.get(id)?.setTitle(title);
              const meta = this.remoteSection?.metaOfPage(id);
              if (meta !== undefined && meta !== null) this.machinePages.get(id)?.setMeta(meta);
            },
            openMachineSection: (id, section) => {
              router.navigate(id);
              this.machinePages.get(id)?.open(section);
            },
            refreshMachine: (origin) => void this.backendSection?.refreshOrigin(origin),
            runFix: (id, fix) => void this.runFix(id, fix),
            machinesReconciled: () => {
              for (const [origin, c] of this.channelOf) this.paintNavDot(origin, c);
              void this.backendSection?.refresh();
            },
          },
        });
        this.remoteSection = sec;
        return sec.element;
      }, { untitled: true }),
    );
    // 这几块讲的是某一台机器的事 ⇒ 单例、跟着「当前在看哪台机器」搬 DOM（不每台各起一份：同一时刻只有一页可见）。
    // `appliesTo` 决定它在本机页 / 远端页出不出现。
    this.perMachineSlot = document.createElement("div");
    this.perMachineSlot.className = "machine-page-sections";
    this.perMachineBlocks = [
      {
        // 账号两页都有意义（每台机器的账号归那台）：一台没配远端的机器只有本机页，写成只远端的话新用户就看不见它。
        // 渲染成什么样归 `accounts-section.vitest.ts`；在哪一页看得见归 `panel-machine-page-visibility.vitest.ts`。
        appliesTo: "both",
        tab: "acct",
        ...this.loadableBlock(copyText("settingsPanel.group.accounts"), () => new AccountsSection()),
      },
      {
        // 规则按机器存（那台的 rotation.json）：两页都有。渲染归 `rules-section.vitest.ts`。
        appliesTo: "both",
        tab: "rot",
        ...this.loadableBlock(copyText("machinePage.tab.rot"), () => new RulesSection()),
      },
      // 本机那一格「别名」（Windows 上是 PowerShell 那一侧的别名块）；远端那一格在 `MachineCard` 的「组件」栏里。
      // 构造零 I/O：它是个 `<details>`，第一次展开才读盘。
      {
        appliesTo: "local",
        tab: "term",
        el: this.safeBlock(
          copyText("settingsPanel.group.aliases"),
          () => {
            // 认不出本机系统 ⇒ 不猜方言：明说、安装入口置灰。
            const shell = localShell();
            return shell === null
              ? buildUnknownOsAliasBlock()
              : buildConfigPage({
                  platform: shell,
                  origin: () => LOCAL_ORIGIN,
                  machine: () => copyText("remote.cards.local"),
                });
          },
        ),
      },
    ];
    for (const b of this.perMachineBlocks) this.perMachineSlot.appendChild(b.el);
    // 兜底落点：`RemoteSection` 挂掉就没有机器页被注册，这几块会一起从界面上消失 ⇒ 那时把它们留在列表页上（位置不理想，但都能用）。
    // 平时机器列表是异步加载的 ⇒ slot 默认藏起来、先给一块骨架；机器页注册上来骨架撤掉、slot 被搬走。
    // 只在真失败时亮兜底：构造同步抛（`this.remoteSection` 留 `undefined`），或 `refresh()` reject 一个页都没来
    // （`machinePagesSettled()` 回调兜，不靠定时器猜）。
    this.perMachineSlot.hidden = true;
    this.perMachineFallbackHint = makeSkeleton(
      "backend",
      copyText("settingsPanel.machines.loading"),
    );
    machinesPage.appendChild(this.perMachineFallbackHint);
    machinesPage.appendChild(this.perMachineSlot);
    // 构造同步抛 ⇒ 当场亮兜底。必须排在 slot 与提示建出来之后，否则它读两个还没赋值的字段，把一块挂了变成整个面板挂了。
    if (!this.remoteSection) this.revealPerMachineFallback(copyText("settingsPanel.machines.buildFailed"));
    router.addRoute({
      id: "machines",
      title: copyText("settingsPanel.nav.machines"),
      element: machinesPage,
      icon: NAV_ICON.machines,
      headActions: this.remoteSection?.headActions() ?? [],
    });
    router.addRoute({
      id: PAGE.data,
      title: copyText("settingsPanel.nav.data"),
      sub: copyText("dataPage.page.sub"),
      element: dataPage,
      icon: NAV_ICON.data,
      gapBefore: true,
    });
    // 第一次切进来才问（之后靠［全部重查］显式补一刀；切页不是轮询）。
    this.loadOnFirstVisit(PAGE.data, () => {
      this.dataSection?.loadNow();
      this.dataPage?.loadNow();
    });

    // ---- 扩展：跨机器的 skill / MCP ----
    //   每次变可见都重读一趟（「新」按上一次来看算）；构造零 I/O。
    const extPage = document.createElement("div");
    extPage.appendChild(
      this.safeBlock(
        copyText("settingsPanel.nav.ext"),
        () => {
          const sec = new ExtSection();
          this.extSection = sec;
          return sec.element;
        },
        { untitled: true },
      ),
    );
    router.addRoute({ id: PAGE.ext, title: copyText("settingsPanel.nav.ext"), element: extPage, icon: NAV_ICON.ext });
    router.addRoute({ id: PAGE.appearance, title: copyText("settingsPanel.nav.appearance"), element: appearancePage, icon: NAV_ICON.appearance, gapBefore: true });
    router.addRoute({ id: PAGE.general, title: copyText("settingsPanel.nav.general"), element: generalPage, icon: NAV_ICON.general });
    router.addRoute({
      id: PAGE.logs,
      title: copyText("settingsPanel.nav.logs"),
      element: logsPage,
      parentId: PAGE.general,
      sub: copyText("diagnostics.page.sub"),
      headActions: this.logsSection ? [this.logsSection.headButton()] : [],
    });
    this.loadOnFirstVisit(PAGE.logs, () => this.logsSection?.loadNow());
    this.loadOnFirstVisit(PAGE.general, () => {
      void buildTerminalRow()
        .then((row) => this.terminalSlot.replaceChildren(...(row ? [row] : [])))
        .catch((e: unknown) => console.warn("[settings] terminal row", e));
    });
    router.onNavigate((id) => {
      if (id === PAGE.ext) this.extSection?.loadNow();
    });



    // 切到某台机器页 ⇒ 把那几块分节搬进来、同步「当前在看哪台」。挂在路由器这一层：切页有两个入口（导航项 · 列表行），这里两条都覆盖。
    router.onNavigate((id) => {
      if (!id.startsWith(MACHINE_PAGE_PREFIX)) return;
      const isLocal = id === LOCAL_MACHINE_PAGE_ID;
      // 页 id 建卡时定死、不跟着改名走 ⇒ 这页讲的是哪台问机器列表（改过名的是新名；刚导入的是它的别名）。
      // 还没填主机地址的卡不是一台连得上的机器 ⇒ 不切当前机器、不把按机器的那几块搬过来（它们会去问一台不存在的机器）。
      if (!isLocal && this.remoteSection?.isUnconfiguredPage(id)) return;
      setCurrentMachine(
        isLocal ? LOCAL_ORIGIN : (this.remoteSection?.originOfPage(id) ?? id.slice(MACHINE_PAGE_PREFIX.length)),
      );
      // 回到机器页 ＝ 展开过的那几块重读一次（照提示在终端里改完回来，不该还是旧的）。
      // 换了机器的由各块自己的订阅重读，这里只管同一台再进来；本机别名块不跟机器走，进本机页就重读。
      if (isLocal) rereadAliases(LOCAL_ORIGIN);
      const page = router.pageContentOf(id);
      if (page) this.movePerMachineTo(page, isLocal, id);
    });

    // 所有 `addRoute` 跑完之后才挂首次可见的钩子，只放行此刻真正 active 的那一页（见 `pageLoaders`）。
    router.onNavigate((id) => this.flushPage(id));
    this.flushPage(router.activeId);

    body.appendChild(router.element);
    return body;
  }

  /** 登记「这一页首次可见时要做的事」。不删条目（放过没有另记在 `pagesLoaded`）：`open()` 要能整体清零，重开时真的重拉。 */
  private loadOnFirstVisit(pageId: string, load: () => void): void {
    const list = this.pageLoaders.get(pageId);
    if (list) list.push(load);
    else this.pageLoaders.set(pageId, [load]);
  }

  /** 构造一块按机器的分节（照常在 `safeBlock` 里），把它的 `loadNow` 留给「机器子页第一次可见」。构造失败 ⇒ 没有 `load`。 */
  private loadableBlock(
    title: string,
    make: () => { element: HTMLElement; loadNow(): void },
  ): { el: HTMLElement; load?: () => void } {
    let sec: { loadNow(): void } | undefined;
    const el = this.safeBlock(title, () => {
      const s = make();
      sec = s;
      return s.element;
    });
    return { el, load: sec ? () => sec!.loadNow() : undefined };
  }

  /** 按机器那几块的第一发：一次打开里只放一次（单例）。 */
  private loadPerMachineOnce(): void {
    if (this.perMachineLoaded) return;
    this.perMachineLoaded = true;
    for (const b of this.perMachineBlocks.filter((x) => x.tab !== "data")) {
      try {
        b.load?.();
      } catch (e) {
        console.warn("[settings] per-machine 分节首次加载抛异常：", e);
      }
    }
  }

  /** 某一页可见了 ⇒ 放出它那几块的第一发 I/O。重复调用不做事。 */
  private flushPage(id: string | null): void {
    if (id === null || this.pagesLoaded.has(id)) return;
    this.pagesLoaded.add(id);
    for (const load of this.pageLoaders.get(id) ?? []) {
      try {
        load();
      } catch (e) {
        // 一块的加载抛异常不能连累同一页上的其它块 —— 同 `safeBlock` 的隔离思路。
        console.warn("[settings] 该页首次可见时的加载抛异常：", e);
      }
    }
  }

  /**
   * 机器页一个都没注册上来 ⇒ 真失败，亮兜底态。两个调用点：构造同步抛；`refresh()` reject（`machinePagesSettled()` 回调）。
   * 不用定时器：「等 N 毫秒没来算失败」会在慢机器上把正常加载判成失败。
   */
  private onMachinePageRegistered(): void {
    this.machinePageRegistered = true;
    this.perMachineFallbackHint.hidden = true;
    this.perMachineSlot.hidden = false;
  }

  private revealPerMachineFallback(why: string): void {
    if (this.machinePageRegistered) return;
    // 后端那几行本该挂在机器列表的行上；列表没建起来 ⇒ 退回列表页（同一个兜底思路）。
    const backendRows = this.backendSection?.element;
    if (backendRows && !backendRows.isConnected) this.perMachineFallbackHint.before(backendRows);
    // 兜底态下这几块就摆在落地的列表页上、用户看得见 ⇒ 这时才放它们的第一发。
    this.loadPerMachineOnce();
    this.perMachineSlot.hidden = false;
    this.perMachineFallbackHint.hidden = false;
    this.perMachineFallbackHint.removeAttribute("aria-busy");
    this.perMachineFallbackHint.dataset.fallback = "per-machine";
    this.perMachineFallbackHint.textContent =
      copyText("settingsPanel.fallback.body", { why });
  }

  /** `RemoteSection` 那趟 `refresh()` 收尾了（成或败）：一个页都没来 ⇒ 兜底。 */
  private onMachinePagesSettled(): void {
    this.machinePagesSettled = true;
    this.applyPendingTarget();
    if (this.machinePageRegistered) return;
    this.revealPerMachineFallback(copyText("settingsPanel.machines.unreadable"));
  }

  /** 一个开关行 ＋ 它下面那一句（存失败时说为什么）。`indent` ＝ 挂在上一项下面（上一项关着时禁用）。 */
  private behaviorSwitch(
    into: HTMLElement,
    label: string,
    help: string | undefined,
    patch: (on: boolean) => Partial<BehaviorConfig>,
    indent = false,
  ): SwitchHandle {
    const row = document.createElement("div");
    row.className = indent ? "settings-switch-row settings-switch-row-indent" : "settings-switch-row";
    const err = document.createElement("div");
    err.className = "settings-row-error";
    err.hidden = true;
    const sw = toggleSwitch({ label, help, on: false, onChange: (on) => this.flipBehavior(patch(on), err) });
    row.append(sw.root, err);
    into.appendChild(row);
    return sw;
  }

  /** 通用页「行为」：五个开关，拨了马上生效、马上存。「同时提到前台」挂在上一项下，上一项关着时禁用。 */
  private buildBehaviorGroup(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group settings-headless";
    this.autoFollowSw = this.behaviorSwitch(group, copyText("settingsPanel.behavior.autoFollow"), undefined, (on) => ({ autoFollowUserActive: on }));
    this.bringFrontSw = this.behaviorSwitch(
      group,
      copyText("settingsPanel.behavior.autoFront"),
      copyText("settingsPanel.behavior.autoFrontHelp"),
      (on) => ({ bringMonitorToFrontOnUserActive: on }),
      true,
    );
    this.showBgSw = this.behaviorSwitch(group, copyText("settingsPanel.behavior.showHiddenTasks"), copyText("settingsPanel.behavior.showHiddenTasksHelp"), (on) => ({
      showBgSessions: on,
    }));
    this.notifyTurnEndSw = this.behaviorSwitch(group, copyText("settingsPanel.behavior.turnNotify"), copyText("settingsPanel.behavior.notifyHelp"), (on) => ({
      notifyTurnEnd: on,
    }));
    this.notifyNeedsSw = this.behaviorSwitch(group, copyText("settingsPanel.behavior.notifyNeeds"), copyText("settingsPanel.behavior.notifyHelp"), (on) => ({
      notifyNeeds: on,
    }));
    return group;
  }

  /**
   * 通用页「恢复」：恢复命令（各台的默认；每台可在「这台上的 cc-monitor」里单独设一格盖过它）＋ 用过的几条 ·
   * 恢复到 tmux 里（历史页［恢复 ▾］与标签页「恢复 ▸」默认的「运行于」）。
   */
  private buildResumeGroup(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group settings-headless";
    const row = document.createElement("label");
    row.className = "settings-row";
    const text = document.createElement("span");
    text.className = "settings-label";
    text.textContent = copyText("settingsPanel.behavior.resume");
    const help = document.createElement("span");
    help.className = "settings-hint";
    help.textContent = copyText("settingsPanel.behavior.resumeHelp");
    const textCol = document.createElement("span");
    textCol.className = "settings-label-col";
    textCol.append(text, help);
    row.appendChild(textCol);
    this.resumeSelect = new ResumeSelect({ inherit: false, onChange: (v) => void this.saveResumeCommand(v) });
    row.appendChild(this.resumeSelect.element);
    group.appendChild(row);
    // 越层启动器诊断：只诊断 ＋ 引导，不自动改这一格。
    this.remoteLauncherWarning = document.createElement("div");
    this.remoteLauncherWarning.className = "settings-launcher-warning";
    this.remoteLauncherWarning.style.display = "none";
    group.appendChild(this.remoteLauncherWarning);
    this.resumeError = document.createElement("div");
    this.resumeError.className = "settings-row-error";
    this.resumeError.hidden = true;
    group.appendChild(this.resumeError);
    // 「终端」那一行（判定在壳的平台层）：通用页第一次可见时问、问回来才画。
    group.appendChild(this.terminalSlot);
    this.resumeInTmuxSw = this.behaviorSwitch(group, copyText("settingsPanel.behavior.resumeInTmux"), copyText("settingsPanel.behavior.resumeInTmuxHelp"), (on) => ({
      resumeInTmux: on,
    }));
    return group;
  }

  /** 「快捷键」一节：［快捷键…］开编辑器（浮在设置之上，首次点开才建）＋ 当前覆盖数。 */
  private buildKeybindingsGroup(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group settings-headless";

    const row = document.createElement("div");
    row.className = "settings-row";
    row.style.gap = "8px";
    row.style.alignItems = "center";

    const openBtn = document.createElement("button");
    openBtn.type = "button";
    openBtn.className = "settings-btn";
    openBtn.textContent = copyText("settingsPanel.keybindings.open");
    openBtn.addEventListener("click", () => {
      if (!this.kbEditor) this.kbEditor = new KeybindingsEditor({ onChange: () => this.refreshKbChip() });
      this.kbEditor.open();
    });
    row.appendChild(openBtn);

    const chip = document.createElement("span");
    chip.className = "settings-kb-chip";
    this.kbOverrideChip = chip;
    this.refreshKbChip();
    row.appendChild(chip);

    group.appendChild(row);
    return group;
  }

  /** 更新覆盖数（open() 时 / 编辑器改过之后）。 */
  private refreshKbChip(): void {
    if (!this.kbOverrideChip) return;
    const n = Object.keys(dispatcher.exportOverrides()).length;
    this.kbOverrideChip.textContent = n > 0 ? copyText("settingsPanel.keybindings.customized", { n }) : copyText("settingsPanel.keybindings.allDefault");
  }

  /** 「文件与数据 → cc-monitor 放了什么」本机那一块「Claude 目录」：一行路径（留空 ＝ 默认）＋［更换…］［用回默认］；存前问在不在、像不像。 */
  private buildDataGroup(): HTMLElement {
    const group = document.createElement("div");
    group.className = "data-card data-claude-dir";
    const row = document.createElement("div");
    row.className = "data-item";
    this.claudeDirInput = document.createElement("input");
    this.claudeDirInput.type = "text";
    this.claudeDirInput.className = "settings-input settings-input-mono data-claude-input";
    this.claudeDirInput.placeholder = copyText("settingsPanel.dataDir.pathHint");
    this.claudeDirInput.spellcheck = false;
    // 全即时：失焦 / 回车（`change`）就落。逐键写盘没意义（路径没打完是个半截串）。
    this.claudeDirInput.addEventListener("change", () => {
      void this.persistClaudeDir().catch((e: unknown) =>
        this.reportSaveFailure(copyText("settingsPanel.save.claudeDir"), e),
      );
    });
    row.appendChild(this.claudeDirInput);
    const resetBtn = document.createElement("button");
    resetBtn.type = "button";
    resetBtn.className = "settings-btn";
    resetBtn.textContent = copyText("settingsPanel.dataDir.reset");
    resetBtn.title = copyText("settingsPanel.dataDir.resetHint");
    resetBtn.addEventListener("click", () => this.resetClaudeDir());
    const pickBtn = document.createElement("button");
    pickBtn.type = "button";
    pickBtn.className = "settings-btn";
    pickBtn.textContent = copyText("settingsPanel.dataDir.browse");
    pickBtn.addEventListener("click", () => void this.pickClaudeDir());
    row.append(resetBtn, pickBtn);
    group.appendChild(row);
    this.claudeDirRestart = document.createElement("div");
    this.claudeDirRestart.className = "diag-restart";
    this.claudeDirRestart.dataset.role = "claude-dir-restart";
    this.claudeDirRestart.hidden = true;
    this.claudeDirRestart.append(icon("warning", "compact"), document.createTextNode(copyText("diagnostics.file.restart")), restartNowButton());
    group.appendChild(this.claudeDirRestart);
    return group;
  }

  private buildGroup(
    title: string,
    fields: ReadonlyArray<FieldSpec>,
  ): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group";
    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = title;
    group.appendChild(heading);
    for (const f of fields) {
      group.appendChild(this.buildField(f));
    }
    return group;
  }

  /**
   * 分区块隔离：构造时抛出 ⇒ 就地渲染「此区块加载失败」，其余区块照常出，不让整个设置窗白屏。每块一个 catch。
   * 收 thunk 而不是 `HTMLElement`：`new Foo()` 作实参在进函数之前就求值，抛了走不到这里的 try。
   * 只挡构造期同步抛；构造期 `void this.loadX()` 的 reject 是未捕获 rejection，靠各块自己的 catch。
   */
  private safeBlock(
    title: string,
    build: () => HTMLElement,
    opts: { untitled?: boolean } = {},
  ): HTMLElement {
    try {
      // `untitled`：这一块独占一个子页、页头已是它的名字 ⇒ 不再自带标题。失败时照旧带标题（得说清是哪一块坏了）。
      const body = build();
      if (!opts.untitled) return this.titledSection(title, body);
      const wrap = document.createElement("div");
      wrap.className = "settings-group";
      wrap.appendChild(body);
      return wrap;
    } catch (e) {
      const wrap = document.createElement("div");
      wrap.className = "settings-group settings-block-failed";
      wrap.dataset.failedBlock = title;
      const heading = document.createElement("div");
      heading.className = "settings-group-title";
      heading.textContent = title;
      wrap.appendChild(heading);
      const msg = document.createElement("div");
      msg.className = "settings-block-failed-msg";
      sayFailure(msg, copyText("settingsPanel.safeBlock.failed"), e);
      wrap.appendChild(msg);
      // 可复制：报障要的是原文，不是转述
      const out = document.createElement("textarea");
      out.readOnly = true;
      out.rows = 3;
      out.className = "settings-input settings-block-failed-out";
      out.value = `[${title}] ${String(e)}`;
      wrap.appendChild(out);
      return wrap;
    }
  }

  private titledSection(title: string, body: HTMLElement): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "settings-group";
    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = title;
    wrap.appendChild(heading);
    wrap.appendChild(body);
    return wrap;
  }

  private buildField(f: FieldSpec): HTMLElement {
    const row = document.createElement("label");
    row.className = "settings-row";

    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = f.label;
    row.appendChild(label);

    const control = this.buildControl(f);
    row.appendChild(control);

    // 单项「恢复默认」：清掉该字段的覆盖，CSS 变量回到 :root 默认
    const resetBtn = document.createElement("button");
    resetBtn.type = "button";
    resetBtn.className = "settings-field-reset";
    resetBtn.appendChild(icon("reset", "compact"));
    resetBtn.title = copyText("settingsPanel.field.resetHint", { label: f.label });
    resetBtn.setAttribute("aria-label", resetBtn.title);
    resetBtn.addEventListener("click", (e) => {
      // row 是 <label>，点击会冒泡到关联的 input ⇒ 阻止默认与冒泡
      e.preventDefault();
      e.stopPropagation();
      this.resetField(f);
    });
    row.appendChild(resetBtn);

    this.inputs.set(f.key, control);
    return row;
  }

  /** 单项重置：清掉覆盖，单 token 应用，重画该 input 的占位值。 */
  private resetField(f: FieldSpec): void {
    delete this.current[f.key];
    applyThemeToken(f.key, undefined);
    this.syncOneInput(f);
    // 单项恢复默认也是改了就落。
    void this.persistTheme().catch((e: unknown) => this.reportSaveFailure(copyText("settingsPanel.save.appearance"), e));
  }

  /** 把单个 token 的当前值（覆盖了的）或 :root 计算值写到对应 input。 */
  private syncOneInput(f: FieldSpec): void {
    const input = this.inputs.get(f.key);
    if (!input) return;
    this.showFieldError(input, null); // 回到存过的值，上一次「没存」那句跟着撤
    const override = this.current[f.key];
    if (override !== undefined && override !== null && override !== "") {
      input.value = String(override);
      return;
    }
    if (f.type === "font-base" || f.type === "font-mono") {
      input.value = "";
      return;
    }
    const computed = getComputedStyle(document.documentElement)
      .getPropertyValue(`--${f.key}`)
      .trim();
    if (f.type === "color") {
      input.value = isShortHex(computed) ? computed : "#000000";
    } else if (f.type === "number") {
      input.value = computed.replace(/px$/, "").trim() || "14";
    } else {
      input.value = computed;
    }
  }

  /** 外观那一格落盘（失败说出来，不静默吞）。 */
  private commitTheme(): void {
    void this.persistTheme().catch((e: unknown) => this.reportSaveFailure(copyText("settingsPanel.save.appearance"), e));
  }

  private buildControl(f: FieldSpec): HTMLInputElement | HTMLSelectElement {
    if (f.type === "font-base" || f.type === "font-mono") {
      const sel = document.createElement("select");
      sel.className = "settings-input settings-input-select";
      const presets =
        f.type === "font-base" ? BASE_FONT_PRESETS() : MONO_FONT_PRESETS();
      for (const p of presets) {
        const opt = document.createElement("option");
        opt.value = p.value;
        opt.textContent = p.label;
        // option 文字本身用对应字体显示
        if (p.value) opt.style.fontFamily = p.value;
        sel.appendChild(opt);
      }
      sel.addEventListener("change", () => {
        this.onFieldChange(f, sel);
        this.commitTheme();
      });
      return sel;
    }
    const input = document.createElement("input");
    input.type = f.type; // color / number / text
    input.className = "settings-input";
    if (f.type === "number") {
      input.min = String(FONT_SIZE_MIN);
      input.max = String(FONT_SIZE_MAX);
    }
    // `input` 只预览（拖取色器 ~60Hz，不逐帧写盘）；`change`（松手 / 失焦 / 回车）才落。
    input.addEventListener("input", () => this.onFieldChange(f, input));
    input.addEventListener("change", () => {
      this.onFieldChange(f, input);
      this.commitTheme();
    });
    return input;
  }

  private makeBtn(
    label: string,
    variant: "primary" | "secondary",
    onClick: () => void,
  ): HTMLElement {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = variant === "primary" ? "settings-btn settings-btn-primary" : "settings-btn";
    btn.textContent = label;
    btn.addEventListener("click", onClick);
    return btn;
  }

  // === 数据同步 ===

  private onFieldChange(
    f: FieldSpec,
    input: HTMLInputElement | HTMLSelectElement,
  ): void {
    const v = input.value;
    let nextValue: string | number | undefined;
    if (v === "") {
      delete this.current[f.key];
      nextValue = undefined;
    } else if (f.type === "number") {
      const n = Number(v);
      if (!Number.isFinite(n) || n < FONT_SIZE_MIN || n > FONT_SIZE_MAX) {
        // 越界：不预览、不存（下次打开显示存过的值），就地说一句。
        this.showFieldError(input, copyText("settingsPanel.field.fontSizeRange"));
        return;
      }
      (this.current as Record<string, unknown>)[f.key] = n;
      nextValue = n;
    } else {
      (this.current as Record<string, unknown>)[f.key] = v;
      nextValue = v;
    }
    this.showFieldError(input, null);
    // 拖取色器时 `input` 约 60Hz：只更新这一个 token，不每帧把整组都设一遍（整棵 :root 子树重算）。
    applyThemeToken(f.key, nextValue);
  }

  /** 一格下面那一行错误：有话说时挂在那一格所在行的后面，没话说时摘掉。 */
  private showFieldError(input: HTMLInputElement | HTMLSelectElement, text: string | null): void {
    let err = this.fieldErrors.get(input);
    if (text === null) {
      err?.remove();
      this.fieldErrors.delete(input);
      input.removeAttribute("aria-invalid");
      return;
    }
    if (!err) {
      err = document.createElement("div");
      err.className = "remote-test-line remote-test-err";
      input.closest(".settings-row")?.after(err);
      this.fieldErrors.set(input, err);
    }
    err.textContent = text;
    input.setAttribute("aria-invalid", "true");
  }

  /** 把 this.current 写回所有 input；无覆盖的字段读 :root 计算值作占位。 */
  private syncInputs(): void {
    for (const f of FIELDS()) {
      this.syncOneInput(f);
    }
  }
}

/** input[type=color] 只接受 #rrggbb；过滤掉 #rgb / rgb() / 字体串。 */
function isShortHex(s: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(s);
}
