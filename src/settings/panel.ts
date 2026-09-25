/**
 * 设置面板：外观（主题 / 字体）+ 数据目录（Claude 数据位置）+ 行为 / 快捷键 / 远端 / 集成 / 诊断。
 *
 * 解耦：
 *  - 外观：只调 theme.ts 的 applyTheme / loadTheme / saveTheme
 *  - 数据目录：只调 paths.ts 的 getClaudeDirOverride / setClaudeDirOverride
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
  loadTheme,
  saveTheme,
  type ThemeConfig,
} from "../theme";
import { getClaudeDirOverride, setClaudeDirOverride } from "../paths";
import { CcIntegrationSection } from "./cc_integration";
import { AccountsSection } from "./accounts-section";
import { McpSection } from "./mcp-section"; // F87：MCP 管理（集成组）
import { PluginsSection } from "./plugins-section"; // P8a：marketplace 只读枚举（**不声称安装/启用**）
import { CcBusHooksSection } from "./cc-bus-hooks-section"; // B04：钩子只读诊断 + 生成待贴文本（绝不写入）
import { ConfigSurfaceSection } from "./config-surface-section"; // T02：配置面审计（只读、按需一次、不轮询）
import { DriftLedgerSection } from "./drift-ledger-section"; // U-CC1：数据面漂移记账（只读、按需一次、不轮询）
import { DiagnosticsSection } from "./diagnostics-section";
import { CollapsibleGroup } from "./collapsible-group";
import { makeSkeleton } from "./skeleton";
import { SettingsRouter } from "./router";
// E62：`markRestartNeeded` —— 本文件两处「重启才生效」的改动此前不给常驻条供货。
import { createRestartBar, markRestartNeeded } from "./restart-notice";
import { createUnknownKeysBar } from "./unknown-keys-notice"; // 🔴 P12：未知键要出声
import { hostOsAllows, type HostOs } from "./host-os"; // S9：本机 OS 门
import { setCurrentMachine } from "./machine-context";
import {
  LOCAL_MACHINE_PAGE_ID,
  MACHINE_PAGE_PREFIX,
} from "./remote-section";
import { DataSection } from "./data-section";
import { RemoteSection } from "./remote-section";
import { BackendSection } from "./backend-section"; // P2s（C8）：每台机一个后端开关
import {
  getBehavior,
  setBehavior,
  withResumePreset,
  type BehaviorConfig,
} from "../behavior";
import { diagnoseRemoteLauncher } from "../launcher-diagnostics";
import { fetchLocalAccounts } from "../accounts"; // K-R49：别名是给**这台机器**的 shell 用的
import { buildAliasManager } from "./machine-aliases"; // 〔AL1〕机器页 ②「别名」
import { dispatcher } from "../keybindings/registry";
import { KeybindingsEditor } from "../keybindings/editor";
// F82a：独立设置窗口——保存后广播 `settings-applied`，主窗口 listen 后重读并应用主题/行为
// （跨 OS 窗口无法直接回调）；close/cancel 关闭本窗口。事件名在中立模块 events.ts。
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { SETTINGS_APPLIED_EVENT } from "./events";

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
const BASE_FONT_PRESETS: ReadonlyArray<{ label: string; value: string }> = [
  { label: "默认（推荐）", value: "" },
  { label: "Inter", value: "Inter, 'Segoe UI', system-ui, sans-serif" },
  {
    label: "Microsoft YaHei UI",
    value: "'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif",
  },
  { label: "Segoe UI", value: "'Segoe UI', system-ui, sans-serif" },
  { label: "系统默认", value: "system-ui, sans-serif" },
];

const MONO_FONT_PRESETS: ReadonlyArray<{ label: string; value: string }> = [
  { label: "默认（推荐）", value: "" },
  { label: "JetBrains Mono", value: "'JetBrains Mono', Consolas, monospace" },
  { label: "Cascadia Code", value: "'Cascadia Code', Consolas, monospace" },
  { label: "Fira Code", value: "'Fira Code', Consolas, monospace" },
  { label: "Source Code Pro", value: "'Source Code Pro', Consolas, monospace" },
  { label: "Consolas", value: "Consolas, monospace" },
  { label: "系统等宽", value: "monospace" },
];

const FIELDS: ReadonlyArray<FieldSpec> = [
  { key: "font-base", label: "正文字体", type: "font-base", group: "font" },
  { key: "font-mono", label: "等宽字体", type: "font-mono", group: "font" },
  {
    key: "font-size-base",
    label: "基础字号 (px)",
    type: "number",
    group: "font",
  },
  { key: "bg", label: "主背景", type: "color", group: "color" },
  { key: "bg-2", label: "次背景", type: "color", group: "color" },
  { key: "card", label: "卡片", type: "color", group: "color" },
  { key: "text", label: "主文本", type: "color", group: "color" },
  { key: "text-2", label: "次文本", type: "color", group: "color" },
  // 🔴 〔2026-09-19 用户裁定：撤掉〕这里原先有「用户色」「Claude 色」两个取色器。
  //    它们**拖了界面不会有任何变化** —— 链子是通的（一路走到 `theme.ts` 真的把值
  //    写进 DOM 的那一步），但**全仓没有一条 CSS 读这两个变量**
  //    ⚠ 这句话**刻意不写出那个调用的字面形状**：`css-conventions` 的扫描器
  //      按字面认「TS 侧设了哪些自定义属性」，注释里写全了会被它当成一处真调用点
  //      （我第一版就这么写的，当场被那条恒等断言逮住）。
  //    （剥注释后 `var(--user)`/`var(--assistant)` 合计 0 处，`main` 上同样 0，一直如此）。
  //    两条路：接上（让哪些元素跟着走）／撤掉。用户逐字「取色器撤掉」。
  //    ⚠ 撤的是**旋钮**，不是「配色可调」这件事 —— 另外七格（主背景/卡片/主文本/
  //      成功/警告/错误/次文本）都有真消费者，一个没动。
  { key: "success", label: "成功", type: "color", group: "color" },
  { key: "warn", label: "警告", type: "color", group: "color" },
  { key: "error", label: "错误", type: "color", group: "color" },
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

const BEHAVIOR_INFO_TEXT =
  "自动跟随用户输入 + 拉前 monitor 窗口。\n\n" +
  "「自动跟随」：watcher 反推—用户在 claude 里敲回车 → monitor 切到对应 session 的 Tab。" +
  "只跟「真用户输入」（不跟工具返回、不跟 claude 流式回复）。你手动点 Tab 后 5 秒内不会被自动切抢回。\n\n" +
  "「拉前 monitor」默认关：monitor 静静在后台切 Tab，不打断你正在用的其他窗口（浏览器/IDE）。" +
  "开启后在终端输入时 monitor 浮上来抢焦。仅当「自动跟随」开启时生效。";

const KEYBINDINGS_INFO_TEXT =
  "自定义全部快捷键：Tab 切换 / 终端拉前 / 行为开关 / 弹层关闭 等约 17 项。\n\n" +
  "改即生效，无需重启。点 [改] 后按下你想要的组合键。冲突会弹覆盖确认。";

// S2：原 `INTEGRATION_INFO_TEXT` 是一段合写的文案，而它描述的两件事在新 IA 里**去了不同的页**
// ——「Claude 数据目录」是 monitor 自己的配置（应用页），「PowerShell 集成」是某台机器上的
// 启动器（机器页）。合着搬会让两页各有一半文案对不上眼前的内容，故按语义拆开。
const DATA_DIR_INFO_TEXT =
  "「Claude 数据目录」—— monitor 监听的 .claude 根目录（下含 projects/ 和 sessions/）。" +
  "默认 ~/.claude 或 $CLAUDE_CONFIG_DIR。修改后需重启 monitor 生效。";
const TERMINAL_INTEGRATION_INFO_TEXT =
  "「终端集成」—— 把 cc 命令注入到 $PROFILE，让你打 `cc` 而不是 `claude` 启动 " +
  "Claude Code，自动跟 monitor 双向绑定（拉前终端按钮才能 work）。可一键安装/卸载。";

const APPEARANCE_INFO_TEXT =
  "字体（正文 / 等宽 / 字号）+ 颜色（10 个语义 token：背景 / 卡片 / 文字 / user / assistant 等）。" +
  "配一次基本不再动，所以默认收起。";

const REMOTE_INFO_TEXT =
  "「远端 (SSH)」—— monitor 通过 SSH 连到远端主机，由远端后端取代本地 " +
  "jsonl-watcher 作为数据源（渲染 / Tab / 分支等行为完全相同）。\n\n" +
  "关闭（默认）时一切走本地，不受影响。启用 / 修改任意远端设置后需重启 monitor 才生效。" +
  "配置不完整（缺 host / user / backendPath）时后端自动回退本地模式。";

// 🔴 `70 §10.3`/`§10.2` 改名 ＋ `§2.4` 纪律：两块的名字跟着改（「诊断」→「日志」·
// 「数据存储」→「数据位置」），并且把 `tracing` 这个**内部标识符**拿掉
//（`91 §2.1` 那一族 —— 用户不需要知道我们用的是哪个日志库）。
const DIAG_STORAGE_INFO_TEXT =
  "「日志」—— 让后端把细节写进日志文件，出问题时拿得到一份能发给作者的记录。\n\n" +
  "「数据位置」—— 透明展示 monitor 自身写入的所有持久化路径：config.json / history-metadata.json / " +
  "WebView2 UserDataFolder / localStorage keys 等。每项可点 [打开] 直接到文件管理器。" +
  "纯展示，无危险操作。";

// F82b（#56+#47）：4 组终态的合并 tooltip——外观并了 行为/快捷键、集成并了 诊断&存储，
// group 级 tooltip 把原分组说明拼一起（子分节各带小标题导航）。
const APPEARANCE_GROUP_INFO_TEXT =
  APPEARANCE_INFO_TEXT +
  "\n\n【行为】" +
  BEHAVIOR_INFO_TEXT +
  "\n\n【快捷键】" +
  KEYBINDINGS_INFO_TEXT;
// S2：「日志与数据」页内折叠组的文案 = 数据目录 + 诊断/存储（正好是它的三块内容）。
const LOGS_AND_DATA_INFO_TEXT =
  DATA_DIR_INFO_TEXT + "\n\n" + DIAG_STORAGE_INFO_TEXT;
// S2：机器页的文案 = 怎么连上远端 + 这台机上的启动器集成。
const MACHINES_PAGE_INFO_TEXT =
  REMOTE_INFO_TEXT + "\n\n【终端集成】" + TERMINAL_INTEGRATION_INFO_TEXT;
// S2 删除：原 `REMOTE_GROUP_INFO_TEXT` 描述的是那个「留空占位」的空组（F82b 拍板的 4 组之一，
// 后被 A3 借去放账号）。它逐字写着「当前尚无独立项…留空占位」「在上面的『连接』组」——
// 那个组和那个「上面」都不存在了，留着就是一句会误导人的话。

/** S2：落地页 id。主计划 §2.3 指定为「机器」。 */
const SETTINGS_LANDING_ROUTE = "machines";

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
    /** S4b-3b-2：这块归详情页的哪一栏。`footprint` 是 `70 §10.1`/`§5.3` 新增的第五栏。 */
    tab: "acct" | "tools" | "footprint";
    el: HTMLElement;
    /**
     * ST1「延后加载」：这一块的第一发 I/O。**某台机器的子页第一次可见时**才调
     * （`70 §5.3` 判据 2）。构造失败（`safeBlock` 收住）的块没有它。
     */
    load?: () => void;
  }[] = [];
  /** ST1：本次打开以来 per-machine 那几块放过 I/O 没有。`open()` 清零（重开要看新读数）。 */
  private perMachineLoaded = false;
  /**
   * 🔴 第一刀 · 步 2（`设计/70 §1.3 B`）：**「这一页首次可见」才发 I/O。**
   *
   * # 为什么门开在这里，而不是各块自己判
   *
   * `70 §1.2` 的成因链第 ③ 层：14 个块**全部在 `buildBody()` 里即时构造**，
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
  /**
   * S9：本机页上按 OS 显隐的块（今天只有「终端集成」）。
   *
   * **不是「构造了再藏」**：`CcIntegrationSection` 的构造函数就会发两次
   * Windows 专用 IPC（`cc_integration_status` / `cc_get_auto_launch`），
   * 藏起来那两次照发。所以门开在**建不建**这一层。
   */
  private static readonly CC_INTEGRATION_HOST_OS: readonly HostOs[] = [
    "windows",
  ];
  /** S4b-3b-2：pageId → 该页「账号 / 工具 / 足迹」三栏的容器。 */
  private machineTabSlots = new Map<
    string,
    { acct: HTMLElement; tools: HTMLElement; footprint: HTMLElement }
  >();
  /** 当前编辑中的 theme（实时预览用） */
  private current: ThemeConfig = {};
  /** 打开时的 theme 快照，取消时回滚 */
  private original: ThemeConfig = {};
  private inputs = new Map<
    keyof ThemeConfig,
    HTMLInputElement | HTMLSelectElement
  >();
  private isOpen = false;

  /** Claude 数据目录输入框 —— 改动后保存会提示需要重启 */
  private claudeDirInput!: HTMLInputElement;
  /** 打开时 claudeDir 的快照，用于判断是否变化（变了就提示重启） */
  private claudeDirOriginal: string = "";
  /** E62：打开设置时 `showBgSessions` 的值——用来判断「真的改了没」。 */
  private showBgOriginal = true;

  /** 顶部状态提示行（保存成功 / 需重启 等） */
  private banner!: HTMLElement;
  /** issue #3 (A): 「数据位置」展示区（`70 §10.2` 改名）。打开面板时 refresh 一次拉最新 stat */
  private dataSection?: DataSection;
  /** `70 §10.1`（步 14a）：「足迹」——已从顶层「改动足迹」页搬进机器子页第五栏。 */
  private footprintSection?: ConfigSurfaceSection;
  /** `70 §10.3` 改名后的「日志」块。步 2 要在「应用」页首次可见时叫醒它。 */
  private logsSection?: DiagnosticsSection;
  /** issue #15 (S6): 远端 (SSH) 配置区。打开面板时 refresh 一次拉最新 config */
  private remoteSection?: RemoteSection;
  /** P2s（C8）：backend 开关区。打开面板时 refresh 一次，重拉每台机的状态。 */
  private backendSection?: BackendSection;

  // v2.4 issue #2: 行为类 toggle
  private autoFollowCheckbox!: HTMLInputElement;
  private showBgCheckbox!: HTMLInputElement;
  private notifyTurnEndCheckbox!: HTMLInputElement;
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
  /** F03（unify-launch）：载荷渲染器那个逃生口（落盘键 `forceLaunchPayloadRenderer`）
   *  无 UI 暴露（手改 config.json），但 `onBehaviorToggle` 每次都要交一份完整
   *  `BehaviorConfig`——缓存 open() 时读到的值原样带回，防止面板任何一个勾选框变动
   *  都把它悄悄重置成 DEFAULTS 里的 false。 */
  private forceLaunchPayloadRenderer = false;
  private onBehaviorChange?: (cfg: BehaviorConfig) => void;
  /** F82a：见 SettingsPanelOptions.windowMode。 */
  private readonly windowMode: boolean;

  /**
   * ST1「关窗改隐藏」（`设计/01 §1.3`：关窗 ＝ 隐藏，不销毁）：本面板亲手把窗口藏起来了。
   * 藏起来之后窗口再拿到焦点 ＝ 被 `open_settings_window` 重新 show 出来 ⇒ 重跑一遍 `open()`
   * （`01 §1.3` 那个「重新打开」事件，这里用窗口自己的 focus 事件代替，不动后端那条命令）。
   */
  private hiddenByUs = false;
  /** ST1「未保存关窗拦截」（`70 §6` #1）：有没保存的改动时拦在关窗前的那一条。 */
  private closeGuard!: HTMLElement;

  // issue #5: 快捷键编辑器（lazy 构造，首次打开时建 DOM）
  private kbEditor?: KeybindingsEditor;
  private kbOverrideChip?: HTMLElement;

  constructor(opts: SettingsPanelOptions = {}) {
    this.onBehaviorChange = opts.onBehaviorChange;
    this.windowMode = opts.windowMode ?? false;
    this.el = this.build();
    document.body.appendChild(this.el);
    if (this.windowMode) this.installWindowLifecycle();
    // issue #5: Esc 由 KeybindingDispatcher 统一调度。本面板 open 时
    // pushOverlay 自己，close 时 pop —— 多弹层共存按 LIFO 顺序关。
  }

  /** dispatcher overlay 接口 */
  handleEsc(): void {
    if (this.isOpen) this.cancel();
  }

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
      this.banner.textContent = `部分设置项加载失败：${String(e)}`;
      this.banner.classList.add("settings-banner-show");
      this.el.classList.add("open");
      this.isOpen = true;
      dispatcher.pushOverlay(this);
    }
  }

  private async openInner(): Promise<void> {
    this.original = await loadTheme();
    this.current = { ...this.original };
    this.claudeDirOriginal = (await getClaudeDirOverride()) ?? "";
    this.claudeDirInput.value = this.claudeDirOriginal;
    // v2.4 issue #2: 每次打开拉最新 behavior，避免跟外部其他改动脱节
    const behavior = await getBehavior();
    this.autoFollowCheckbox.checked = behavior.autoFollowUserActive;
    this.bringFrontCheckbox.checked = behavior.bringMonitorToFrontOnUserActive;
    this.showBgCheckbox.checked = behavior.showBgSessions;
    // E62：记下打开设置时的值，只有**真的改了**才供货（每次 toggle 都标会把噪音变回来）。
    this.showBgOriginal = behavior.showBgSessions;
    this.notifyTurnEndCheckbox.checked = behavior.notifyTurnEnd;
    this.resumeLocalInput.value = behavior.resumeCommandLocal;
    this.resumeRemoteInput.value = behavior.resumeCommandRemote;
    // ⚠ `?? []` 不是防 `getBehavior`（它总会填缺省），是防**这一排 chip 掀翻整个面板**：
    // 实测缺字段时 `renderResumePresets` 抛错 ⇒ `open()` 整个中断 ⇒ 面板停在错误的页。
    // 一个装饰性的候选条不该有那种权力。
    this.resumeLocalPresets = behavior.resumeCommandLocalPresets ?? [];
    this.resumeRemotePresets = behavior.resumeCommandRemotePresets ?? [];
    this.renderResumePresets();
    this.updateRemoteLauncherWarning();
    this.forceLaunchPayloadRenderer = behavior.forceLaunchPayloadRenderer;
    this.updateBringFrontEnabled();
    this.banner.textContent = "";
    this.banner.classList.remove("settings-banner-show");
    this.syncInputs();
    // 🔴 步 2：**重开设置 = 每一页的「首次可见」重新算一遍**，但仍然只拉
    // **用户真看得见的那一页**。原来这里是无条件 `this.dataSection?.refresh()`
    // 与「日志」那两发 —— 那三发在落地页是「机器」的情况下**每次打开都是白发的**，
    // 正是 `§8` 判据 #3（非落地页零 I/O）今天被打破的那一处。
    this.pagesLoaded.clear();
    this.perMachineLoaded = false;
    // issue #15 (S6): 每次打开重拉 config.json 的 remote 子对象，跟外部改动对齐
    // 步 4（`70 §1.3 D`）：`refresh()` 失败时会把原因画到那一块自己的 banner 上；
    // 这里 `catch` 掉是为了不再多产一条走状态栏的未捕获 rejection（同一件事说两遍，
    // 而其中一遍说在了离现场十万八千里的地方）。
    void this.remoteSection?.refresh().catch(() => {});
    // P2s：状态是**运行期**的东西，每次打开都要重拉 —— 缓存住等于给用户看一张旧照片。
    // ⚠ 它住**落地页**（「机器」），所以它不在延后那一档里：打开就该是新的。
    void this.backendSection?.refresh();
    // issue #5: 同步快捷键覆盖数 chip（编辑器关闭时也可能改了）
    this.refreshKbChip();
    // S2：每次打开回落地页。**刻意不记忆上次停在哪一页** —— 既然计划把「机器」定为落地页，
    // 记忆就会让这个决定从第二次打开起失效。
    this.router.navigate(SETTINGS_LANDING_ROUTE);
    // ⚠ `navigate()` 在「已经在这一页」时会**提前返回、不通知订阅者**（同页不重复通知，
    //   那条是对的：订阅者会做搬 DOM 这类有代价的事）。⇒ 第二次打开时落地页的 flush
    //   必须在这里补一刀，否则它只在**第一次**打开时发生过。
    this.flushPage(this.router.activeId);
    this.el.classList.add("open");
    this.isOpen = true;
    // 面板始终作为 overlay 栈**底**（窗口模式也是）：这样设置窗内的快捷键编辑器 / SFTP 面板
    // 压栈其上时 Esc 走 dispatcher 的 LIFO 逐层关（先关它们、再 Esc 关面板→关窗），
    // 与主窗口抽屉行为一致。窗口模式下面板 handleEsc→cancel→close()→关窗（见 close()）。
    dispatcher.pushOverlay(this);
  }

  /** v2.4 issue #2: autoFollow 关 → bringFront 灰显（依赖前者，无意义） */
  private updateBringFrontEnabled(): void {
    this.bringFrontCheckbox.disabled = !this.autoFollowCheckbox.checked;
  }

  /** F08：越层启动器诊断——只读提示，不碰 `resumeRemoteInput.value` 本身（MASTERPLAN 设计
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
        chip.className = "settings-btn settings-btn-secondary settings-preset";
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
        del.className = "settings-btn settings-btn-secondary settings-preset-del";
        del.textContent = "×";
        // ⚠ **正在生效的那条不给移除**：这张表的含义是「用过的命令」，
        // 而移除一条**此刻正在用**的命令是自相矛盾的 —— 保存时它必然又被记回来
        // （`withResumePreset` 会把输入框里的值并进去），用户会看见「点了没反应」。
        // 与其造那种假象，不如当场说清为什么点不了。
        const active = input.value.trim() === cmd;
        del.disabled = active;
        del.title = active
          ? `${cmd} 正在生效 —— 换成别的命令之后才能把它从预设里移除。`
          : `从预设里移除 ${cmd}`;
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
      forceLaunchPayloadRenderer: this.forceLaunchPayloadRenderer,
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
        markRestartNeeded("显示后台任务会话");
        this.showBgOriginal = next.showBgSessions;
      }
      this.onBehaviorChange?.(next); // 同窗（主窗口浮层）直接同步 TabManager
      this.broadcastApplied(); // 窗口模式：广播让主窗口 applyBehavior
    } catch (e) {
      console.warn("save behavior failed:", e);
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
    } catch (e) {
      console.warn("[settings] 窗口生命周期接管失败：", e);
    }
  }

  /** 外观（主题）或 Claude 数据目录有没保存的改动 —— 其余各块都是即时写，没有「未保存」这回事。 */
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
   * ST1「未保存关窗拦截」（`70 §6` #1 · `§8` #7）：系统 X 与页头 × 都走这里。
   * 没改动 ⇒ 直接关；有改动 ⇒ 亮出那一条（保存并关闭 / 丢弃改动 / 继续编辑），**不静默丢**。
   * 「取消」与 Esc 不走这里 —— 那两个本来就是明说的「丢弃并关闭」（`§6` #1 原文：它们会回滚）。
   */
  requestClose(): void {
    if (!this.isDirty()) {
      this.close();
      return;
    }
    // 显隐只走类（`.settings-banner` 自带 `display:none`，再用 `hidden` 切会撞 S30 ⑦）。
    this.closeGuard.classList.add("settings-banner-show");
  }

  private hideCloseGuard(): void {
    this.closeGuard.classList.remove("settings-banner-show");
  }

  close(): void {
    // 关闭前 blur 掉面板内仍聚焦的输入框。本面板是 hide（移除 .open class）而非从 DOM
    // 移除，元素留着、焦点不会自动释放。若不 blur，document.activeElement 仍是这个隐藏
    // 输入，单键快捷键守卫（registry.ts::isEditableTarget）会把它当"正在打字"，从而吞掉
    // 所有单键快捷键（h/w/数字… 全部失效）—— 用户配置完远端/外观关掉设置后最典型。
    const active = document.activeElement;
    if (active instanceof HTMLElement && this.el.contains(active))
      active.blur();
    this.hideCloseGuard();
    // 窗口模式：ST1「关窗改隐藏」—— 关闭 ＝ 把窗口藏起来（`01 §1.3`），面板状态照非窗口模式那样收好，
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
            this.banner.textContent = `关不掉这个窗口：${String(e)}`;
            this.banner.classList.add("settings-banner-show");
          });
        });
      return;
    }
    this.el.classList.remove("open");
    this.isOpen = false;
    dispatcher.popOverlay(this);
  }

  private cancel(): void {
    applyTheme(this.original);
    this.close();
  }

  /** F82a：窗口模式下广播「设置已应用」，主窗口 listen 后重读并 applyTheme/applyBehavior。
   *  非窗口模式（主窗口内浮层）走 applyTheme/onBehaviorChange 同窗直接生效，无需广播。 */
  private broadcastApplied(): void {
    if (this.windowMode) void emit(SETTINGS_APPLIED_EVENT);
  }

  private async save(): Promise<void> {
    await saveTheme(this.current);
    this.original = { ...this.current };
    this.broadcastApplied(); // 主窗口重读主题并应用

    // claudeDir：与 theme 字段独立保存。变了就提示重启
    const nextDir = this.claudeDirInput.value.trim();
    const dirChanged = nextDir !== this.claudeDirOriginal;
    if (dirChanged) {
      await setClaudeDirOverride(nextDir === "" ? null : nextDir);
      this.claudeDirOriginal = nextDir;
      // E62：给 S7 那条常驻条**供货**。这里的 banner 是一次性的（关窗即没），
      // 而「还没生效」是个会一直为真到重启为止的状态 —— 两者不是一回事，都要有。
      markRestartNeeded("Claude 数据目录");
      this.banner.textContent =
        "Claude 数据目录已更新 —— 需要重启 monitor 才能生效";
      this.banner.classList.add("settings-banner-show");
      return; // 不关面板，让用户看到提示
    }
    this.close();
  }

  private async resetAll(): Promise<void> {
    if (
      !window.confirm("确定要恢复全部外观默认？已保存的颜色和字体偏好会丢失。")
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
        title: "选择 Claude 数据目录（含 projects 和 sessions 子目录）",
      });
      if (typeof selected === "string" && selected) {
        this.claudeDirInput.value = selected;
      }
    } catch (e) {
      console.warn("dialog open failed:", e);
    }
  }

  private resetClaudeDir(): void {
    this.claudeDirInput.value = "";
  }

  // === DOM 构建 ===

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = "settings-panel";

    root.appendChild(this.buildHeader());
    root.appendChild(this.buildCloseGuard());
    // 🔴 P12：「配置里有 app 不认识的键」常驻条。**放在最上面、任何一页之前** ——
    // 它说的是「你写下的某个设置根本没生效」，比面板里任何一格都更该先被看见。
    // 同 S7 那条：它是状态不是事件，所以不属于任何一页，也刻意没有关闭按钮。
    root.appendChild(createUnknownKeysBar());
    root.appendChild(this.buildBody());
    // ★ S7：「有改动待重启」常驻条。**放在 footer 之上、面板底部** —— 它是状态，
    // 不是某一页的事（改远端配置和改诊断开关都会点亮它），所以不属于任何一页。
    // 空时整块不渲染；**刻意没有关闭按钮**（见 restart-notice.ts 头注）。
    root.appendChild(createRestartBar());
    root.appendChild(this.buildFooter());

    return root;
  }

  /** ST1「未保存关窗拦截」那一条。平时藏着；`requestClose()` 发现有改动才亮。 */
  private buildCloseGuard(): HTMLElement {
    const bar = document.createElement("div");
    bar.className = "settings-banner";
    bar.dataset.closeGuard = "unsaved";
    bar.append("外观或 Claude 数据目录有改动还没保存。");
    const saveBtn = this.makeBtn("保存并关闭", "primary", () => {
      void this.save()
        .then(() => {
          if (this.isOpen) this.close();
        })
        .catch((e: unknown) => {
          this.hideCloseGuard();
          this.banner.textContent = `保存失败：${String(e)}`;
          this.banner.classList.add("settings-banner-show");
        });
    });
    const dropBtn = this.makeBtn("丢弃改动", "secondary", () => this.cancel());
    const stayBtn = this.makeBtn("继续编辑", "secondary", () => this.hideCloseGuard());
    bar.append(saveBtn, dropBtn, stayBtn);
    this.closeGuard = bar;
    return bar;
  }

  private buildHeader(): HTMLElement {
    const header = document.createElement("div");
    header.className = "settings-header";
    const title = document.createElement("span");
    title.textContent = "设置";
    header.appendChild(title);

    const close = document.createElement("button");
    close.className = "settings-close";
    close.type = "button";
    close.textContent = "×";
    close.title = "关闭（ESC 也行）";
    // ST1：页头 × 与系统 X 同一条路 —— 有没保存的改动先拦一下（`70 §6` #1）。
    close.addEventListener("click", () => this.requestClose());
    header.appendChild(close);
    return header;
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
    for (const b of this.perMachineBlocks) {
      b.el.hidden =
        b.appliesTo !== "both" && b.appliesTo !== (isLocal ? "local" : "remote");
    }
    const slots = pageId ? this.machineTabSlots.get(pageId) : undefined;
    if (slots) {
      // S4b-3b-2：分栏页 —— 每块按 `tab` 归到「账号 / 工具 / 足迹」栏里。
      for (const b of this.perMachineBlocks) slots[b.tab].appendChild(b.el);
      return;
    }
    // 没有分栏（本机页、以及 RemoteSection 挂掉时的兜底落点）→ 整块搬，形态同 S4b-2。
    for (const b of this.perMachineBlocks) this.perMachineSlot.appendChild(b.el);
    page.appendChild(this.perMachineSlot);
  }

  /**
   * S4b-3b-2：一台远端机器的四栏页。
   *
   * 「连接 / 组件」来自 `MachineCard` 拆出的两块；「账号 / 工具」是 per-machine 那几块
   * 分节的落点 —— 它们是**单例**，由 `movePerMachineTo` 在切页时搬进当前这一页的对应栏。
   */
  private buildMachineTabs(
    pageId: string,
    parts: { connection: HTMLElement; components: HTMLElement },
  ): HTMLElement {
    const tabs = new SettingsRouter({
      landingId: `${pageId}#conn`,
      orientation: "horizontal",
      hidePageHeader: true,
    });
    tabs.addRoute({ id: `${pageId}#conn`, title: "连接", element: parts.connection });
    tabs.addRoute({ id: `${pageId}#comp`, title: "组件", element: parts.components });
    const acct = document.createElement("div");
    const tools = document.createElement("div");
    const footprint = document.createElement("div");
    tabs.addRoute({ id: `${pageId}#acct`, title: "账号", element: acct });
    tabs.addRoute({ id: `${pageId}#tools`, title: "工具", element: tools });
    // 步 14a（`70 §5.3` 那张图 · `§10.1`）：**第五栏「足迹」**。
    // ⚠ 它是**新增一栏**，不是把已有的某一栏搬个位置 —— 所以 `§10.4` 把它单列成 `14a`。
    tabs.addRoute({ id: `${pageId}#footprint`, title: "足迹", element: footprint });
    this.machineTabSlots.set(pageId, { acct, tools, footprint });
    return tabs.element;
  }

  private buildBody(): HTMLElement {
    const body = document.createElement("div");
    body.className = "settings-body";

    // 顶部状态条（保存提示 / 重启提示等）
    this.banner = document.createElement("div");
    this.banner.className = "settings-banner";
    body.appendChild(this.banner);

    // ★ S2（settings-ia）：从「一列 4 个折叠组」改成**左导航 + 分页**。
    //
    // 原来的 4 组（连接 / 外观 / 账号 / 集成）**不在同一抽象层** —— 「连接」是动作、
    // 「外观」「账号」是切面、「集成」是其它。判据不统一的后果是可量化的：14 个叶子里
    // **8 个挤在「集成」**，因为新东西没地方放就往那儿扔。
    //
    // 新判据（主计划 §2.1，可机械执行）：**每个顶层 = 一类「被设置的对象」**。
    // 新功能来了只问一句：**它改的是谁的状态。**
    //
    // 「集成」这个名字就此消失 —— 不是改叫「部署」：那 8 个叶子里真属部署的只有 1 个。
    //
    // **本轮只搬不改**：每个块的内部实现、构造时机都一字不动（见下面「构造时机」一段）。
    // 机器列表页 → S3；机器详情页四栏 → S4；改动足迹页扩充 → S5；cc-bus 移出 → S6。
    const router = new SettingsRouter({ landingId: SETTINGS_LANDING_ROUTE });
    this.router = router;

    // ---- 应用：改 monitor 自己的状态 ----
    const appPage = document.createElement("div");
    appPage.appendChild(this.safeBlock("行为", () => this.buildBehaviorGroup()));
    appPage.appendChild(
      this.safeBlock("快捷键", () => this.buildKeybindingsGroup()),
    );
    const appearance = new CollapsibleGroup({
      // id 沿用 F82b 那个：字体+颜色这两块的归属没变，用户此前的折叠状态该跟过来。
      id: "appearance-4grp",
      title: "外观",
      defaultCollapsed: true,
      infoTooltip: APPEARANCE_GROUP_INFO_TEXT,
    });
    appearance.appendChild(
      this.buildGroup(
        "字体",
        FIELDS.filter((f) => f.group === "font"),
      ),
    );
    appearance.appendChild(
      this.buildGroup(
        "颜色",
        FIELDS.filter((f) => f.group === "color"),
      ),
    );
    appPage.appendChild(appearance.element);

    // 「日志与数据」——主计划 §2.3 逐字指定的折叠组。读一次就够的东西。
    const logsAndData = new CollapsibleGroup({
      id: "logs-and-data",
      title: "日志与数据",
      defaultCollapsed: true,
      infoTooltip: LOGS_AND_DATA_INFO_TEXT,
    });
    logsAndData.appendChild(this.buildDataGroup());
    // 🔴 `70 §10.3`：「诊断」→「日志」。**让名**给 `§5.3` 那个改名，否则设置面板里
    // 会同时有两个「诊断」（一个是「这台机器还缺什么」，一个是 monitor 的日志开关）。
    logsAndData.appendChild(
      this.safeBlock("日志", () => {
        const sec = new DiagnosticsSection({ headless: true });
        this.logsSection = sec;
        return sec.element;
      }),
    );
    // 🔴 步 3b（`70 §10.2` 差项 2 · `§10.4` 第一刀）：**这一块原先的 `new` 在 `safeBlock` 外面。**
    // 原文是 `const dataSection = new DataSection(...)` 裸构造，`safeBlock` 只包住了
    // `() => dataSection.element` 那个 thunk —— 而 thunk 不可能抛。
    // ⇒ 这一块**没有 T07 那层隔离**：`§9` 里写着「`safeBlock` 每块一个 catch 不动」，
    //   那条纪律在这里**漏了一块**。现在 `new` 挪进 thunk 里，与它的九个兄弟同形。
    // ⚠ 字段仍然在 build 时赋值（`open()` 那边 `?.refresh()` 靠它）——
    //   构造失败时留 `undefined`，与 `remoteSection` 那一格同一个约定。
    logsAndData.appendChild(
      this.safeBlock("数据位置", () => {
        const sec = new DataSection({ headless: true });
        this.dataSection = sec;
        return sec.element;
      }),
    );
    appPage.appendChild(logsAndData.element);
    router.addRoute({ id: "app", title: "应用", element: appPage });
    // 步 2：这一页的 I/O（日志 2 发 + 数据位置 1 发）挂到「这一页首次可见」上。
    this.loadOnFirstVisit("app", () => {
      this.logsSection?.loadNow();
      this.dataSection?.loadNow();
    });

    // ---- 机器：改**某一台机器**的状态 ----
    //
    // 下面四块（账号 / 终端集成 / MCP / cc-bus 钩子）之所以归这里：**它们各自都维护着
    // 一份自己的 origin 选择器**（主计划 §5-4 点名 `accounts`/`mcp`/`cc-bus`/`cc-bus-hooks`
    // 四份互不同步）。有 origin 选择器 = 它改的是某台机器的状态。
    // S4 会把这四份选择器换成「当前在哪台机器页」这个上下文。
    const machinesPage = document.createElement("div");
    // P2s（C8）：backend 开关排在「连接（远端）」**之前** —— 它管的是**每台机**（含本机），
    // 而下面那块是远端专有的 SSH 配置面。本机在这一区的第一行，
    // 不为它造特例（`C1`：本地只是不走 ssh 的那一台）。
    // 同样进 `safeBlock`：它构造时会发 IPC，失败不该把整个设置面板炸穿。
    machinesPage.appendChild(
      this.safeBlock("backend 开关", () => {
        const sec = new BackendSection({ headless: true });
        this.backendSection = sec;
        return sec.element;
      }),
    );
    // **T07 审计阻塞 1**：这里必须在 `safeBlock` 里——`RemoteSection` 正是唯一活的同步
    // throw 宿主（构造路径含 `remote-section.ts` 那个三句话必填的 `throw`）。审计真造它抛过：
    // 裸构造会让 `new SettingsPanel` 直接炸穿、**什么都没上屏**。
    // `this.remoteSection` 失败时留 `undefined`——`open()` 那边是 `?.refresh()`，天然容错。
    machinesPage.appendChild(
      this.safeBlock("连接（远端）", () => {
        // S4b：把路由器包成 `MachinePagesHost` 交给它 —— 每台机器的编辑表单去它自己那一页，
        // 列表里只留一行。分节不需要知道路由器长什么样，只要「开页 / 收页 / 跳过去」。
        const sec = new RemoteSection({
          headless: true,
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
              this.pageLoaders.set(id, [
                () => this.footprintSection?.loadNow(),
                () => this.loadPerMachineOnce(),
              ]);
              // ★ S4b-3b-2：远端机器页拆成横向四栏（主计划 §2.3）。
              // 复用 `SettingsRouter`（横向 + 无页头）而不是另造 tab 原语 ——
              // 「同一时刻只有一栏可见 + aria + 方向键 + 不重复注册」与左侧导航
              // 逐条相同，另造等于把这套逻辑抄第二遍、然后各自漂。
              const pageEl = parts ? this.buildMachineTabs(id, parts) : element;
              router.addRoute({ id, title, element: pageEl, parentId: "machines" });
              // S4b-2：本机页一出现就让那几块 per-machine 分节先落在它上面。
              // 这与 `machine-context` 的初始值（null = 本机）对齐 —— 否则 slot 在
              // 用户第一次点进某台机器之前是**游离的**（不在文档里，谁也找不到它）。
              if (id === LOCAL_MACHINE_PAGE_ID) this.movePerMachineTo(element, true);
            },
            removeMachinePage: (id) => {
              router.removeRoute(id);
              // 机器没了，它那一页的登记与「放过了没有」一起清 —— 留着就是死条目，
              // 而且同名机器再出现时会被当成「这一页已经放过了」。
              this.pageLoaders.delete(id);
              this.pagesLoaded.delete(id);
            },
            navigateToMachinePage: (id) => router.navigate(id),
          },
        });
        this.remoteSection = sec;
        return sec.element;
      }),
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
        ...this.loadableBlock("账号", () => new AccountsSection()),
      },
      {
        // PowerShell $PROFILE 注入 —— 只对**本机**有意义；远端的对应物是
        // 机器详情页「组件」栏里的「装 / 卸别名块」（〔MC1〕从前叫「装/卸 ccm」；主计划 §2.4 那张表的「启动器」一行）。
        //
        // S9：而且只对**跑在 Windows 上的**本机有意义。非 Windows 上换成一行说明，
        // **整块不构造**（构造即发两次 Windows 专用 IPC）。
        appliesTo: "local",
        tab: "tools",
        ...(hostOsAllows(SettingsPanel.CC_INTEGRATION_HOST_OS)
          ? this.loadableBlock("终端集成", () => new CcIntegrationSection())
          : { el: SettingsPanel.ccIntegrationNotApplicable() }),
      },
      // 〔AL1 · 2026-09-24〕机器页 ②「别名」（`设计/71 §13`）。本机那一格在这里；远端那一格在
      // `MachineCard` 的「组件」栏里（它只给得出待贴文本 ＋ 装 / 卸别名块，理由见那边）。
      // 构造零 I/O：它是个 `<details>`，第一次展开才读盘（`70 §8` #3）。
      {
        appliesTo: "local",
        tab: "tools",
        el: this.safeBlock("别名", () =>
          buildAliasManager({
            loadAccounts: async () => (await fetchLocalAccounts()).accounts.map((a) => a.name),
          }),
        ),
      },
      // F87（#50+#51）：MCP 管理——读跨 scope 展示 / 写只项目 .mcp.json（SS-14）。
      // 本机与远端都有意义（它自己的机器行第一颗按钮就是本机）。
      {
        appliesTo: "both",
        tab: "tools",
        ...this.loadableBlock("MCP", () => new McpSection()),
      },
      // P8a：插件面（marketplace）只读枚举。
      // 〔RM1b · 第四波〕`appliesTo: "local"` → `"both"`：后端补了 `plugins-marketplaces`
      // （本机与远端同一条路），`PluginsSection` 跟着「当前在看哪台机器」问那一台 ——
      // 原先那句「挂成 both 会出现一个恒失败的块」的前提（远端没有口）不在了。
      {
        appliesTo: "both",
        tab: "tools",
        ...this.loadableBlock("插件（marketplace）", () => new PluginsSection()),
      },
      // B04：钩子诊断。**只读**——不替用户改 ~/.claude/settings.json（共享全局配置）。
      // 本机与远端都要诊断（§2.4 表里这一行两栏都写着「诊断 + 待贴片段」）。
      {
        appliesTo: "both",
        tab: "tools",
        ...this.loadableBlock("cc-bus 钩子", () => new CcBusHooksSection()),
      },
      // 🔴 步 14a（`70 §10.1` · `§5.3` 那张图的第五栏）：**「足迹」**。
      //
      // 它原来住顶层「改动足迹」页，名字叫「配置面审计」。两条都改：
      // ① **住哪**：它答的是「装了 cc-monitor 之后，它在**我这台机器**上动过哪些文件」
      //    —— 被设置的对象是**一台机器**，按 `panel.ts` 那条顶层判据（每个顶层 = 一类
      //    「被设置的对象」）它归**机器子页**，不归顶层页。
      // ② **叫什么**：「配置面审计」→「足迹」。⚠ **诚实标注：这不是 R5 命中**
      //    （「配置面审计」本来就是名词短语，`91 §4` 的 R5 抓不到它）。理由是另外两条：
      //    「面」「审计」是**我们这侧**的词（`91 §2.1` 那一族）；且它要和顶层页
      //    「改动足迹」同一个口径，而不是页叫足迹、块叫审计。
      //
      // ⚠ `appliesTo: "both"` 而**远端那一页上它不画表** —— 今天后端那条读口不收 origin
      //   （`§10.1` ②），照原样画就是拿本机的答案冒充 devbox 的。那一格由
      //   `ConfigSurfaceSection.applyOriginGate()` 负责说实话，头注在那边。
      {
        appliesTo: "both",
        tab: "footprint",
        el: this.safeBlock("足迹", () => {
          const sec = new ConfigSurfaceSection();
          this.footprintSection = sec;
          return sec.element;
        }),
      },
    ];
    for (const b of this.perMachineBlocks) this.perMachineSlot.appendChild(b.el);
    // ★ 兜底落点：先挂在列表页上。
    //
    // 这不是"顺手"—— 它是 `safeBlock` 隔离的一部分。`RemoteSection` 是唯一活的同步
    // throw 宿主（T07 审计阻塞 1）；它挂掉就没有任何机器页被注册，slot 便无处安放，
    // **这五块会一起从界面上消失**。那等于「一块坏，六块没」，把 T07 好不容易建立的
    // 隔离又打破了。有兜底落点的话，最坏情况只是它们留在列表页上 —— 位置不理想，
    // 但都还在、都能用。（审计时真造 RemoteSection 抛才发现的。）
    //
    // 🔴 步 3（`70 §1.1` · `§1.3 C`）：**但它今天是每次打开的前 3 秒的默认视图。**
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
      "正在列这台机器上的账号 / 工具…",
    );
    machinesPage.appendChild(this.perMachineFallbackHint);
    machinesPage.appendChild(this.perMachineSlot);
    // 步 3：`safeBlock` 收住了同步抛 ⇒ `this.remoteSection` 留 `undefined`。
    // **那正是「机器页注册失败」的判别式**（`RemoteSection` 是唯一活的同步 throw 宿主，
    // 也是唯一会注册机器页的人）。这一档就是设计意图里的「最坏情况」，当场亮兜底。
    // ⚠ 这一句必须排在 slot 与那块提示**建出来之后** —— 排在 `safeBlock` 紧后面的话，
    //   它会去读两个还没赋值的字段，把「RemoteSection 挂了」变成「整个面板挂了」。
    if (!this.remoteSection) this.revealPerMachineFallback("机器列表这一块没能建起来");
    router.addRoute({
      id: "machines",
      title: "机器",
      element: machinesPage,
      infoTooltip: MACHINES_PAGE_INFO_TEXT,
    });

    // ---- 改动足迹：「你在我机器上写过什么、能不能撤」 ----
    const footprintPage = document.createElement("div");
    // 🔴 步 14a（`70 §10.1`）：原来这一页的第一块是「配置面审计」——**它已经搬走了**，
    // 改名「足迹」、挂进**每台机器自己的子页**（那张表答的是「我这台机器」的事）。
    // U-CC1：这一页剩下的那一半 —— 「CC 变了、而我们看不懂的那些东西」。
    const drift = new DriftLedgerSection();
    footprintPage.appendChild(this.safeBlock("数据面漂移记账", () => drift.element));
    router.addRoute({
      id: "footprint",
      title: "改动足迹",
      element: footprintPage,
    });
    // 步 2：这一页也不是落地页 ⇒ 它那一发 IPC 同样推到「首次可见」。
    this.loadOnFirstVisit("footprint", () => drift.loadNow());
    // ⚠ `70 §10.5` #1 **判不了**：「足迹」搬走之后这个顶层页还留不留。
    //   剩下的 `DriftLedgerSection` 也**没有 origin**、计数还是本进程内的
    //   ⇒ 按那条顶层判据它其实属「应用」，那这个顶层页就空了。
    //   但 drift ledger 不在 `99 §3.2` 点的那三块射程里，**撤不撤这一页本件不定**。

    // S6：cc-bus 驾驶舱**已搬出设置**，成为顶层运营视图（入口 = 命令面板）。
    // S2 当初把它临时单列成一页，正是为了这一刻只删这一段注册 —— 兑现了。
    // 视图本体见 `views/cc-bus-view.ts`；驾驶舱组件 `CcBusSection` 原样复用，未重写。


    // ★ S4b-2：切到某台机器页时，把那几块 per-machine 分节搬进那一页，并同步
    // 「当前在看哪台机器」。挂在**路由器**这一层而不是列表行上，是因为切页有两个入口
    // （点导航项 / 点列表行），只有这里两条都覆盖得到。
    router.onNavigate((id) => {
      if (!id.startsWith(MACHINE_PAGE_PREFIX)) return;
      const isLocal = id === LOCAL_MACHINE_PAGE_ID;
      setCurrentMachine(isLocal ? null : id.slice(MACHINE_PAGE_PREFIX.length));
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
    for (const b of this.perMachineBlocks) {
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
    // ST1「延后加载」：兜底态下这几块就摆在（落地的）列表页上、用户看得见 ⇒ 这时才放它们的第一发。
    this.loadPerMachineOnce();
    this.perMachineSlot.hidden = false;
    this.perMachineFallbackHint.hidden = false;
    this.perMachineFallbackHint.removeAttribute("aria-busy");
    this.perMachineFallbackHint.dataset.fallback = "per-machine";
    this.perMachineFallbackHint.textContent =
      `${why} —— 下面这几块本该在每台机器自己的页面上，` +
      `现在临时留在这里。它们都还能用，只是位置不对。`;
  }

  /** 步 3：`RemoteSection` 那趟 `refresh()` 收尾了（成或败）。一个页都没来 ⇒ 兜底。 */
  private onMachinePagesSettled(): void {
    if (this.machinePageRegistered) return;
    this.revealPerMachineFallback("这台机器上的机器列表没读出来");
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
    autoLabel.textContent = "用户在终端里输入时自动切到对应 Tab";
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
    frontLabel.textContent = "自动切 Tab 时同时把 monitor 窗口拉到前台";
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
      "显示后台任务会话（⚙ 标识，挂在同项目会话之后；改动重启生效）";
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
    notifyLabel.textContent = "Claude 完成一轮时发系统通知（仅窗口在后台时）";
    notifyRow.appendChild(notifyLabel);
    group.appendChild(notifyRow);

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
      "本地 resume 命令",
      "默认：检测 cc，回退 claude",
      "历史浏览器 ↺ 在本机新终端里 resume 会话用的命令。\n留空 = 自动检测 PowerShell 的 cc 函数，没有则用 claude。",
    );
    this.resumeLocalInput = localInput;
    group.appendChild(localRow);
    this.resumeLocalPresetsBox = document.createElement("div");
    this.resumeLocalPresetsBox.className = "settings-presets resume-presets-local";
    group.appendChild(this.resumeLocalPresetsBox);
    const [remoteRow, remoteInput] = mkResumeRow(
      "远端 resume 命令",
      "默认：claude",
      "远端 resume / 起会话时实际敲的启动器。\n" +
        // 〔MC1〕「ccm 启动器」这个词删掉（`设计/71 §13`）：ccm 入口由机器页「部署后端」放。
        "推荐填 `ccm`（部署过后端就有）——tmux 与账号由 cc-monitor 经参数控制。\n" +
        // `70 §2.4` ＋ `§8` #5：这里原来是 `**…**` —— **界面不渲染 markdown**，
        // 那两对星号是连着一起显示给用户看的（截图 2 里那句 `**下一步：…**` 同一个病）。
        // ⇒ 强调改由句子结构承担（`§2.3` 那条「强调由 DOM 结构承担」的同一条道理）。
        "⚠ 别填 cct 这类自己建 tmux 的命令：它会另起一个 tmux，cc-monitor 设的账号 env\n" +
        "落在那个 tmux 进程边界之外、被整个吃掉，「用账号 X resume」就不生效。\n" +
        "留空 = claude。",
    );
    this.resumeRemoteInput = remoteInput;
    group.appendChild(remoteRow);
    this.resumeRemotePresetsBox = document.createElement("div");
    this.resumeRemotePresetsBox.className = "settings-presets resume-presets-remote";
    group.appendChild(this.resumeRemotePresetsBox);
    // F08：越层启动器诊断——只诊断+引导，不自动改这个输入框的值（MASTERPLAN 设计原则#7）。
    this.remoteLauncherWarning = document.createElement("div");
    this.remoteLauncherWarning.className = "settings-launcher-warning";
    this.remoteLauncherWarning.style.display = "none";
    group.appendChild(this.remoteLauncherWarning);
    remoteInput.addEventListener("input", () =>
      this.updateRemoteLauncherWarning(),
    );
    // 〔AL1 · 2026-09-24〕这里原来挂着两块别名（「按账号生成命令」·「生成自定义别名」）。
    // 它们合成一类、搬去了机器页「本机 → 工具 → 别名」（`设计/70 §3.3` · `设计/71 §13`）：
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
    openBtn.textContent = "打开快捷键编辑器";
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
    this.kbOverrideChip.textContent = n > 0 ? `已自定义 ${n} 项` : "全部默认";
  }

  /** "Claude 数据目录" 子表单（F82b 起嵌在「集成」组里） */
  private buildDataGroup(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group";

    // 子标题（跟同分组里的「PowerShell 集成」子标题对称）
    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = "Claude 数据目录";
    group.appendChild(heading);

    // 行 1：标签 + 文本输入
    const row1 = document.createElement("div");
    row1.className = "settings-row settings-row-stack";
    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = "目录路径";
    row1.appendChild(label);
    this.claudeDirInput = document.createElement("input");
    this.claudeDirInput.type = "text";
    this.claudeDirInput.className = "settings-input settings-input-wide";
    this.claudeDirInput.placeholder = "默认：~/.claude  或  $CLAUDE_CONFIG_DIR";
    row1.appendChild(this.claudeDirInput);
    group.appendChild(row1);

    // 行 2：操作按钮
    const row2 = document.createElement("div");
    row2.className = "settings-row settings-row-end";
    const pickBtn = document.createElement("button");
    pickBtn.type = "button";
    pickBtn.className = "settings-btn settings-btn-secondary";
    pickBtn.textContent = "浏览…";
    pickBtn.addEventListener("click", () => void this.pickClaudeDir());
    row2.appendChild(pickBtn);
    const resetBtn = document.createElement("button");
    resetBtn.type = "button";
    resetBtn.className = "settings-btn settings-btn-secondary";
    resetBtn.textContent = "重置";
    resetBtn.title = "清空 → 回退到 $CLAUDE_CONFIG_DIR 或 ~/.claude";
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
  private safeBlock(title: string, build: () => HTMLElement): HTMLElement {
    try {
      return this.titledSection(title, build());
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
      msg.textContent = `此区块加载失败：${String(e)}`;
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

  /**
   * S9：非 Windows 本机上「终端集成」那一格的替身。
   *
   * **不能就这么少一块**：主计划 §2.4 那张表里「本机 · 启动器」写的是
   * 「PowerShell 集成（未来 POSIX ccm）」—— Linux 上这一格**确实还空着**，
   * 空着的原因得说出来，否则用户只能猜是不是自己装漏了什么。
   *
   * 沿用 S5 `readiness.ts` 立的那条区分：**不适用 ≠ 缺**。所以这是一行
   * **静态说明**而不是 ⚠ —— S7 的判据表里静态说明本就不该做成警告。
   */
  private static ccIntegrationNotApplicable(): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "settings-group";
    wrap.dataset.naBlock = "终端集成";
    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = "终端集成";
    wrap.appendChild(heading);
    const msg = document.createElement("div");
    msg.className = "settings-hint";
    msg.textContent =
      "本机不是 Windows，PowerShell 集成不适用。" +
      "POSIX 这边的终端集成（ccm）目前只在远端机器的「组件」栏提供安装入口。";
    wrap.appendChild(msg);
    return wrap;
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
    resetBtn.textContent = "↺";
    resetBtn.title = `恢复 "${f.label}" 默认值`;
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
  }

  /** 把单个 token 当前值（如果覆盖了）或 :root 计算值写到对应 input */
  private syncOneInput(f: FieldSpec): void {
    const input = this.inputs.get(f.key);
    if (!input) return;
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

  private buildControl(f: FieldSpec): HTMLInputElement | HTMLSelectElement {
    if (f.type === "font-base" || f.type === "font-mono") {
      const sel = document.createElement("select");
      sel.className = "settings-input settings-input-select";
      const presets =
        f.type === "font-base" ? BASE_FONT_PRESETS : MONO_FONT_PRESETS;
      for (const p of presets) {
        const opt = document.createElement("option");
        opt.value = p.value;
        opt.textContent = p.label;
        // 控件预览：option 文字本身用对应字体显示
        if (p.value) opt.style.fontFamily = p.value;
        sel.appendChild(opt);
      }
      sel.addEventListener("change", () => this.onFieldChange(f, sel));
      return sel;
    }
    const input = document.createElement("input");
    input.type = f.type; // color / number / text
    input.className = "settings-input";
    input.addEventListener("input", () => this.onFieldChange(f, input));
    return input;
  }

  private buildFooter(): HTMLElement {
    const footer = document.createElement("div");
    footer.className = "settings-footer";
    footer.appendChild(
      this.makeBtn("恢复默认", "secondary", () => this.resetAll()),
    );
    footer.appendChild(this.makeBtn("取消", "secondary", () => this.cancel()));
    footer.appendChild(this.makeBtn("保存", "primary", () => this.save()));
    return footer;
  }

  private makeBtn(
    label: string,
    variant: "primary" | "secondary",
    onClick: () => void,
  ): HTMLElement {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = `settings-btn settings-btn-${variant}`;
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
      (this.current as Record<string, unknown>)[f.key] = n;
      nextValue = n;
    } else {
      (this.current as Record<string, unknown>)[f.key] = v;
      nextValue = v;
    }
    // 性能关键：拖 color picker 时 `input` 事件 ~60Hz 高频；只更新这一个 token，
    // 避免每帧调 14 次 setProperty 触发整棵 :root 子树重算
    applyThemeToken(f.key, nextValue);
  }

  /** 把 this.current 的值写回所有 input；无覆盖的字段读 :root 计算值作为占位 */
  private syncInputs(): void {
    for (const f of FIELDS) {
      this.syncOneInput(f);
    }
  }
}

/** input[type=color] 只接受 #rrggbb；过滤掉 #rgb / rgb()/font 串 */
function isShortHex(s: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(s);
}
