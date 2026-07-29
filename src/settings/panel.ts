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
import { CcBusSection } from "./cc-bus-section"; // B03：cc-bus 驾驶舱（只读，按需读，无轮询）
import { DiagnosticsSection } from "./diagnostics-section";
import { CollapsibleGroup } from "./collapsible-group";
import { DataSection } from "./data-section";
import { RemoteSection } from "./remote-section";
import { getBehavior, setBehavior, type BehaviorConfig } from "../behavior";
import { diagnoseRemoteLauncher, buildAliasGeneratorSection } from "../launcher-diagnostics";
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
  { label: "Microsoft YaHei UI", value: "'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif" },
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
  { key: "font-size-base", label: "基础字号 (px)", type: "number", group: "font" },
  { key: "bg", label: "主背景", type: "color", group: "color" },
  { key: "bg-2", label: "次背景", type: "color", group: "color" },
  { key: "card", label: "卡片", type: "color", group: "color" },
  { key: "text", label: "主文本", type: "color", group: "color" },
  { key: "text-2", label: "次文本", type: "color", group: "color" },
  { key: "user", label: "用户色", type: "color", group: "color" },
  { key: "assistant", label: "Claude 色", type: "color", group: "color" },
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

const INTEGRATION_INFO_TEXT =
  "monitor 怎么对接 Claude Code：\n\n" +
  "「Claude 数据目录」—— monitor 监听的 .claude 根目录（下含 projects/ 和 sessions/）。" +
  "默认 ~/.claude 或 $CLAUDE_CONFIG_DIR。修改后需重启 monitor 生效。\n\n" +
  "「PowerShell 集成」—— 把 cc 命令注入到 $PROFILE，让你打 `cc` 而不是 `claude` 启动 " +
  "Claude Code，自动跟 monitor 双向绑定（拉前终端按钮才能 work）。可一键安装/卸载。";

const APPEARANCE_INFO_TEXT =
  "字体（正文 / 等宽 / 字号）+ 颜色（10 个语义 token：背景 / 卡片 / 文字 / user / assistant 等）。" +
  "配一次基本不再动，所以默认收起。";

const REMOTE_INFO_TEXT =
  "「远端 (SSH)」—— monitor 通过 SSH 连到远端主机，由远端 daemon 取代本地 " +
  "jsonl-watcher 作为数据源（渲染 / Tab / 分支等行为完全相同）。\n\n" +
  "关闭（默认）时一切走本地，不受影响。启用 / 修改任意远端设置后需重启 monitor 才生效。" +
  "配置不完整（缺 host / user / daemonPath）时后端自动回退本地模式。";

const DIAG_STORAGE_INFO_TEXT =
  "「诊断」—— 打开后端 INFO 级别 tracing 到状态栏（开发用；出问题排查时打开）。\n\n" +
  "「数据存储」—— 透明展示 monitor 自身写入的所有持久化路径：config.json / history-metadata.json / " +
  "WebView2 UserDataFolder / localStorage keys 等。每项可点 [打开] 直接到文件管理器。" +
  "纯展示，无危险操作。";

// F82b（#56+#47）：4 组终态的合并 tooltip——外观并了 行为/快捷键、集成并了 诊断&存储，
// group 级 tooltip 把原分组说明拼一起（子分节各带小标题导航）。
const APPEARANCE_GROUP_INFO_TEXT =
  APPEARANCE_INFO_TEXT + "\n\n【行为】" + BEHAVIOR_INFO_TEXT + "\n\n【快捷键】" + KEYBINDINGS_INFO_TEXT;
const INTEGRATION_GROUP_INFO_TEXT =
  INTEGRATION_INFO_TEXT + "\n\n【诊断 & 存储】" + DIAG_STORAGE_INFO_TEXT;
const REMOTE_GROUP_INFO_TEXT =
  "远端会话「连上之后」的行为与历史相关设置。当前尚无独立项（resume 命令等在「外观 → 行为」里），" +
  "留空占位；后续远端会话行为 / 历史项加入本组。（「连上远端」的 SSH 连接配置在上面的「连接」组。）";

export class SettingsPanel {
  private el: HTMLElement;
  /** 当前编辑中的 theme（实时预览用） */
  private current: ThemeConfig = {};
  /** 打开时的 theme 快照，取消时回滚 */
  private original: ThemeConfig = {};
  private inputs = new Map<keyof ThemeConfig, HTMLInputElement | HTMLSelectElement>();
  private isOpen = false;

