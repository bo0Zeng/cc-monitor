/**
 * 外观设置面板。
 *
 * 解耦：只调 theme.ts 的 applyTheme / loadTheme / saveTheme；
 * 不直接 setProperty、不直接 invoke。所有 CSS 变量映射逻辑都封装在 theme 模块里。
 */

import { applyTheme, loadTheme, saveTheme, type ThemeConfig } from "../theme";

type FieldType = "color" | "text" | "number";

interface FieldSpec {
  key: keyof ThemeConfig;
  label: string;
  type: FieldType;
  group: "font" | "color";
}

const FIELDS: ReadonlyArray<FieldSpec> = [
  { key: "font-base", label: "正文字体", type: "text", group: "font" },
  { key: "font-mono", label: "等宽字体", type: "text", group: "font" },
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

export class SettingsPanel {
  private el: HTMLElement;
  /** 当前编辑中的 theme（实时预览用） */
  private current: ThemeConfig = {};
  /** 打开时的 theme 快照，取消时回滚 */
  private original: ThemeConfig = {};
  private inputs = new Map<keyof ThemeConfig, HTMLInputElement>();
  private isOpen = false;

  constructor() {
    this.el = this.build();
    document.body.appendChild(this.el);
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && this.isOpen) this.cancel();
    });
  }

  async open(): Promise<void> {
    this.original = await loadTheme();
    this.current = { ...this.original };
    this.syncInputs();
    this.el.classList.add("open");
    this.isOpen = true;
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
    this.close();
  }

  private async resetAll(): Promise<void> {
    this.current = {};
    applyTheme({});
    await saveTheme({});
    this.original = {};
    this.syncInputs();
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
    body.appendChild(this.buildGroup("字体", FIELDS.filter((f) => f.group === "font")));
    body.appendChild(this.buildGroup("颜色", FIELDS.filter((f) => f.group === "color")));
    return body;
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

    const input = document.createElement("input");
    input.type = f.type;
    input.className = "settings-input";
    input.addEventListener("input", () => this.onFieldChange(f, input));
    row.appendChild(input);

    this.inputs.set(f.key, input);
    return row;
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

  private onFieldChange(f: FieldSpec, input: HTMLInputElement): void {
    const v = input.value;
    if (v === "") {
      delete this.current[f.key];
    } else if (f.type === "number") {
      (this.current as Record<string, unknown>)[f.key] = Number(v);
    } else {
      (this.current as Record<string, unknown>)[f.key] = v;
    }
    applyTheme(this.current);
  }

  /** 把 this.current 的值写回所有 input；无覆盖的字段读 :root 计算值作为占位 */
  private syncInputs(): void {
    const root = getComputedStyle(document.documentElement);
    for (const f of FIELDS) {
      const input = this.inputs.get(f.key);
      if (!input) continue;
      const override = this.current[f.key];
      if (override !== undefined && override !== null && override !== "") {
        input.value = String(override);
        continue;
      }
      // 无覆盖：读计算值反向填，作为占位（不写入 this.current）
      const cssVar = `--${f.key}`;
      const computed = root.getPropertyValue(cssVar).trim();
      if (f.type === "color") {
        input.value = isShortHex(computed) ? computed : "#000000";
      } else if (f.type === "number") {
        input.value = computed.replace(/px$/, "").trim() || "14";
      } else {
        input.value = computed;
      }
    }
  }
}

/** input[type=color] 只接受 #rrggbb；过滤掉 #rgb / rgb()/font 串 */
function isShortHex(s: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(s);
}
