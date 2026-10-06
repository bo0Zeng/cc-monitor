/**
 * 设置面板：外观（主题 / 字体）+ 数据目录（Claude 数据位置）+ 行为 / 快捷键 / 远端 / 集成 / 诊断。
 *
 * 解耦：
 *  - 外观：只调 theme.ts 的 applyTheme / themeIn / saveTheme
 *  - 数据目录：只调 paths.ts 的 claudeDirIn / setClaudeDirOverride
 *  - 打开时只 `loadConfig()` 一次，三格各经自己那一家的 `*In(cfg)` 派生
 *
 * F82a（#56+#47）两种承载模式（`windowMode`）：
 *  - **主窗口浮层**（`windowMode:false`，F82a 后当前无调用方，保留供回退 / 未来复用）：抽屉式，
 *    close 隐藏 `.open`、行为改动经 `onBehaviorChange` 同窗直连 TabManager。
 *  - **独立设置窗口**（`windowMode:true`，SS-3 终态）：面板即窗口全部内容；close/cancel 关**窗口**；
 *    保存 / 行为 toggle / resetAll 后 `emit(SETTINGS_APPLIED_EVENT)`，主窗口 listen 后重读并应用
 *    theme+behavior+keybindings（跨 OS 窗口回调够不到）。此模式下会 `import` 并调用 tauri window/event。
 */

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
import { ExtSection } from "./ext-section"; // 顶层「扩展」：跨机器的 skill / MCP，一张表 ＋ 一个抽屉
import { ConfigSurfaceSection } from "./config-surface-section"; // T02：配置面审计（只读、按需一次、不轮询）
import { DriftLedgerSection } from "./drift-ledger-section"; // U-CC1：数据面漂移记账（只读、按需一次、不轮询）
import { RelayOptinSection } from "./relay-optin-section"; // 「终端」栏：直接敲的 claude 也走中转（可选、生成让你贴）
import { DiagnosticsSection } from "./diagnostics-section";
import { makeSkeleton } from "./skeleton";
import { SettingsRouter } from "./router";
import { buildMachinePage, localMeta, type MachinePage } from "./machine-page";
import type { IconName } from "../kit/icon";
// E62：`markRestartNeeded` —— 本文件两处「重启才生效」的改动此前不给常驻条供货。
import { createRestartBar, markRestartNeeded } from "./restart-notice";
import { claudeDirProblem } from "./claude-dir-check";
import { createUnknownKeysBar, rerenderUnknownKeys } from "./unknown-keys-notice"; // 🔴 P12：未知键要出声
import { getCurrentMachine, setCurrentMachine } from "./machine-context";
import { LOCAL_ORIGIN, isLocalOrigin } from "../ipc/origin";
import { toast } from "../kit/toast"; // 行为设置落盘失败出声
import {
  LOCAL_MACHINE_PAGE_ID,
  MACHINE_PAGE_PREFIX,
} from "./remote-section";
import { DataSection } from "./data-section";
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
import { buildAliasManager, buildUnknownOsAliasBlock, localShell, rereadAliases } from "./machine-aliases"; // 机器页 ②「别名」（两个平台一份，含 PowerShell 的终端集成）
import { dispatcher } from "../keybindings/registry";
import { KeybindingsEditor } from "../keybindings/editor";
// F82a：独立设置窗口——保存后广播 `settings-applied`，主窗口 listen 后重读并应用主题/行为
// （跨 OS 窗口无法直接回调）；close/cancel 关闭本窗口。事件名在中立模块 events.ts。
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { BEHAVIOR_TOGGLED_EVENT, SETTINGS_APPLIED_EVENT, type BehaviorToggled } from "./events";
import { confirmDialog } from "../kit/dialog";
import { copyText } from "../copy-table";
import { parseSettingsTarget, type SettingsTarget } from "./open-settings";

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
// 下面这几张表与几段说明都做成函数、用到时才取文：模块顶层一句取文口调用都不留。顶层有调用时，
//   Rollup 把整个设置面板挪进主窗也加载的共享 chunk（entry-graphs「主窗挂得上却没链样式」现打 302 条）。
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
  // 🔴 这里原先有「用户色」「Claude 色」两个取色器。
  //    它们**拖了界面不会有任何变化** —— 链子是通的（一路走到 `theme.ts` 真的把值
  //    写进 DOM 的那一步），但**全仓没有一条 CSS 读这两个变量**
  //    ⚠ 这句话**刻意不写出那个调用的字面形状**：`css-conventions` 的扫描器
  //      按字面认「TS 侧设了哪些自定义属性」，注释里写全了会被它当成一处真调用点
  //      （我第一版就这么写的，当场被那条恒等断言逮住）。
  //    （剥注释后 `var(--user)`/`var(--assistant)` 合计 0 处，`main` 上同样 0，一直如此）。
  //    两条路：接上（让哪些元素跟着走）／撤掉。用户逐字「取色器撤掉」。
  //    ⚠ 撤的是**旋钮**，不是「配色可调」这件事 —— 另外七格（主背景/卡片/主文本/
  //      成功/警告/错误/次文本）都有真消费者，一个没动。
  { key: "success", label: copyText("settingsPanel.field.success"), type: "color", group: "color" },
  { key: "warn", label: copyText("settingsPanel.field.warn"), type: "color", group: "color" },
  { key: "error", label: copyText("settingsPanel.field.error"), type: "color", group: "color" },
];

/** v2.4 issue #2：设置面板回调，行为类 toggle 改变时通知外部（TabManager 同步） */
export interface SettingsPanelOptions {
  onBehaviorChange?: (cfg: BehaviorConfig) => void;
  /** F82a：窗口模式——面板作为**独立设置窗口**的全部内容挂载（非主窗口内浮层）。影响：
   *  不 push overlay 栈（窗口本身就是全部）；close/cancel 关**窗口**而非移除 `.open`；
   *  保存 / 行为 toggle 后 `emit(SETTINGS_APPLIED_EVENT)` 让主窗口重读同步（跨窗回调够不到）。 */
  windowMode?: boolean;
}

// 各模块的 ? 图标 tooltip 文案（原来散在表单里的 .settings-hint 长文本收纳到这里）

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
const MACHINE_TAB_OF_TARGET: Record<string, string> = { acct: "acct", config: "config" };
/** 顶层「扩展」页的路由 id。 */

export class SettingsPanel {
  private el: HTMLElement;
  /** S2：页面路由器。`open()` 每次回落地页（计划指定不记忆上次停在哪一页）。 */
  private router!: SettingsRouter;
  /** S4b-2：跟着当前机器页走的那几块分节（整块搬 DOM，不每台各起一份）。 */
  private perMachineSlot!: HTMLElement;
  /** 步 3：机器列表页上那块**加载态**（不是兜底态）。机器页注册上来就撤掉。 */
  private perMachineFallbackHint!: HTMLElement;
  /** 步 3：机器页到底注册上来过没有 —— 兜底态的**判别式**，不是猜的。 */
  private machinePageRegistered = false;
  private perMachineBlocks: {
    appliesTo: "local" | "remote" | "both";
    /** S4b-3b-2：这块归详情页的哪一栏。`footprint` 是新增的第五栏。 */
    tab: "acct" | "term" | "data";
    el: HTMLElement;
    /**
     * ST1「延后加载」：这一块的第一发 I/O。**某台机器的子页第一次可见时**才调。
     * 构造失败（`safeBlock` 收住）的块没有它。
     */
    load?: () => void;
  }[] = [];
  /** ST1：本次打开以来 per-machine 那几块放过 I/O 没有。`open()` 清零（重开要看新读数）。 */
  private perMachineLoaded = false;
  /**
   * 🔴 第一刀 · 步 2：**「这一页首次可见」才发 I/O。**
   *
   * # 为什么门开在这里，而不是各块自己判
   *
   * 成因链第 ③ 层：14 个块**全部在 `buildBody()` 里即时构造**，
   * 而其中 9 个文件 / 24 处在构造期就 `void this.loadX()`。落地页只有一页
   *（`machines`），于是另外两页的 IPC 是**白发的**，并且正好落在那 3 秒重排窗口里。
   *
   * `router.ts` 头注拒绝过「懒加载」，理由是两条：会引入「某页第一次打开才炸」这类新
   * 失效模式，且 `panel.ts` 那几个 build 时赋值、`open()` 时使用的字段会行为漂移。
   * 🔴 **本机制绕开了那两条**：**不改构造时机**（块照常在 `buildBody` 里 `new` 出来、
   * 字段照常赋值），只把**那一发网络往返**推后。构造仍然在一个地方、仍然被 `safeBlock`
   * 包着；漂不了。
   *
   * # 为什么钩子要在 `buildBody` 跑完之后才挂
   *
   * `SettingsRouter.addRoute` 里有一句「第一页注册完就得有东西可看」——
   * 于是**「应用」在注册的那一瞬间是 active 的**（它排在「机器」前面注册），
   * 等落地页「机器」注册上来才切过去。挂早了，「应用」会被当成「可见过」而当场放行，
   * 这条门就白开了。⇒ 钩子挂在所有 `addRoute` 之后，然后只 flush **此刻真正 active 的那一页**。
   */
  private readonly pageLoaders = new Map<string, Array<() => void>>();
  /** 本次打开以来，哪几页已经放过 I/O 了。`open()` 会清空它（重开要看新读数）。 */
  private readonly pagesLoaded = new Set<string>();
  /** S4b-3b-2：pageId → 该页「账号 / 终端 / 足迹」三栏的容器。 */
  private machineTabSlots = new Map<string, { acct: HTMLElement; term: HTMLElement }>();
  /** 机器页 id → 它的卡头与两栏。 */
  private readonly machinePages = new Map<string, MachinePage>();
  /** 机器页 id → 它的分栏。 */
  /** 还没落地的目的地（那台的页还没注册上来时留着，注册了再落）。 */
  private pendingTarget: SettingsTarget | null = null;
  /** 各台最近一次问到的连接（导航点照它画；机器页后注册的也照它补）。 */
  private readonly channelOf = new Map<string, boolean | null>();
  /** 机器表那一趟读完了（成或败）：还没注册的机器页不会再来了。 */
  private machinePagesSettled = false;
  /** 当前编辑中的 theme（实时预览用） */
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