  /** Claude 数据目录输入框 —— 改动后保存会提示需要重启 */
  private claudeDirInput!: HTMLInputElement;
  /** 打开时 claudeDir 的快照，用于判断是否变化（变了就提示重启） */
  private claudeDirOriginal: string = "";
  /** 顶部状态提示行（保存成功 / 需重启 等） */
  private banner!: HTMLElement;
  /** issue #3 (A): 数据存储展示区。打开面板时 refresh 一次拉最新 stat */
  private dataSection?: DataSection;
  /** issue #15 (S6): 远端 (SSH) 配置区。打开面板时 refresh 一次拉最新 config */
  private remoteSection?: RemoteSection;

  // v2.4 issue #2: 行为类 toggle
  private autoFollowCheckbox!: HTMLInputElement;
  private showBgCheckbox!: HTMLInputElement;
  private notifyTurnEndCheckbox!: HTMLInputElement;
  // F34：自定义 resume 命令（本地 / 远端）
  private resumeLocalInput!: HTMLInputElement;
  private resumeRemoteInput!: HTMLInputElement;
  private remoteLauncherWarning!: HTMLElement; // F08：越层启动器诊断提示（只诊断，不代改）
  private bringFrontCheckbox!: HTMLInputElement;
  /** F03（unify-launch）：`forceLegacyLaunchRenderer` 无 UI 暴露（手改 config.json 的逃生口），
   *  但 `onBehaviorToggle` 每次都要交一份完整 `BehaviorConfig`——缓存 open() 时读到的值原样带回，
   *  防止面板任何一个勾选框变动都把它悄悄重置成 DEFAULTS 里的 false。 */
  private forceLegacyLaunchRenderer = false;
  private onBehaviorChange?: (cfg: BehaviorConfig) => void;
  /** F82a：见 SettingsPanelOptions.windowMode。 */
  private readonly windowMode: boolean;

  // issue #5: 快捷键编辑器（lazy 构造，首次打开时建 DOM）
  private kbEditor?: KeybindingsEditor;
  private kbOverrideChip?: HTMLElement;

  constructor(opts: SettingsPanelOptions = {}) {
    this.onBehaviorChange = opts.onBehaviorChange;
    this.windowMode = opts.windowMode ?? false;
    this.el = this.build();
    document.body.appendChild(this.el);
    // issue #5: Esc 由 KeybindingDispatcher 统一调度。本面板 open 时
    // pushOverlay 自己，close 时 pop —— 多弹层共存按 LIFO 顺序关。
  }

  /** dispatcher overlay 接口 */
  handleEsc(): void {
    if (this.isOpen) this.cancel();
  }

