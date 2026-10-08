/**
 * 恢复命令那一格（通用页默认 · 每台覆盖共用）：下拉。候选 ＝ 适配层画像里默认那一家的启动器（`defaultLauncher`，生成物来自后端注册表）
 * ＋ 你用过的自定义命令（通用页那份预设）＋「自定义…」（展开一格输入）。界面不写死任何一家的命令名。
 * 每台那一格多一个「通用设置」（留空 ＝ 跟通用页那一格）。选了 / 自定义那格失焦 ⇒ 回调存。
 */
import { copyText } from "../copy-table";
import { DEFAULT_AGENT, listAgents, lookupAgentProfile } from "../agent-profile";
import { select, type SelectHandle, type SelectOption } from "../kit/select";

/** 「自定义…」那一项的值（不会和命令撞：命令里没有 NUL）。 */
const CUSTOM = "\u0000custom";

/** 适配层画像给的启动器（默认那一家；问不到 ⇒ 空，不猜）。 */
export function launcherCandidates(): string[] {
  const out: string[] = [];
  for (const agent of listAgents()) {
    if (agent !== DEFAULT_AGENT) continue;
    const got = lookupAgentProfile(agent);
    if (got.known && !out.includes(got.facts.defaultLauncher)) out.push(got.facts.defaultLauncher);
  }
  return out;
}

export interface ResumeSelectOpts {
  /** 每台那一格：空值那一项叫「通用设置」；通用页那一格：`false`（空值 ＝ 默认那一家的启动器）。 */
  inherit: boolean;
  onChange: (value: string) => void;
}

export class ResumeSelect {
  readonly element: HTMLElement;
  private readonly box: SelectHandle;
  private readonly custom: HTMLInputElement;
  private readonly opts: ResumeSelectOpts;
  private current = "";
  private presets: string[] = [];

  constructor(opts: ResumeSelectOpts) {
    this.opts = opts;
    this.element = document.createElement("span");
    this.element.className = "resume-select";
    this.box = select({
      label: copyText("machineCard.field.resumeCmd"),
      options: [],
      onChange: (v) => {
        if (v === CUSTOM) {
          this.custom.hidden = false;
          this.custom.value = "";
          this.custom.focus();
          return;
        }
        this.custom.hidden = true;
        this.commit(v);
      },
    });
    this.box.el.dataset.role = "resume-select";
    this.custom = document.createElement("input");
    this.custom.type = "text";
    this.custom.className = "settings-input settings-input-mono";
    this.custom.dataset.role = "resume-custom";
    this.custom.placeholder = copyText("resumeSelect.custom.hint");
    this.custom.spellcheck = false;
    this.custom.autocomplete = "off";
    this.custom.hidden = true;
    this.custom.addEventListener("change", () => {
      const v = this.custom.value.trim();
      this.custom.hidden = true;
      if (v === "") this.paint();
      else this.commit(v);
    });
    this.element.append(this.box.el, this.custom);
    this.paint();
  }

  get value(): string {
    return this.current;
  }

  set disabled(on: boolean) {
    this.box.setDisabled(on);
    this.custom.disabled = on;
  }

  /** 照存着的值与预设重画（存着的值不在候选里 ⇒ 也列上，选中它）。 */
  set(value: string, presets: readonly string[] = this.presets): void {
    this.current = value.trim();
    this.presets = [...presets];
    this.custom.hidden = true;
    this.paint();
  }

  private commit(v: string): void {
    this.current = v;
    this.paint();
    this.opts.onChange(v);
  }

  private paint(): void {
    const launchers = launcherCandidates();
    const items: SelectOption[] = [];
    if (this.opts.inherit) items.push({ value: "", label: copyText("resumeSelect.option.inherit") });
    else items.push({ value: "", label: launchers[0] ?? copyText("resumeSelect.option.defaultBare"), note: launchers[0] ? copyText("resumeSelect.option.defaultBare") : undefined });
    for (const c of [...launchers.slice(this.opts.inherit ? 0 : 1), ...this.presets, this.current]) {
      if (c !== "" && !items.some((i) => i.value === c)) items.push({ value: c, label: c });
    }
    items.push({ value: CUSTOM, label: copyText("resumeSelect.option.custom") });
    this.box.setOptions(items, this.current);
  }
}