  /** Claude 数据目录输入框 —— 改动后保存会提示需要重启 */
  private claudeDirInput!: HTMLInputElement;
  /** 打开时 claudeDir 的快照，用于判断是否变化（变了就提示重启） */
  private claudeDirOriginal: string = "";
  /** E62：打开设置时 `showBgSessions` 的值——用来判断「真的改了没」。 */
  private showBgOriginal = true;

  /** 顶部状态提示行（保存成功 / 需重启 等） */
  private banner!: HTMLElement;
  /** issue #3 (A): 「数据位置」展示区（改名）。打开面板时 refresh 一次拉最新 stat */
  private dataSection?: DataSection;
  /** 改名后的「日志」块。步 2 要在「应用」页首次可见时叫醒它。 */
  private logsSection?: DiagnosticsSection;
  /** 顶层「扩展」页那一块（构造失败 ⇒ 留 `undefined`，同 `remoteSection` 那一格的约定）。 */
  private extSection?: ExtSection;
  /** issue #15 (S6): 远端 (SSH) 配置区。打开面板时 refresh 一次拉最新 config */
  private remoteSection?: RemoteSection;
  /** P2s（C8）：backend 开关区。打开面板时 refresh 一次，重拉每台机的状态。 */
  private backendSection?: BackendSection;
  /** 后端报来的各台版本（按后端那套名字）。 */
  private readonly versionOf = new Map<string, string | null>();
  /** 「终端」栏「直接敲的 claude 也走中转」那一块：回到机器页时展开过就重问。 */
  private relayOptin?: RelayOptinSection;

  // v2.4 issue #2: 行为类 toggle
  private autoFollowCheckbox!: HTMLInputElement;
  private showBgCheckbox!: HTMLInputElement;
  private notifyTurnEndCheckbox!: HTMLInputElement;
  private notifyNeedsCheckbox!: HTMLInputElement;
  // F34：自定义 resume 命令（本地 / 远端）
  private resumeLocalInput!: HTMLInputElement;
  private resumeRemoteInput!: HTMLInputElement;
  /** P6c：两格各自的预设条（chip 行）与当前列表。 */
  private resumeLocalPresetsBox!: HTMLElement;
  private resumeRemotePresetsBox!: HTMLElement;
  private resumeLocalPresets: string[] = [];
  private resumeRemotePresets: string[] = [];
  private remoteLauncherWarning!: HTMLElement; // F08：越层启动器诊断提示（只诊断，不代改）
  private bringFrontCheckbox!: HTMLInputElement;
  private onBehaviorChange?: (cfg: BehaviorConfig) => void;
  /** F82a：见 SettingsPanelOptions.windowMode。 */
  private readonly windowMode: boolean;

  /**
   * ST1「关窗改隐藏」（关窗 ＝ 隐藏，不销毁）：本面板亲手把窗口藏起来了。
   * 藏起来之后窗口再拿到焦点 ＝ 被 `open_settings_window` 重新 show 出来 ⇒ 重跑一遍 `open()`
   * （那个「重新打开」事件，这里用窗口自己的 focus 事件代替，不动后端那条命令）。
   */
  private hiddenByUs = false;
  // 原来这里有一格 `closeGuard`（ST1「未保存关窗拦截」那一条）。全即时之后没有「未保存」
  //   这回事了，那一条随混合保存模型一起退场（关窗前还没落的那一格由 `requestClose` 顺手落掉）。

  // issue #5: 快捷键编辑器（lazy 构造，首次打开时建 DOM）
  private kbEditor?: KeybindingsEditor;
  private kbOverrideChip?: HTMLElement;

  constructor(opts: SettingsPanelOptions = {}) {
    this.onBehaviorChange = opts.onBehaviorChange;
    this.windowMode = opts.windowMode ?? false;
    this.el = this.build();
    document.body.appendChild(this.el);
    if (this.windowMode) this.installWindowLifecycle();
    // 新建的窗：壳把目的地放在初始化脚本里。
    this.goTo(parseSettingsTarget((window as { __CCM_SETTINGS_TARGET__?: unknown }).__CCM_SETTINGS_TARGET__));
    // issue #5: Esc 由 KeybindingDispatcher 统一调度。本面板 open 时
    // pushOverlay 自己，close 时 pop —— 多弹层共存按 LIFO 顺序关。
  }

  /** 弹层栈底：设置窗是窗口不是浮层，Esc 不关它（只关压在它上面的那一层）；关窗用 × 或 Ctrl+W。 */
  handleEsc(): void {}

  /**
   * **T07 审计⑤：`safeBlock` 的隔离原先只覆盖生命周期前半。**
   *
   * 这个方法直接解引用 8 个由 `safeBlock` **内部**赋值的 `!` 字段
   * （`autoFollowCheckbox` … `kbOverrideChip`）。某块构造失败时 `safeBlock` 把异常收住了、
   * 面板照常渲染 —— **但那些字段仍是 undefined**，于是 `open()` 在解引用时 reject。
   * 审计实测：让「快捷键」块失败 → 构造被收住，可 `await panel.open()` **reject**、
   * `.settings-panel` 拿不到 `.open` class。也就是说"一块坏、别的还能用"只成立到构造结束。
   *
   * 今天没有活的 throw 源（`registry.ts` 的 `exportOverrides` 不会抛），所以这是
   * **结构性不完整**而非活缺陷 —— 但隔离要么覆盖整个生命周期，要么就不算隔离。
   * 这里包一层：任何一步失败都把面板**照常打开**并在 banner 里说明，而不是让 `open()` reject。
   */
  async open(): Promise<void> {
    try {
      await this.openInner();
    } catch (e) {
      // 面板必须能打开——它是用户唯一的逃生口（里面有"打开 profile"之类的按钮）
      this.banner.textContent = copyText("settingsPanel.open.partialFailed", { e: String(e) });
      this.banner.classList.add("settings-banner-show");
      this.el.classList.add("open");
      this.isOpen = true;
      dispatcher.pushOverlay(this);
    }
  }

  private async openInner(): Promise<void> {
    // 〔骨架优先〕先开窗，再读**一次**配置派生外观 · 数据目录 · 行为三格。
    //   原来是三次串行 `await loadConfig()` 挡在 `open` 之前（成因 ③）。
    this.banner.textContent = "";
    this.banner.classList.remove("settings-banner-show");
    // 🔴 步 2：**重开设置 = 每一页的「首次可见」重新算一遍**，但仍然只拉
    // **用户真看得见的那一页**。原来这里是无条件 `this.dataSection?.refresh()`
    // 与「日志」那两发 —— 那三发在落地页是「机器」的情况下**每次打开都是白发的**，
    // 正是 `§8` 判据 #3（非落地页零 I/O）今天被打破的那一处。
    this.pagesLoaded.clear();
    this.perMachineLoaded = false;
    // issue #15 (S6): 每次打开重拉 config.json 的 remote 子对象，跟外部改动对齐
    // 步 4：`refresh()` 失败时会把原因画到那一块自己的 banner 上；
    // 这里 `catch` 掉是为了不再多产一条走状态栏的未捕获 rejection（同一件事说两遍，
    // 而其中一遍说在了离现场十万八千里的地方）。
    void this.remoteSection?.refresh().catch(() => {});
    // P2s：状态是**运行期**的东西，每次打开都要重拉 —— 缓存住等于给用户看一张旧照片。
    // ⚠ 它住**落地页**（「机器」），所以它不在延后那一档里：打开就该是新的。
    void this.backendSection?.refresh();
    // issue #5: 同步快捷键覆盖数 chip（编辑器关闭时也可能改了）
    this.refreshKbChip();
    // 同一次运行里再打开：停在上次离开的那一页；带目的地的直达那里。
    this.applyPendingTarget();
    // ⚠ `navigate()` 在「已经在这一页」时会**提前返回、不通知订阅者**（同页不重复通知，
    //   那条是对的：订阅者会做搬 DOM 这类有代价的事）。⇒ 第二次打开时落地页的 flush
    //   必须在这里补一刀，否则它只在**第一次**打开时发生过。
    this.flushPage(this.router.activeId);
    // 读回之前三格的控件是 pending（禁用）：勾选框此刻显示的是上一次的值，点上去就点在旧值上。
    this.setConfigCellsPending(true);
    this.el.classList.add("open");
    this.isOpen = true;
    // 面板始终作为 overlay 栈**底**（窗口模式也是）：这样设置窗内的快捷键编辑器 / SFTP 面板
    // 压栈其上时 Esc 走 dispatcher 的 LIFO 逐层关（先关它们、再 Esc 关面板→关窗），
    // 与主窗口抽屉行为一致。窗口模式下面板 handleEsc→cancel→close()→关窗（见 close()）。
    dispatcher.pushOverlay(this);
    try {
      const cfg = await loadConfig();
      this.fillConfigCells(cfg);
      // 「config.json 里有认不出的项」那条跟着这一份重算（窗口是藏起来再开，那条不会自己重建）。
      rerenderUnknownKeys(cfg);
    } catch (e) {
      // 读不回来 ⇒ 三格照旧各回落到缺省（与原来三个读者各自的降级同形），外观不重刷。
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
      this.autoFollowCheckbox,
      this.bringFrontCheckbox,
      this.showBgCheckbox,
      this.notifyTurnEndCheckbox,
      this.notifyNeedsCheckbox,
      this.resumeLocalInput,
      this.resumeRemoteInput,
    ];
    for (const c of controls) c.disabled = pending;
    if (!pending) this.updateBringFrontEnabled();
  }