  async open(): Promise<void> {
    this.original = await loadTheme();
    this.current = { ...this.original };
    this.claudeDirOriginal = (await getClaudeDirOverride()) ?? "";
    this.claudeDirInput.value = this.claudeDirOriginal;
    // v2.4 issue #2: 每次打开拉最新 behavior，避免跟外部其他改动脱节
    const behavior = await getBehavior();
    this.autoFollowCheckbox.checked = behavior.autoFollowUserActive;
    this.bringFrontCheckbox.checked = behavior.bringMonitorToFrontOnUserActive;
    this.showBgCheckbox.checked = behavior.showBgSessions;
    this.notifyTurnEndCheckbox.checked = behavior.notifyTurnEnd;
    this.resumeLocalInput.value = behavior.resumeCommandLocal;
    this.resumeRemoteInput.value = behavior.resumeCommandRemote;
    this.updateRemoteLauncherWarning();
    this.forceLegacyLaunchRenderer = behavior.forceLegacyLaunchRenderer;
    this.updateBringFrontEnabled();
    this.banner.textContent = "";
    this.banner.classList.remove("settings-banner-show");
    this.syncInputs();
    // issue #3: 每次打开重拉一次 stat，让"已创建 / 文件大小"是最新的
    this.dataSection?.refresh();
    // issue #15 (S6): 每次打开重拉 config.json 的 remote 子对象，跟外部改动对齐
    void this.remoteSection?.refresh();
    // issue #5: 同步快捷键覆盖数 chip（编辑器关闭时也可能改了）
    this.refreshKbChip();
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

  /** v2.4 issue #2: 任一行为 toggle 改 → 立即 save + 通知 TabManager 同步 */
  private async onBehaviorToggle(): Promise<void> {
    this.updateBringFrontEnabled();
    const next: BehaviorConfig = {
      autoFollowUserActive: this.autoFollowCheckbox.checked,
      bringMonitorToFrontOnUserActive: this.bringFrontCheckbox.checked,
      showBgSessions: this.showBgCheckbox.checked,
      resumeCommandLocal: this.resumeLocalInput.value.trim(),
      resumeCommandRemote: this.resumeRemoteInput.value.trim(),
      notifyTurnEnd: this.notifyTurnEndCheckbox.checked,
      forceLegacyLaunchRenderer: this.forceLegacyLaunchRenderer,
    };
    try {
      await setBehavior(next);
      this.onBehaviorChange?.(next); // 同窗（主窗口浮层）直接同步 TabManager
      this.broadcastApplied(); // 窗口模式：广播让主窗口 applyBehavior
    } catch (e) {
      console.warn("save behavior failed:", e);
    }
  }

  close(): void {
    // 关闭前 blur 掉面板内仍聚焦的输入框。本面板是 hide（移除 .open class）而非从 DOM
    // 移除，元素留着、焦点不会自动释放。若不 blur，document.activeElement 仍是这个隐藏
    // 输入，单键快捷键守卫（registry.ts::isEditableTarget）会把它当"正在打字"，从而吞掉
    // 所有单键快捷键（h/w/数字… 全部失效）—— 用户配置完远端/外观关掉设置后最典型。
    const active = document.activeElement;
    if (active instanceof HTMLElement && this.el.contains(active)) active.blur();
    // 窗口模式：关闭 = 关掉这个独立设置窗口（而非隐藏浮层）。
    if (this.windowMode) {
      void getCurrentWindow().close();
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
      this.banner.textContent =
        "Claude 数据目录已更新 —— 需要重启 monitor 才能生效";
      this.banner.classList.add("settings-banner-show");
      return; // 不关面板，让用户看到提示
    }
    this.close();
  }

  private async resetAll(): Promise<void> {
    if (!window.confirm("确定要恢复全部外观默认？已保存的颜色和字体偏好会丢失。")) {
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
    root.appendChild(this.buildBody());
    root.appendChild(this.buildFooter());

    return root;
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
    close.addEventListener("click", () => this.cancel());
    header.appendChild(close);
    return header;
  }

  private buildBody(): HTMLElement {
    const body = document.createElement("div");
    body.className = "settings-body";

    // 顶部状态条（保存提示 / 重启提示等）
    this.banner = document.createElement("div");
    this.banner.className = "settings-banner";
    body.appendChild(this.banner);

    // F82b（#56+#47）：**4 组终态**（连接 / 外观 / 远端 / 集成）。用户 2026-07-17 拍板「硬落 4 组，
    // 连接=SSH、远端留空占位」。原 6 组合并：行为 + 快捷键 → 外观；诊断 & 存储 → 集成。端口转发（F58
    // 独立视图）/ SSH config 导入（F89）/ 拉前折叠（F81）落地后进「连接」；MCP（F87）进「集成」。
    // build*Group 只产出表单本体，CollapsibleGroup / titledSection 接管标题与描述。

    // 1. 连接 —— 怎么连上远端（当前只有 SSH 数据源配置一块；F58/F89/F81 落地后补入本组）
    const connection = new CollapsibleGroup({
      id: "connection",
      title: "连接",
      defaultCollapsed: true,
      infoTooltip: REMOTE_INFO_TEXT,
    });
    this.remoteSection = new RemoteSection({ headless: true });
    connection.appendChild(this.remoteSection.element);
    body.appendChild(connection.element);

    // 2. 外观 —— 行为 + 快捷键 + 字体 + 颜色（默认展开，保留「行为」的高可达性）
    const appearance = new CollapsibleGroup({
      // F82b：用新 id（旧 `appearance` 只含字体+颜色，且此组现默认展开）——避免返回用户旧的
      // collapsed 状态套到语义已变的合并组上、抵消「默认展开保『行为』可达」的意图。
      id: "appearance-4grp",
      title: "外观",
      defaultCollapsed: false,
      infoTooltip: APPEARANCE_GROUP_INFO_TEXT,
    });
    appearance.appendChild(this.titledSection("行为", this.buildBehaviorGroup()));
    appearance.appendChild(this.titledSection("快捷键", this.buildKeybindingsGroup()));
    appearance.appendChild(this.buildGroup("字体", FIELDS.filter((f) => f.group === "font")));
    appearance.appendChild(this.buildGroup("颜色", FIELDS.filter((f) => f.group === "color")));
    body.appendChild(appearance.element);

    // 3. 远端 —— 连上后的行为 & 历史。当前无独立设置（resume 命令等在「外观 → 行为」里），留空占位（用户拍板）。
    // A3：多账号「账号」组（占用原「远端」空占位组，id 沿用避免影响用户折叠状态）。
    // 展示远端账号 + 设为本机默认（只读 + 改本机默认账号，不注入/不重启——A4/A5）。
    const accountsGroup = new CollapsibleGroup({
      id: "remote-placeholder",
      title: "账号",
      defaultCollapsed: true,
      infoTooltip: REMOTE_GROUP_INFO_TEXT,
    });
    accountsGroup.appendChild(new AccountsSection().element);
    body.appendChild(accountsGroup.element);

    // 4. 集成 —— Claude 数据源 + PowerShell + MCP（F87）+ 诊断 & 存储
    const integration = new CollapsibleGroup({
      id: "integration",
      title: "集成",
      defaultCollapsed: true,
      infoTooltip: INTEGRATION_GROUP_INFO_TEXT,
    });
    integration.appendChild(this.buildDataGroup());
    integration.appendChild(new CcIntegrationSection().element);
    // F87（#50+#51）：MCP 管理——读跨 scope 展示 / 写只项目 .mcp.json（SS-14）。
    integration.appendChild(this.titledSection("MCP", new McpSection().element));
    // B03 批一：cc-bus 驾驶舱——只读看远端登记过的 agent；登记≠在线；点「读取」才发请求。
    integration.appendChild(this.titledSection("cc-bus", new CcBusSection().element));
    integration.appendChild(
      this.titledSection("诊断", new DiagnosticsSection({ headless: true }).element),
    );
    this.dataSection = new DataSection({ headless: true });
    integration.appendChild(this.titledSection("数据存储", this.dataSection.element));
    body.appendChild(integration.element);

    return body;
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
    this.autoFollowCheckbox.addEventListener("change", () => void this.onBehaviorToggle());
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
    this.bringFrontCheckbox.addEventListener("change", () => void this.onBehaviorToggle());
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
    this.showBgCheckbox.addEventListener("change", () => void this.onBehaviorToggle());
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
    this.notifyTurnEndCheckbox.addEventListener("change", () => void this.onBehaviorToggle());
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
    const [remoteRow, remoteInput] = mkResumeRow(
      "远端 resume 命令",
      "默认：claude",
      "远端 resume / 起会话时实际敲的启动器。\n"
        + "推荐填 `ccm`（装了「ccm 启动器」后可用）——tmux 与账号由 cc-monitor 经参数控制。\n"
        + "**别填 cct 这类自己建 tmux 的命令**：它会另起一个 tmux，cc-monitor 设的账号 env\n"
        + "落在那个 tmux 进程边界之外、被整个吃掉，「用账号 X resume」就不生效。\n"
        + "留空 = claude。",
    );
    this.resumeRemoteInput = remoteInput;
    group.appendChild(remoteRow);
    // F08：越层启动器诊断——只诊断+引导，不自动改这个输入框的值（MASTERPLAN 设计原则#7）。
    this.remoteLauncherWarning = document.createElement("div");
    this.remoteLauncherWarning.className = "settings-launcher-warning";
    this.remoteLauncherWarning.style.display = "none";
    group.appendChild(this.remoteLauncherWarning);
    remoteInput.addEventListener("input", () => this.updateRemoteLauncherWarning());
    // F08 Phase D 审计（重要项修复）：别名生成器紧挨着诊断放在同一处——此前生成器藏在
    // "远端 (SSH)"每台主机卡片的三层折叠里、且按主机重复渲染（内容与选中哪台机器无关），
    // 诊断提示也从未指向它。两者是同一段用户旅程的两半，理应彼此相邻。
    group.appendChild(buildAliasGeneratorSection());

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

  private buildGroup(title: string, fields: ReadonlyArray<FieldSpec>): HTMLElement {
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
      const presets = f.type === "font-base" ? BASE_FONT_PRESETS : MONO_FONT_PRESETS;
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
    footer.appendChild(this.makeBtn("恢复默认", "secondary", () => this.resetAll()));
    footer.appendChild(this.makeBtn("取消", "secondary", () => this.cancel()));
    footer.appendChild(this.makeBtn("保存", "primary", () => this.save()));
    return footer;
  }

  private makeBtn(label: string, variant: "primary" | "secondary", onClick: () => void): HTMLElement {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = `settings-btn settings-btn-${variant}`;
    btn.textContent = label;
    btn.addEventListener("click", onClick);
    return btn;
  }

  // === 数据同步 ===

  private onFieldChange(f: FieldSpec, input: HTMLInputElement | HTMLSelectElement): void {
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
