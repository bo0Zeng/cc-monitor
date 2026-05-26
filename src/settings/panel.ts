/**
 * 设置面板：外观（主题 / 字体）+ 数据目录（Claude 数据位置）。
 *
 * 解耦：
 *  - 外观：只调 theme.ts 的 applyTheme / loadTheme / saveTheme
 *  - 数据目录：只调 paths.ts 的 getClaudeDirOverride / setClaudeDirOverride
 *  - 不直接 setProperty、不直接 invoke
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
import { DiagnosticsSection, DIAGNOSTICS_INFO_TEXT } from "./diagnostics-section";
import { CollapsibleGroup } from "./collapsible-group";
import { DataSection, DATA_SECTION_INFO_TEXT } from "./data-section";
import { getBehavior, setBehavior, type BehaviorConfig } from "../behavior";

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
}

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

  // v2.4 issue #2: 行为类 toggle
  private autoFollowCheckbox!: HTMLInputElement;
  private bringFrontCheckbox!: HTMLInputElement;
  private onBehaviorChange?: (cfg: BehaviorConfig) => void;

  constructor(opts: SettingsPanelOptions = {}) {
    this.onBehaviorChange = opts.onBehaviorChange;
    this.el = this.build();
    document.body.appendChild(this.el);
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && this.isOpen) this.cancel();
    });
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
    this.updateBringFrontEnabled();
    this.banner.textContent = "";
    this.banner.classList.remove("settings-banner-show");
    this.syncInputs();
    // issue #3: 每次打开重拉一次 stat，让"已创建 / 文件大小"是最新的
    this.dataSection?.refresh();
    this.el.classList.add("open");
    this.isOpen = true;
  }

  /** v2.4 issue #2: autoFollow 关 → bringFront 灰显（依赖前者，无意义） */
  private updateBringFrontEnabled(): void {
    this.bringFrontCheckbox.disabled = !this.autoFollowCheckbox.checked;
  }

  /** v2.4 issue #2: 任一行为 toggle 改 → 立即 save + 通知 TabManager 同步 */
  private async onBehaviorToggle(): Promise<void> {
    this.updateBringFrontEnabled();
    const next: BehaviorConfig = {
      autoFollowUserActive: this.autoFollowCheckbox.checked,
      bringMonitorToFrontOnUserActive: this.bringFrontCheckbox.checked,
    };
    try {
      await setBehavior(next);
      this.onBehaviorChange?.(next);
    } catch (e) {
      console.warn("save behavior failed:", e);
    }
  }

  close(): void {
    this.el.classList.remove("open");
    this.isOpen = false;
  }

  private cancel(): void {
    applyTheme(this.original);
    this.close();
  }

  private async save(): Promise<void> {
    await saveTheme(this.current);
    this.original = { ...this.current };

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
    title.textContent = "外观设置";
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

    body.appendChild(this.buildDataGroup());
    // v2.4 issue #2: 行为类 toggle —— 自动切 tab / 拉前 monitor 窗口
    body.appendChild(this.buildBehaviorGroup());
    // v1.7：PowerShell 集成区（cc 命令注入式绑定）—— 高频，不折叠
    body.appendChild(new CcIntegrationSection().element);

    // issue #7：低频 section 收到 collapsible 里，默认折叠
    //
    // 外观（字体 + 颜色）—— 13 个字段，配一次几乎不再动
    const appearance = new CollapsibleGroup({
      id: "appearance",
      title: "外观",
      defaultCollapsed: true,
      infoTooltip:
        "字体（正文 / 等宽 / 字号）+ 颜色（10 个 token）。配一次后基本不再动，默认收起。",
    });
    appearance.appendChild(this.buildGroup("字体", FIELDS.filter((f) => f.group === "font")));
    appearance.appendChild(this.buildGroup("颜色", FIELDS.filter((f) => f.group === "color")));
    body.appendChild(appearance.element);

    // 诊断 —— "出问题才用" 的工具，默认折叠
    const diagnostics = new CollapsibleGroup({
      id: "diagnostics",
      title: "诊断",
      defaultCollapsed: true,
      infoTooltip: DIAGNOSTICS_INFO_TEXT,
    });
    diagnostics.appendChild(new DiagnosticsSection({ headless: true }).element);
    body.appendChild(diagnostics.element);

    // issue #3 (A): 数据存储 —— 透明展示所有持久路径 + WebView2 + localStorage
    // 默认折叠（用户偶尔好奇查时才看，平时不占视觉）
    const dataGroup = new CollapsibleGroup({
      id: "data-storage",
      title: "数据存储",
      defaultCollapsed: true,
      infoTooltip: DATA_SECTION_INFO_TEXT,
    });
    this.dataSection = new DataSection({ headless: true });
    dataGroup.appendChild(this.dataSection.element);
    body.appendChild(dataGroup.element);

    return body;
  }

  /**
   * v2.4 issue #2: 「行为」分组——自动切 tab + 可选拉前 monitor 窗口。
   *
   * 两个 toggle 都是热更新（不重启）。"拉前窗口" 在 "自动切 tab" 关闭时灰显
   * （前者依赖后者，单独开没意义）。
   */
  private buildBehaviorGroup(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group";

    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = "行为";
    group.appendChild(heading);

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

    const autoHint = document.createElement("div");
    autoHint.className = "settings-hint";
    autoHint.textContent =
      "watcher 反推：用户在 claude 里敲回车 → monitor 切到对应 session 的 Tab。" +
      "只跟「真用户输入」（不跟工具返回、不跟 claude 流式回复）。" +
      "你手动点 Tab 后 5 秒内不会被自动切抢回。";
    group.appendChild(autoHint);

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

    const frontHint = document.createElement("div");
    frontHint.className = "settings-hint";
    frontHint.textContent =
      "默认关：monitor 静静在后台切 Tab，不打断你正在用的其他窗口（浏览器/IDE）。" +
      "开启后：在终端输入时 monitor 浮上来抢焦。仅当上面的「自动切 Tab」开启时生效。";
    group.appendChild(frontHint);

    return group;
  }

  /** "数据" 分组：当前只有 Claude 数据目录一项 */
  private buildDataGroup(): HTMLElement {
    const group = document.createElement("div");
    group.className = "settings-group";

    const heading = document.createElement("div");
    heading.className = "settings-group-title";
    heading.textContent = "数据";
    group.appendChild(heading);

    // 行 1：标签 + 文本输入
    const row1 = document.createElement("div");
    row1.className = "settings-row settings-row-stack";
    const label = document.createElement("span");
    label.className = "settings-label";
    label.textContent = "Claude 数据目录";
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

    // 行 3：说明文字
    const hint = document.createElement("div");
    hint.className = "settings-hint";
    hint.textContent =
      "应指向 .claude 根目录（其下应有 projects/ 和 sessions/ 子目录）。修改后需重启 monitor 生效。";
    group.appendChild(hint);

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