  /** 从**一份**配置派生三格（派生规则各住 `theme.ts` / `paths.ts` / `behavior.ts`，这里不另写）。 */
  private fillConfigCells(cfg: Record<string, unknown>, applyLoadedTheme = true): void {
    this.original = themeIn(cfg);
    if (applyLoadedTheme) applyTheme(this.original);
    this.current = { ...this.original };
    this.claudeDirOriginal = claudeDirIn(cfg) ?? "";
    this.claudeDirInput.value = this.claudeDirOriginal;
    // v2.4 issue #2: 每次打开拉最新 behavior，避免跟外部其他改动脱节
    const behavior = behaviorIn(cfg);
    this.autoFollowCheckbox.checked = behavior.autoFollowUserActive;
    this.bringFrontCheckbox.checked = behavior.bringMonitorToFrontOnUserActive;
    this.showBgCheckbox.checked = behavior.showBgSessions;
    // E62：记下打开设置时的值，只有**真的改了**才供货（每次 toggle 都标会把噪音变回来）。
    this.showBgOriginal = behavior.showBgSessions;
    this.notifyTurnEndCheckbox.checked = behavior.notifyTurnEnd;
    this.notifyNeedsCheckbox.checked = behavior.notifyNeeds;
    this.resumeLocalInput.value = behavior.resumeCommandLocal;
    this.resumeRemoteInput.value = behavior.resumeCommandRemote;
    // ⚠ `?? []` 不是防 `behaviorIn`（它总会填缺省），是防**这一排 chip 掀翻整个面板**：
    // 实测缺字段时 `renderResumePresets` 抛错 ⇒ `open()` 整个中断 ⇒ 面板停在错误的页。
    // 一个装饰性的候选条不该有那种权力。
    this.resumeLocalPresets = behavior.resumeCommandLocalPresets ?? [];
    this.resumeRemotePresets = behavior.resumeCommandRemotePresets ?? [];
    this.renderResumePresets();
    this.updateRemoteLauncherWarning();
    this.syncInputs();
  }

  /** v2.4 issue #2: autoFollow 关 → bringFront 灰显（依赖前者，无意义） */
  private updateBringFrontEnabled(): void {
    this.bringFrontCheckbox.disabled = !this.autoFollowCheckbox.checked;
  }

  /** F08：越层启动器诊断——只读提示，不碰 `resumeRemoteInput.value` 本身（设计
   *  原则#7：只诊断+引导迁移，不自动降级、不偷改配置）。 */
  private updateRemoteLauncherWarning(): void {
    const msg = diagnoseRemoteLauncher(this.resumeRemoteInput.value);
    this.remoteLauncherWarning.textContent = msg ?? "";
    this.remoteLauncherWarning.style.display = msg ? "block" : "none";
  }

  /**
   * P6c：把两格的预设渲染成可点的 chip。
   *
   * ⚠ **远端那格点完必须走同一条越层诊断**（`updateRemoteLauncherWarning`）。
   * 理由在远端输入框自己的 tooltip 里逐字写着：「别填 `cct` 这类自己建 tmux 的命令」——
   * 手打错一次是一次，**存成预设是把错误固化成一键**。预设是个放大器，
   * 不让它走诊断的话，本件是净减安全性。
   */
  private renderResumePresets(): void {
    const fill = (
      box: HTMLElement,
      list: readonly string[],
      input: HTMLInputElement,
      remote: boolean,
    ): void => {
      box.replaceChildren();
      for (const cmd of list) {
        const wrap = document.createElement("span");
        wrap.className = "settings-preset-wrap";
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "settings-btn settings-preset";
        chip.textContent = cmd;
        chip.title = cmd;
        chip.addEventListener("click", () => {
          input.value = cmd;
          if (remote) this.updateRemoteLauncherWarning();
          void this.onBehaviorToggle();
        });
        // ★ **只进不出是个缺陷**〔D 阶段补审〕：打错一个 `ccmm`，它会一直占着位子，
        // 直到被后来的 12 条挤出去。成例抄 SFTP 书签栏那个 `×`（`sftp/panel.ts:595`）。
        const del = document.createElement("button");
        del.type = "button";
        del.className = "settings-btn settings-preset-del";
        del.textContent = copyText("settingsPanel.preset.remove");
        // ⚠ **正在生效的那条不给移除**：这张表的含义是「用过的命令」，
        // 而移除一条**此刻正在用**的命令是自相矛盾的 —— 保存时它必然又被记回来
        // （`withResumePreset` 会把输入框里的值并进去），用户会看见「点了没反应」。
        // 与其造那种假象，不如当场说清为什么点不了。
        const active = input.value.trim() === cmd;
        del.disabled = active;
        del.title = active
          ? copyText("settingsPanel.preset.inUse", { cmd })
          : copyText("settingsPanel.preset.removeHint", { cmd });
        del.addEventListener("click", () => void this.removeResumePreset(cmd, remote));
        wrap.append(chip, del);
        box.appendChild(wrap);
      }
    };
    fill(this.resumeLocalPresetsBox, this.resumeLocalPresets, this.resumeLocalInput, false);
    fill(this.resumeRemotePresetsBox, this.resumeRemotePresets, this.resumeRemoteInput, true);
  }

  /** P6c：把一条预设从列表里拿掉并落盘。生效中的那条到不了这里（按钮是 disabled 的）。 */
  private async removeResumePreset(cmd: string, remote: boolean): Promise<void> {
    if (remote) this.resumeRemotePresets = this.resumeRemotePresets.filter((x) => x !== cmd);
    else this.resumeLocalPresets = this.resumeLocalPresets.filter((x) => x !== cmd);
    await this.onBehaviorToggle();
  }

  /** v2.4 issue #2: 任一行为 toggle 改 → 立即 save + 通知 TabManager 同步 */
  private async onBehaviorToggle(): Promise<void> {
    this.updateBringFrontEnabled();
    const next: BehaviorConfig = {
      autoFollowUserActive: this.autoFollowCheckbox.checked,
      bringMonitorToFrontOnUserActive: this.bringFrontCheckbox.checked,
      showBgSessions: this.showBgCheckbox.checked,
      resumeCommandLocal: this.resumeLocalInput.value.trim(),
      resumeCommandRemote: this.resumeRemoteInput.value.trim(),
      // P6c：**保存时记一次**，不猜。空值不进（空 = 用默认，不是一条命令）——
      // 那条规则住在 `withResumePreset` 里，这里不重写一遍。
      resumeCommandLocalPresets: withResumePreset(
        this.resumeLocalPresets,
        this.resumeLocalInput.value,
      ),
      resumeCommandRemotePresets: withResumePreset(
        this.resumeRemotePresets,
        this.resumeRemoteInput.value,
      ),
      notifyTurnEnd: this.notifyTurnEndCheckbox.checked,
      notifyNeeds: this.notifyNeedsCheckbox.checked,
    };
    try {
      await setBehavior(next);
      // 存成功了才更新内存里那份并重画 —— 存失败还改了 UI，就是让面板说一件没发生的事。
      this.resumeLocalPresets = next.resumeCommandLocalPresets;
      this.resumeRemotePresets = next.resumeCommandRemotePresets;
      this.renderResumePresets();
      // E62：`showBgSessions` 是**重启生效**的（`behavior.ts` 的字段注释逐字写着：
      // 后端启动时读一次 —— 本地扫描过滤 + 远端 backend `--with-bg`）。改了却不供货，
      // 用户就只能靠记性知道「我刚才改的那个还没生效」。
      if (next.showBgSessions !== this.showBgOriginal) {
        markRestartNeeded(copyText("settingsPanel.behavior.hiddenTasks"));
        this.showBgOriginal = next.showBgSessions;
      }
      this.onBehaviorChange?.(next); // 同窗（主窗口浮层）直接同步 TabManager
      this.broadcastApplied(); // 窗口模式：广播让主窗口 applyBehavior
    } catch (e) {
      console.warn("save behavior failed:", e);
      // 从前只记日志：勾选框已经翻了、盘上没变，界面一句不说（E §3.3）。
      toast(copyText("settings.behavior.saveFailed"), String(e));
    }
  }

  /**
   * ST1：窗口模式下接管两件事 —— 系统标题栏的 X（`onCloseRequested`，一律 `preventDefault`，
   * 交给 [`requestClose`]）与「藏起来之后又被 show 出来」（focus ⇒ 重跑 `open()`）。
   *
   * ⚠ 一旦挂上 close-requested 的监听，Tauri 就不再替我们关窗（它看到 JS 侧有这个监听，就把系统那次关窗拦下来），
   *   放行的动作只剩 `hide()`（`core:window:allow-hide`，`capabilities/default.json`）。
   *   **本面板从不调 destroy / close**：主窗销毁时由后端把本窗一起 destroy
   *   （`lib.rs::windows_to_destroy_after`），否则一个藏着的窗口会把进程吊住。
   * ⚠ 挂监听失败（运行时 / ACL 拒）不许把面板炸穿 —— 与 `safeBlock` 同一个隔离思路；
   *   那时系统 X 退回 Tauri 的默认行为（销毁），只是少了拦截。
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
    this.autoFollowCheckbox.checked = b.autoFollowUserActive;
    this.bringFrontCheckbox.checked = b.bringMonitorToFrontOnUserActive;
    if (!this.autoFollowCheckbox.disabled) this.updateBringFrontEnabled(); // 还在读配置（整组禁用）时不提前放开
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
    const ver = this.versionOf.get(origin) ?? null;
    return ver ? copyText("machinePage.cc.summaryVer", { ver, state }) : state;
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
    spot.scrollIntoView?.({ block: "nearest" });
    spot.classList.add("settings-highlight");
    window.setTimeout(() => spot.classList.remove("settings-highlight"), SETTINGS_HIGHLIGHT_MS);
  }

  /**
   * **还有没落盘的那一格没有。**
   *
   * 全即时之后每一格改了就落（外观：`change`；Claude 数据目录：`change`）。剩下的只有一个窗口：
   * 输入框里改了、焦点还没离开（`change` 还没触发）就关窗 —— 那一格由 `requestClose` 顺手落掉。
   */
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

  /**
   * 系统 X、页头 ×、Esc 都走这里。
   *
   * 🔴 原来有改动时亮一条「保存并关闭 / 丢弃改动 / 继续编辑」（ST1 · `§6` #1）——
   *   那一条存在的前提是「外观要点保存」的混合模型（`§6` #2 逐字：「预览了没保存」就是 #1 的温床）。
   *   全即时之后那个温床没了 ⇒ 拦截条退场；**不静默丢**这一格由「关之前顺手落掉」兑现。
   */
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
        this.banner.textContent = copyText("settingsPanel.close.saveFailed", { e: e instanceof Error ? e.message : String(e) });
        this.banner.classList.add("settings-banner-show");
      });
  }

  /** 把还没触发 `change` 的那一格落掉（外观 ＋ Claude 数据目录，各自只在真的改了时写）。 */
  private async flushPending(): Promise<void> {
    await this.persistTheme();
    await this.persistClaudeDir();
  }

  close(): void {
    // 关闭前 blur 掉面板内仍聚焦的输入框。本面板是 hide（移除 .open class）而非从 DOM
    // 移除，元素留着、焦点不会自动释放。若不 blur，document.activeElement 仍是这个隐藏
    // 输入，单键快捷键守卫（registry.ts::isEditableTarget）会把它当"正在打字"，从而吞掉
    // 所有单键快捷键（h/w/数字… 全部失效）—— 用户配置完远端/外观关掉设置后最典型。
    const active = document.activeElement;
    if (active instanceof HTMLElement && this.el.contains(active))
      active.blur();
    // 窗口模式：ST1「关窗改隐藏」—— 关闭 ＝ 把窗口藏起来，面板状态照非窗口模式那样收好，
    // 下一次 show 出来由 focus 那一路重跑 `open()`（见 `installWindowLifecycle`）。
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
            this.banner.textContent = copyText("settingsPanel.close.failed", { e: String(e) });
            this.banner.classList.add("settings-banner-show");
          });
        });
      return;
    }
    this.el.classList.remove("open");
    this.isOpen = false;
    dispatcher.popOverlay(this);
  }

  /** F82a：窗口模式下广播「设置已应用」，主窗口 listen 后重读并 applyTheme/applyBehavior。
   *  非窗口模式（主窗口内浮层）走 applyTheme/onBehaviorChange 同窗直接生效，无需广播。 */
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

  /**
   * Claude 数据目录改了就落（输入框 `change` / 浏览… / 重置）。
   * 它要重启才生效 ⇒ 照旧给常驻条供货（E62）＋ 当场一条 banner。
   */
  private async persistClaudeDir(): Promise<void> {
    const nextDir = this.claudeDirInput.value.trim();
    if (nextDir === this.claudeDirOriginal) return;
    // 不在的目录不存：照收的话重启后会被悄悄忽略。抛出去由调用方说「没存下：…」。
    const problem = nextDir === "" ? null : await claudeDirProblem(nextDir);
    if (problem !== null) throw new Error(problem);
    await setClaudeDirOverride(nextDir === "" ? null : nextDir);
    this.claudeDirOriginal = nextDir;
    // E62：给 S7 那条常驻条**供货**。这里的 banner 是一次性的（关窗即没），
    // 而「还没生效」是个会一直为真到重启为止的状态 —— 两者不是一回事，都要有。
    markRestartNeeded(copyText("settingsPanel.save.claudeDir"));
    this.banner.textContent =
      copyText("settingsPanel.claudeDir.updated");
    this.banner.classList.add("settings-banner-show");
  }

  /** 落盘失败时说出来（全即时的每一格都走它，不许静默吞）。 */
  private reportSaveFailure(what: string, e: unknown): void {
    this.banner.textContent = copyText("settingsPanel.save.failed", { what, e: e instanceof Error ? e.message : String(e) });
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
      this.banner.textContent = copyText("settingsPanel.claudeDir.pickFailed", { e: String(e) });
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

  /**
   * S4b-2：把那几块 per-machine 分节搬到某一页上，并按「这块对本机/远端有没有意义」显隐。
   *
   * `appendChild` 会把节点从原父节点上摘下来 —— 正是想要的「搬过去」，
   * 所以同一时刻它们只存在于一个页面上，不会有两份。
   */
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
      // S4b-3b-2：分栏页 —— 每块按 `tab` 归到「账号 / 终端 / 足迹」栏里。
      for (const b of movable) slots[b.tab as "acct" | "term"].appendChild(b.el);
      return;
    }
    // 没有分栏（本机页、以及 RemoteSection 挂掉时的兜底落点）→ 整块搬，形态同 S4b-2。
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
        cc.push(this.backendSection.cellsFor(origin, { rows: parts ? [parts.components] : [], trailing: parts?.uninstall }));
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
    this.machineTabSlots.set(pageId, { acct: page.slots.acct, term: page.slots.config });
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
    appearancePage.appendChild(
      this.safeBlock(copyText("settingsPanel.group.keybindings"), () => this.buildKeybindingsGroup()),
    );
    const resetRow = document.createElement("div");
    resetRow.className = "settings-row settings-row-end";
    resetRow.appendChild(
      this.makeBtn(copyText("settingsPanel.appearance.reset"), "secondary", () => void this.resetAll()),
    );
    appearancePage.appendChild(resetRow);

    const generalPage = document.createElement("div");
    generalPage.appendChild(this.safeBlock(copyText("settingsPanel.group.behavior"), () => this.buildBehaviorGroup()));
    generalPage.appendChild(
      this.safeBlock(copyText("contextLimits.editor.title"), () => new ContextLimitsSection().element, { untitled: true }),
    );

    const logsPage = document.createElement("div");
    logsPage.appendChild(
      this.safeBlock(
        copyText("settingsPanel.group.logs"),
        () => {
          const sec = new DiagnosticsSection({ headless: true });
          this.logsSection = sec;
          return sec.element;
        },
        { untitled: true },
      ),
    );

    const dataPage = document.createElement("div");
    dataPage.appendChild(this.buildDataGroup());
    dataPage.appendChild(
      this.safeBlock(
        copyText("settingsPanel.group.dataPlaces"),
        () => {
          const sec = new DataSection({ headless: true });
          this.dataSection = sec;
          return sec.element;
        },
        { untitled: true },
      ),
    );

    // ---- 机器：改**某一台机器**的状态 ----
    //
    // 下面四块（账号 / 别名（含从前的终端集成）/ MCP / cc-bus 钩子）之所以归这里：**它们各自都维护着
    // 一份自己的 origin 选择器**（点名 `accounts`/`mcp`/`cc-bus`/`cc-bus-hooks`
    // 四份互不同步）。有 origin 选择器 = 它改的是某台机器的状态。
    // S4 会把这四份选择器换成「当前在哪台机器页」这个上下文。
    const machinesPage = document.createElement("div");
    // 🔴 **「backend 开关」不再单独占一块**：
    //   它本来就是每台一行，并进机器列表那一行上（状态 / 操作 / 退出行为 / 健康 四格，
    //   `BackendSection.cellsFor`）。列表页从此只剩「列表 ＋ 添加 ＋ 全局开关 ＋ 诊断」四样（`§8` #10）。
    // ⚠ 隔离照旧（T07）：它构造失败不许把机器列表带走 —— 失败时列表行上不挂那四格，
    //   这里亮一块「此区块加载失败」，与 `safeBlock` 同一个样子。
    let backend: BackendSection | undefined;
    try {
      backend = new BackendSection({
        headless: true,
        hosted: true,
        onLinkSeen: (origin) => this.remoteSection?.noteLedgerChanged(origin),
        onChannel: (origin, connected) => this.paintNavDot(origin, connected),
      });
      this.backendSection = backend;
    } catch (e) {
      machinesPage.appendChild(
        this.safeBlock(copyText("settingsPanel.group.backend"), () => {
          throw e;
        }),
      );
    }
    // **T07 审计阻塞 1**：这里必须在 `safeBlock` 里——`RemoteSection` 正是唯一活的同步
    // throw 宿主（构造路径含 `remote-section.ts` 那个三句话必填的 `throw`）。审计真造它抛过：
    // 裸构造会让 `new SettingsPanel` 直接炸穿、**什么都没上屏**。
    // `this.remoteSection` 失败时留 `undefined`——`open()` 那边是 `?.refresh()`，天然容错。
    machinesPage.appendChild(
      this.safeBlock(copyText("settingsPanel.group.remote"), () => {
        // S4b：把路由器包成 `MachinePagesHost` 交给它 —— 每台机器的编辑表单去它自己那一页，
        // 列表里只留一行。分节不需要知道路由器长什么样，只要「开页 / 收页 / 跳过去」。
        const sec = new RemoteSection({
          headless: true,
          rowExtras: backend && {
            // 后端清单里有、机器列表里没有的那几台（重名被后缀化 / 列表还没读出来）。
            tail: () => backend.element,
          },
          pages: {
            machinePagesSettled: () => this.onMachinePagesSettled(),
            addMachinePage: (id, title, element, parts) => {
              // 步 3：**判别式**——机器页真注册上来了，兜底态就不该出现。
              // ⚠ 那两句 `.hidden =` 刻意住在 `onMachinePageRegistered()` 里而不是这里：
              //   `css-conventions.vitest.ts` 的 S30 ⑦ 那把尺子只往**上游**找类名赋值，
              //   而 `perMachineSlot.className = …` 在本文件里排在这一行**之后** ⇒
              //   写在这儿会让那处变成「静态推不出它挂的是哪个类」，判据就看不见它了。
              this.onMachinePageRegistered();
              // 步 2 + 步 14a：机器子页是**动态注册**的，登记表只能在这一刻填。
              // ⚠ 用 `set` 不是 `push`：`rebuildCards` 每次 refresh 都会重注册一遍同一批
              //   页 id，push 会让同一发 I/O 排队攒到 N 份。
              // ST1「延后加载」：per-machine 那几块是**单例**、跟着机器子页搬 ⇒ 任意一台机器的
              //   子页第一次可见时放一次（`loadPerMachineOnce` 自己去重）；之后切机器由各块
              //   自己的 `subscribeMachine` 重读。
              // 「足迹」也并进那一批单例：它今天按机器去问（远端也有真栏），
              //   切机器由它自己的订阅重读 —— 与账号 / MCP 同一个形状，不再每台机器页各放一发。
              this.pageLoaders.set(id, [() => this.loadPerMachineOnce()]);
              // ★ S4b-3b-2：远端机器页拆成横向四栏。
              // 复用 `SettingsRouter`（横向 + 无页头）而不是另造 tab 原语 ——
              // 「同一时刻只有一栏可见 + aria + 方向键 + 不重复注册」与左侧导航
              // 逐条相同，另造等于把这套逻辑抄第二遍、然后各自漂。
              void element;
              const pageEl = this.buildMachinePage(id, title, parts).element;
              router.addRoute({ id, title, element: pageEl, parentId: PAGE.machines, hideHead: true });
              // S4b-2：本机页一出现就让那几块 per-machine 分节先落在它上面。
              // 这与 `machine-context` 的初始值（本机 = `LOCAL_ORIGIN`）对齐 —— 否则 slot 在
              // 用户第一次点进某台机器之前是**游离的**（不在文档里，谁也找不到它）。
              if (id === LOCAL_MACHINE_PAGE_ID) this.movePerMachineTo(pageEl, true, id);
              const origin = id === LOCAL_MACHINE_PAGE_ID ? LOCAL_ORIGIN : this.remoteSection?.originOfPage(id);
              if (origin) this.paintNavDot(origin, this.channelOf.get(origin) ?? null);
              this.applyPendingTarget();
            },
            removeMachinePage: (id) => {
              router.removeRoute(id);
              this.machinePages.delete(id);
              this.machineTabSlots.delete(id);
              // 机器没了，它那一页的登记与「放过了没有」一起清 —— 留着就是死条目，
              // 而且同名机器再出现时会被当成「这一页已经放过了」。
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
    // ★ S4b-2：这四块**不再挂在列表页上**，而是跟着「当前在看哪台机器」走 ——
    // 它们讲的本来就是某一台机器的事（S4a 已把四份各自为政的 origin 选择器收口）。
    //
    // 单例 + 按页搬 DOM，不给每台机器各起一份：起 N 份意味着 N 倍的构造开销与
    // N 份互不相干的缓存，而同一时刻只有一台机器的页面是可见的。
    //
    // `appliesTo` 决定它在本机页/远端页出不出现。这就是 S4a 登记的那个半截状态的解药：
    // 那时三块的下拉只列远端、表示不了本机，收到 `null` 只能原地不动；
    // 现在**本机页上它们压根不出现**，「表示不了」这件事也就不存在了。
    this.perMachineSlot = document.createElement("div");
    this.perMachineSlot.className = "machine-page-sections";
    this.perMachineBlocks = [
      {
        // 账号这一节**两页都有意义** —— 每台机器的账号归那台机器（用户 09-05 拍的板）。
        //
        // ⚠ 这里原先逐字写着「账号列表是 per-origin 的**远端**概念（本机的多账号入口是
        // L3a 的欠账，见 BACKLOG）」并登记成 `appliesTo: "remote"`。那句话**已经不成立**：
        // 本机那条读口（`accounts.ts` 的 `fetchLocalAccounts` → `list_local_accounts`）
        // 自 `a354c83` 起就在盘上，状态栏那个账号 chip 一直在用它；`N-F1b` 之后
        // `AccountsSection.reload()` 在 `origin` 为空时也走本机那一支了。
        //
        // 🔴 而 `appliesTo: "remote"` 是**第二道锁**：`movePerMachineTo` 会把不匹配的整块
        // `hidden` 掉 ⇒ 本机页上这一节是黑的，而**一台没有配任何远端的机器只有本机页**
        // ⇒ 那一节渲染得再对，新用户也看不见它（定框 `G1` 成功标准第 1 条卡在这一行上）。
        // 两条判据分工：那一节**渲染成什么样**归 `accounts-section.vitest.ts`；
        // 它在**哪一页上看得见**归 `panel-machine-page-visibility.vitest.ts`
        // —— 后者从 `SettingsPanel` 这一头进，断的是渲染之后的 `el.hidden`，
        // 因为前者在 jsdom 里直接 `new AccountsSection()`，结构性地绕过了这一层。
        appliesTo: "both",
        tab: "acct",
        ...this.loadableBlock(copyText("settingsPanel.group.accounts"), () => new AccountsSection()),
      },
      // 这里原来是本机页上单独一块「终端集成」（PowerShell `$PROFILE` 注入，S9 只在 Windows 上构造）。
      // 它并进了下面「别名」那一块（界面合成一份，平台是它的一个输入）——
      // Windows 上是 PowerShell 那一侧的「别名块」，第一次展开才构造（构造即发 Windows 专用 IPC 那条纪律跟着过去了）。
      // 机器页 ②「别名」。本机那一格在这里；远端那一格在
      // `MachineCard` 的「组件」栏里（它只给得出待贴文本 ＋ 装 / 卸别名块，理由见那边）。
      // 构造零 I/O：它是个 `<details>`，第一次展开才读盘。
      {
        appliesTo: "local",
        tab: "term",
        el: this.safeBlock(copyText("settingsPanel.group.aliases"), () => {
          // 认不出本机系统 ⇒ 不猜方言：明说、安装入口置灰。
          const shell = localShell();
          return shell === null
            ? buildUnknownOsAliasBlock()
            : buildAliasManager({
                platform: shell,
                origin: () => LOCAL_ORIGIN,
              });
        }),
      },
      // 机器页「终端」栏：让直接敲的 claude 也走中转（可选、生成让你贴）。本机远端同一块，跟当前机器走；
      //   构造零 I/O，第一次展开才问那台（要贴的那一段带着中转钥匙，不展开不进界面）。
      {
        appliesTo: "both",
        tab: "term",
        el: this.safeBlock(
          copyText("relayOptin.section.title"),
          () => {
            const sec = new RelayOptinSection();
            this.relayOptin = sec;
            return sec.element;
          },
          { untitled: true },
        ),
      },
      // 〔资产目录 · 插件〕三块搬走了：skill / MCP 是跨机器的一类对象，住顶层「扩展」页（一张表 ＋ 一个抽屉）；
      //   插件只读列表没有可做的事，先拿掉。
      // cc-bus 钩子那一块拿掉了：cc-bus 是扩展页里的一行，它的内置备注下面每台一行钩子状态与要加的内容。
      // 🔴 步 14a（那张图的第五栏）：**「足迹」**。
      //
      // 它原来住顶层「改动足迹」页，名字叫「配置面审计」。两条都改：
      // ① **住哪**：它答的是「装了 cc-monitor 之后，它在**我这台机器**上动过哪些文件」
      //    —— 被设置的对象是**一台机器**，按 `panel.ts` 那条顶层判据（每个顶层 = 一类
      //    「被设置的对象」）它归**机器子页**，不归顶层页。
      // ② **叫什么**：「配置面审计」→「足迹」。⚠ **诚实标注：这不是 R5 命中**
      //    （「配置面审计」本来就是名词短语，R5 抓不到它）。理由是另外两条：
      //    「面」「审计」是**我们这侧**的词；且它要和顶层页
      //    「改动足迹」同一个口径，而不是页叫足迹、块叫审计。
      //
      // 远端也有真栏：`appliesTo: "both"`：远端那一页上它**按那台机器去问**
      //   （读口归 RM1a）；答复的 `origin` 与所问对不上 ⇒ 说这台还答不了，不拿本机的答案冒充
      //   （`ConfigSurfaceSection.readFootprint` / `answersFor` 头注）。
      {
        appliesTo: "both",
        tab: "data",
        // 字段 `footprintSection` 删了：它唯一的读者（每台机器页各放一发）随上面那次合批一起没了。
        ...this.loadableBlock(copyText("settingsPanel.group.footprint"), () => new ConfigSurfaceSection()),
      },
      // 〔协调方转「改动足迹并进机器页、漂移记账按机器分」〕原顶层「改动足迹」页剩下的那一块。
      //   与足迹同栏：两块答的都是「这台机器上发生了什么」。账按机器分了：每台问自己那一本、
      //   回声对上才画（形状与理由在 `drift-ledger-section.ts` 头注「按机器分」一节）。
      {
        appliesTo: "both",
        tab: "data",
        ...this.loadableBlock(copyText("settingsPanel.group.unknown"), () => new DriftLedgerSection()),
      },
    ];
    for (const b of this.perMachineBlocks) (b.tab === "data" ? dataPage : this.perMachineSlot).appendChild(b.el);
    // ★ 兜底落点：先挂在列表页上。
    //
    // 这不是"顺手"—— 它是 `safeBlock` 隔离的一部分。`RemoteSection` 是唯一活的同步
    // throw 宿主（T07 审计阻塞 1）；它挂掉就没有任何机器页被注册，slot 便无处安放，
    // **这五块会一起从界面上消失**。那等于「一块坏，六块没」，把 T07 好不容易建立的
    // 隔离又打破了。有兜底落点的话，最坏情况只是它们留在列表页上 —— 位置不理想，
    // 但都还在、都能用。（审计时真造 RemoteSection 抛才发现的。）
    //
    // 🔴 步 3：**但它今天是每次打开的前 3 秒的默认视图。**
    //
    // 安全网本身没错（它防的是真问题）。错的是**没有第三态**：
    //   今天：  兜底态（= 失败态）→ 正常态
    //   该有：  骨架/加载态        → 正常态
    //                              └→ 兜底态（**只在真失败时**）
    // 机器列表是异步加载的（`RemoteSection.refresh()` 里才 `addMachinePage`），
    // 于是那个「RemoteSection 抛异常时的最坏情况」变成了**每次打开都先看一眼**的样子
    // —— 用户截图 1 里那个「账号（只有一个刷新按钮）」「POWERSHELL 集成」就是它。
    //
    // ⇒ slot 默认**藏起来**，屏幕上先给一块骨架；机器页注册上来的那一刻骨架撤掉、
    //   slot 被 `movePerMachineTo` 搬走。**只有在 `RemoteSection` 真的挂了**
    //   （构造同步抛 ⇒ `this.remoteSection` 留 `undefined`）才当场把 slot 亮出来。
    //
    // ⚠ 还有一条**异步**的失败路：`RemoteSection` 构造成功、但它那趟 `refresh()`
    //   reject（`readRemoteConfig()` 失败）⇒ 一个机器页都不会注册。那一档由
    //   `machinePagesSettled()` 回调兜（`remote-section.ts` 那侧在 `refresh()` 的
    //   `finally` 里叫它），**不是靠定时器猜**。
    this.perMachineSlot.hidden = true;
    this.perMachineFallbackHint = makeSkeleton(
      "backend",
      copyText("settingsPanel.machines.loading"),
    );
    machinesPage.appendChild(this.perMachineFallbackHint);
    machinesPage.appendChild(this.perMachineSlot);
    // 步 3：`safeBlock` 收住了同步抛 ⇒ `this.remoteSection` 留 `undefined`。
    // **那正是「机器页注册失败」的判别式**（`RemoteSection` 是唯一活的同步 throw 宿主，
    // 也是唯一会注册机器页的人）。这一档就是设计意图里的「最坏情况」，当场亮兜底。
    // ⚠ 这一句必须排在 slot 与那块提示**建出来之后** —— 排在 `safeBlock` 紧后面的话，
    //   它会去读两个还没赋值的字段，把「RemoteSection 挂了」变成「整个面板挂了」。
    if (!this.remoteSection) this.revealPerMachineFallback(copyText("settingsPanel.machines.buildFailed"));
    router.addRoute({
      id: "machines",
      title: copyText("settingsPanel.nav.machines"),
      element: machinesPage,
      icon: NAV_ICON.machines,
      headActions: this.remoteSection?.headActions() ?? [],
    });
    router.addRoute({ id: PAGE.data, title: copyText("settingsPanel.nav.data"), element: dataPage, icon: NAV_ICON.data, gapBefore: true });
    this.loadOnFirstVisit(PAGE.data, () => {
      this.dataSection?.loadNow();
      for (const b of this.perMachineBlocks) if (b.tab === "data") b.load?.();
    });

    // ---- 扩展：改**跨机器的 skill / MCP**（一类被设置的对象，不挂在某台机器下面）----
    //   每次变可见都重读一趟（第一问算「来看了一次」，「新」按上一次来看算）；构造零 I/O。
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
    router.addRoute({ id: PAGE.logs, title: copyText("settingsPanel.nav.logs"), element: logsPage, parentId: PAGE.general });
    this.loadOnFirstVisit(PAGE.logs, () => this.logsSection?.loadNow());
    router.onNavigate((id) => {
      if (id === PAGE.ext) this.extSection?.loadNow();
    });

    // 🔴 **顶层「改动足迹」页没了。**
    //   它剩下的那一块（漂移记账）在**每台机器子页的「足迹」栏**里（per-machine 那一批，见上面
    //   `perMachineBlocks` 最后一格）。按机器分那一半的设计在（写区外）。

    // S6：cc-bus 驾驶舱**已搬出设置**，成为顶层运营视图（入口 = 命令面板）。
    // S2 当初把它临时单列成一页，正是为了这一刻只删这一段注册 —— 兑现了。
    // 视图本体见 `views/cc-bus-view.ts`；驾驶舱组件 `CcBusSection` 原样复用，未重写。


    // ★ S4b-2：切到某台机器页时，把那几块 per-machine 分节搬进那一页，并同步
    // 「当前在看哪台机器」。挂在**路由器**这一层而不是列表行上，是因为切页有两个入口
    // （点导航项 / 点列表行），只有这里两条都覆盖得到。
    router.onNavigate((id) => {
      if (!id.startsWith(MACHINE_PAGE_PREFIX)) return;
      const isLocal = id === LOCAL_MACHINE_PAGE_ID;
      // 页 id 建卡时定死、不跟着改名走 ⇒ 这页讲的是哪台问机器列表（改过名的是新名；刚导入的是它的别名）。
      // 还没填主机地址的卡不是一台连得上的机器 ⇒ 不切当前机器、不把按机器的那几块搬过来（它们会去问一台不存在的机器）。
      if (!isLocal && this.remoteSection?.isUnconfiguredPage(id)) return;
      const before = getCurrentMachine();
      setCurrentMachine(
        isLocal ? LOCAL_ORIGIN : (this.remoteSection?.originOfPage(id) ?? id.slice(MACHINE_PAGE_PREFIX.length)),
      );
      // 回到机器页 ＝ 展开过的那几块重读一次（照提示在终端里改完回来，不该还是旧的）。
      // 换了机器的由各块自己的订阅重读，这里只管同一台再进来；本机别名块不跟机器走，进本机页就重读。
      if (isLocal) rereadAliases(LOCAL_ORIGIN);
      if (getCurrentMachine() === before) this.relayOptin?.rereadIfOpened();
      const page = router.pageContentOf(id);
      if (page) this.movePerMachineTo(page, isLocal, id);
    });

    // 🔴 步 2 的门闩：**所有 `addRoute` 都跑完之后**才挂 flush 钩子，然后只放行
    // 此刻真正 active 的那一页。挂早了，「应用」会因为 `addRoute` 里那句
    // 「第一页注册完就得有东西可看」而被当成可见过 —— 这条门就白开了。
    router.onNavigate((id) => this.flushPage(id));
    this.flushPage(router.activeId);

    body.appendChild(router.element);
    return body;
  }

  /**
   * 步 2：登记「这一页首次可见时要做的事」。
   *
   * ⚠ 登记表**不删条目**（`flushPage` 用一个单独的 `pagesLoaded` 记「放过了没有」）：
   * `open()` 要能把「放过了没有」整体清零，让重开一次设置真的重拉一遍读数。
   * 删条目的话第二次打开就永远拉不到新数了 —— 而界面上看不出来，它只是显示旧值。
   */
  private loadOnFirstVisit(pageId: string, load: () => void): void {
    const list = this.pageLoaders.get(pageId);
    if (list) list.push(load);
    else this.pageLoaders.set(pageId, [load]);
  }

  /**
   * ST1「延后加载」：构造一块 per-machine 分节（照常在 `safeBlock` 里），并把它的 `loadNow`
   * 挂出来留给「机器子页第一次可见」那一刻。构造失败 ⇒ 没有 `load`（与 `remoteSection` 那格同一约定）。
   */
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

  /** ST1：per-machine 那几块的第一发。一次打开里只放一次（它们是单例，不按机器各起一份）。 */
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

  /** 步 2：某一页可见了 —— 把它那几块的第一发 I/O 放出去。重复调用是 no-op。 */
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
   * 步 3：机器页一个都没注册上来 ⇒ 这是**真失败**，把兜底态亮出来。
   *
   * 两个调用点，对应两条失败路：
   * ① `RemoteSection` 构造同步抛（`safeBlock` 收住 ⇒ 字段留 `undefined`）；
   * ② 构造成功、但 `refresh()` 那趟 reject（`readRemoteConfig()` 失败）——
   *    由 `machinePagesSettled()` 从 `remote-section.ts` 那侧回调过来。
   *
   * ⚠ **不是定时器**。「等 N 毫秒还没来就算失败」会在慢机器上把正常加载判成失败，
   * 而它出错的方向正好是本件要治的那一个（让兜底态提前露脸）。
   */
  private onMachinePageRegistered(): void {
    this.machinePageRegistered = true;
    this.perMachineFallbackHint.hidden = true;
    this.perMachineSlot.hidden = false;
  }

  private revealPerMachineFallback(why: string): void {
    if (this.machinePageRegistered) return;
    // 后端那几行本该挂在机器列表的行上（列表的尾巴）。列表没建起来 ⇒ 它们无处安放，
    //   **退回列表页上**（与下面那几块同一个兜底思路：位置不理想，但都还在、都能用）。
    const backendRows = this.backendSection?.element;
    if (backendRows && !backendRows.isConnected) this.perMachineFallbackHint.before(backendRows);
    // ST1「延后加载」：兜底态下这几块就摆在（落地的）列表页上、用户看得见 ⇒ 这时才放它们的第一发。
    this.loadPerMachineOnce();
    this.perMachineSlot.hidden = false;
    this.perMachineFallbackHint.hidden = false;
    this.perMachineFallbackHint.removeAttribute("aria-busy");
    this.perMachineFallbackHint.dataset.fallback = "per-machine";
    this.perMachineFallbackHint.textContent =
      copyText("settingsPanel.fallback.body", { why });
  }

  /** 步 3：`RemoteSection` 那趟 `refresh()` 收尾了（成或败）。一个页都没来 ⇒ 兜底。 */
  private onMachinePagesSettled(): void {
    this.machinePagesSettled = true;
    this.applyPendingTarget();
    if (this.machinePageRegistered) return;
    this.revealPerMachineFallback(copyText("settingsPanel.machines.unreadable"));
  }

  /**
   * v2.4 issue #2: 「行为」分组——自动切 tab + 可选拉前 monitor 窗口。
   *
   * 两个 toggle 都是热更新（不重启）。"拉前窗口" 在 "自动切 tab" 关闭时灰显
   * （前者依赖后者，单独开没意义）。
   */
  private buildBehaviorGroup(): HTMLElement {
    // CollapsibleGroup 接管标题与描述（infoTooltip），这里只产出表单本体
    const group = document.createElement("div");
    group.className = "settings-group settings-headless";

    // 1. 自动切 tab
    const autoRow = document.createElement("label");
    autoRow.className = "settings-row settings-row-checkbox";
    this.autoFollowCheckbox = document.createElement("input");
    this.autoFollowCheckbox.type = "checkbox";
    this.autoFollowCheckbox.className = "settings-checkbox";
    this.autoFollowCheckbox.addEventListener(
      "change",
      () => void this.onBehaviorToggle(),
    );
    autoRow.appendChild(this.autoFollowCheckbox);
    const autoLabel = document.createElement("span");
    autoLabel.className = "settings-checkbox-label";
    autoLabel.textContent = copyText("settingsPanel.behavior.autoFollow");
    autoRow.appendChild(autoLabel);
    group.appendChild(autoRow);

    // 2. 拉前 monitor 窗口
    const frontRow = document.createElement("label");
    frontRow.className = "settings-row settings-row-checkbox";
    this.bringFrontCheckbox = document.createElement("input");
    this.bringFrontCheckbox.type = "checkbox";
    this.bringFrontCheckbox.className = "settings-checkbox";
    this.bringFrontCheckbox.addEventListener(
      "change",
      () => void this.onBehaviorToggle(),
    );
    frontRow.appendChild(this.bringFrontCheckbox);
    const frontLabel = document.createElement("span");
    frontLabel.className = "settings-checkbox-label";
    frontLabel.textContent = copyText("settingsPanel.behavior.autoFront");
    frontRow.appendChild(frontLabel);
    group.appendChild(frontRow);

    // 3. Batch7-F24：显示 bg 后台任务会话
    const bgRow = document.createElement("label");
    bgRow.className = "settings-row settings-row-checkbox";
    this.showBgCheckbox = document.createElement("input");
    this.showBgCheckbox.type = "checkbox";
    this.showBgCheckbox.className = "settings-checkbox";
    this.showBgCheckbox.addEventListener(
      "change",
      () => void this.onBehaviorToggle(),
    );
    bgRow.appendChild(this.showBgCheckbox);
    const bgLabel = document.createElement("span");
    bgLabel.className = "settings-checkbox-label";
    bgLabel.textContent =
      copyText("settingsPanel.behavior.showHiddenTasks");
    bgRow.appendChild(bgLabel);
    group.appendChild(bgRow);

    // 4. Batch14-F42：turn-end 系统通知
    const notifyRow = document.createElement("label");
    notifyRow.className = "settings-row settings-row-checkbox";
    this.notifyTurnEndCheckbox = document.createElement("input");
    this.notifyTurnEndCheckbox.type = "checkbox";
    this.notifyTurnEndCheckbox.className = "settings-checkbox";
    this.notifyTurnEndCheckbox.addEventListener(
      "change",
      () => void this.onBehaviorToggle(),
    );
    notifyRow.appendChild(this.notifyTurnEndCheckbox);
    const notifyLabel = document.createElement("span");
    notifyLabel.className = "settings-checkbox-label";
    notifyLabel.textContent = copyText("settingsPanel.behavior.turnNotify");
    notifyRow.appendChild(notifyLabel);
    group.appendChild(notifyRow);

    // 4b. 需要你 · 系统通知（默认开、只在主窗口不在前台时发；与上一格分开）
    const needsRow = document.createElement("label");
    needsRow.className = "settings-row settings-row-checkbox";
    this.notifyNeedsCheckbox = document.createElement("input");
    this.notifyNeedsCheckbox.type = "checkbox";
    this.notifyNeedsCheckbox.className = "settings-checkbox";
    this.notifyNeedsCheckbox.addEventListener("change", () => void this.onBehaviorToggle());
    needsRow.appendChild(this.notifyNeedsCheckbox);
    const needsLabel = document.createElement("span");
    needsLabel.className = "settings-checkbox-label";
    needsLabel.textContent = copyText("settingsPanel.behavior.notifyNeeds");
    needsRow.appendChild(needsLabel);
    group.appendChild(needsRow);

    // 5. F34：自定义 resume 启动命令（历史浏览器 ↺ 用）。change 事件（失焦/回车）保存，
    //    避免逐键写盘。本地命令后端有防注入校验（仅字母数字 -_. 空格）。
    const mkResumeRow = (
      labelText: string,
      placeholder: string,
      titleText: string,
    ): [HTMLElement, HTMLInputElement] => {
      const row = document.createElement("label");
      row.className = "settings-row";
      row.title = titleText;
      const span = document.createElement("span");
      span.className = "settings-label";
      span.textContent = labelText;
      row.appendChild(span);
      const input = document.createElement("input");
      input.type = "text";
      input.className = "settings-input";
      input.placeholder = placeholder;
      input.addEventListener("change", () => void this.onBehaviorToggle());
      row.appendChild(input);
      return [row, input];
    };
    const [localRow, localInput] = mkResumeRow(
      copyText("settingsPanel.behavior.localResume"),
      copyText("settingsPanel.behavior.localResumeHint"),
      copyText("settingsPanel.behavior.localResumeInfo"),
    );
    this.resumeLocalInput = localInput;
    group.appendChild(localRow);
    this.resumeLocalPresetsBox = document.createElement("div");
    this.resumeLocalPresetsBox.className = "settings-presets resume-presets-local";
    group.appendChild(this.resumeLocalPresetsBox);
    const [remoteRow, remoteInput] = mkResumeRow(
      copyText("settingsPanel.behavior.remoteResume"),
      copyText("settingsPanel.behavior.remoteResumeHint"),
      copyText("settingsPanel.behavior.remoteResumeInfo"),
    );
    this.resumeRemoteInput = remoteInput;
    group.appendChild(remoteRow);
    this.resumeRemotePresetsBox = document.createElement("div");
    this.resumeRemotePresetsBox.className = "settings-presets resume-presets-remote";
    group.appendChild(this.resumeRemotePresetsBox);
    // F08：越层启动器诊断——只诊断+引导，不自动改这个输入框的值（设计原则#7）。
    this.remoteLauncherWarning = document.createElement("div");
    this.remoteLauncherWarning.className = "settings-launcher-warning";
    this.remoteLauncherWarning.style.display = "none";
    group.appendChild(this.remoteLauncherWarning);
    remoteInput.addEventListener("input", () =>
      this.updateRemoteLauncherWarning(),
    );
    // 这里原来挂着两块别名（「按账号生成命令」·「生成自定义别名」）。
    // 它们合成一类、搬去了机器页「本机 → 终端 → 别名」：
    // 别名是**每台机器一份**的东西，按本面板那条顶层判据（它改的是谁的状态）归机器页，不归「应用」。
    // 上面那条越层诊断的提示文案跟着指过去了。

    return group;
  }

  /**
   * issue #5: 「快捷键」分组——只放一行说明 + [打开快捷键编辑器] 按钮 + 当前覆盖数 chip。
   *
   * 编辑器是 modal overlay (kb-editor-overlay)，点开后浮在设置面板之上。
   * lazy 实例化：首次点开时建 DOM，后续 reuse 单例避免重复构建。
   */
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
      if (!this.kbEditor) this.kbEditor = new KeybindingsEditor();
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

  /** 更新覆盖数 chip。open() 时 / 编辑器关闭时（如果需要）调 */
  private refreshKbChip(): void {
    if (!this.kbOverrideChip) return;
    const n = Object.keys(dispatcher.exportOverrides()).length;
    this.kbOverrideChip.textContent = n > 0 ? copyText("settingsPanel.keybindings.customized", { n }) : copyText("settingsPanel.keybindings.allDefault");
  }

  /** "Claude 数据目录" 子表单（F82b 起嵌在「集成」组里） */
  private buildDataGroup(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group";

    // 子标题（跟同分组里的「PowerShell 集成」子标题对称）
    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = copyText("settingsPanel.dataDir.title");
    group.appendChild(heading);

    // 行 1：标签 + 文本输入
    const row1 = document.createElement("div");
    row1.className = "settings-row settings-row-stack";
    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = copyText("settingsPanel.dataDir.path");
    row1.appendChild(label);
    this.claudeDirInput = document.createElement("input");
    this.claudeDirInput.type = "text";
    this.claudeDirInput.className = "settings-input settings-input-wide";
    this.claudeDirInput.placeholder = copyText("settingsPanel.dataDir.pathHint");
    // 全即时：失焦 / 回车（`change`）就落。逐键写盘没意义（路径没打完是个半截串）。
    this.claudeDirInput.addEventListener("change", () => {
      void this.persistClaudeDir().catch((e: unknown) =>
        this.reportSaveFailure(copyText("settingsPanel.save.claudeDir"), e),
      );
    });
    row1.appendChild(this.claudeDirInput);
    group.appendChild(row1);

    // 行 2：操作按钮
    const row2 = document.createElement("div");
    row2.className = "settings-row settings-row-end";
    const pickBtn = document.createElement("button");
    pickBtn.type = "button";
    pickBtn.className = "settings-btn";
    pickBtn.textContent = copyText("settingsPanel.dataDir.browse");
    pickBtn.addEventListener("click", () => void this.pickClaudeDir());
    row2.appendChild(pickBtn);
    const resetBtn = document.createElement("button");
    resetBtn.type = "button";
    resetBtn.className = "settings-btn";
    resetBtn.textContent = copyText("settingsPanel.dataDir.reset");
    resetBtn.title = copyText("settingsPanel.dataDir.resetHint");
    resetBtn.addEventListener("click", () => this.resetClaudeDir());
    row2.appendChild(resetBtn);
    group.appendChild(row2);

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
   * F82b：把一个既有的「表单本体」`body` 包一层子分节小标题（`.settings-group-title`），供
   * 合并后的 4 组内部导航（如「外观」里的 行为 / 快捷键，「集成」里的 诊断 / 数据存储）。
   */
  /**
   * **分区块隔离**（T07）。构造 `build()` 时抛出 → 就地渲染「此区块加载失败」，
   * **其余区块照常出**，而不是整个设置窗白屏。
   *
   * 为什么必须收 thunk 而不是收 `HTMLElement`：`titledSection(t, new Foo().element)`
   * 的**实参在进入函数前就求值**，`new Foo()` 抛的话根本走不到函数体里的 try。
   *
   * 爆炸半径实测（T07 §0/§1）：**9 个文件 / 24 处在构造期发起 I/O**
   * （`void this.loadX()`），而整条构造链
   * `panel.buildBody → panel 构造器 → main.ts:859 new SettingsPanel → main.ts:122
   * bootstrapSettings → main.ts:102 DOMContentLoaded` **没有一个 try/catch**。
   * 两者相乘：任一 section 构造期抛 → 整页白、零提示。
   *
   * 每块一个 catch，**不是整个 `buildBody` 一个** —— 那样一块坏还是全没。
   *
   * ## 它挡什么、不挡什么（如实写明，别让人以为白屏问题全解了）
   *
   * **挡住的**：构造期**同步**抛。`buildPasteBlock` 的三句话必填 `throw` 就是这一类
   * ——那也是当初发现这条链没有 try/catch 的起因。
   *
   * **挡不住的**：那 24 处构造期 I/O 全是 `void this.someAsyncMethod()` 形态
   * （实测：`cc-bus-section.ts:72` 的 `void this.loadOrigins()` 对应
   *  `:192 private async loadOrigins()`）。**`void` 掉的 Promise 其 reject 是
   * 未捕获 rejection，同步 try/catch 抓不到。**
   *
   * 为什么今天不炸：那些 section 各自内部有 catch（实测 7 个文件共 39 处）。
   * **但我没有逐一核实那 39 处是否覆盖了全部 24 条路** —— 所以这里只声明
   * `safeBlock` 覆盖同步路径，**不声明白屏问题已全解**。异步那半边留给 T07 的
   * 对抗性审计去核；覆盖不到就如实说没守，这是本工作区的纪律。
   */
  private safeBlock(
    title: string,
    build: () => HTMLElement,
    opts: { untitled?: boolean } = {},
  ): HTMLElement {
    try {
      // `untitled`：这一块独占一个子页，页头已经是它的名字 ⇒ 块不再自带标题
      //   （同名标题叠两层是 `§8` #11 那种重名）。失败时照旧带标题 —— 那一刻得说清是哪一块坏了。
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
      msg.textContent = copyText("settingsPanel.safeBlock.failed", { e: String(e) });
      wrap.appendChild(msg);
      // 可复制——用户报障时要的是原文，不是转述
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

    // 单项"恢复默认"按钮：清掉该字段的覆盖，CSS var 回到 styles.css :root 默认
    const resetBtn = document.createElement("button");
    resetBtn.type = "button";
    resetBtn.className = "settings-field-reset";
    resetBtn.textContent = copyText("settingsPanel.field.reset");
    resetBtn.title = copyText("settingsPanel.field.resetHint", { label: f.label });
    resetBtn.addEventListener("click", (e) => {
      // row 是 <label>，点击会冒泡到关联的 input；阻止默认 + 阻止冒泡
      e.preventDefault();
      e.stopPropagation();
      this.resetField(f);
    });
    row.appendChild(resetBtn);

    this.inputs.set(f.key, control);
    return row;
  }

  /** 单项重置：清掉 this.current[key]，单 token 应用，重画该 input 的占位值 */
  private resetField(f: FieldSpec): void {
    delete this.current[f.key];
    applyThemeToken(f.key, undefined);
    this.syncOneInput(f);
    // 单项恢复默认也是改了就落。
    void this.persistTheme().catch((e: unknown) => this.reportSaveFailure(copyText("settingsPanel.save.appearance"), e));
  }

  /** 把单个 token 当前值（如果覆盖了）或 :root 计算值写到对应 input */
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
        // 控件预览：option 文字本身用对应字体显示
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
    // `secondary` 就是默认那一种：原先拼出来的 `settings-btn-secondary` 从没有过规则，已摘。
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
    // 性能关键：拖 color picker 时 `input` 事件 ~60Hz 高频；只更新这一个 token，
    // 避免每帧调 14 次 setProperty 触发整棵 :root 子树重算
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

  /** 把 this.current 的值写回所有 input；无覆盖的字段读 :root 计算值作为占位 */
  private syncInputs(): void {
    for (const f of FIELDS()) {
      this.syncOneInput(f);
    }
  }
}

/** input[type=color] 只接受 #rrggbb；过滤掉 #rgb / rgb()/font 串 */
function isShortHex(s: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(s);
}
