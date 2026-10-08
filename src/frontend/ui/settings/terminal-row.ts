/**
 * 设置 → 通用 → 恢复 →「终端」那一行：开终端窗口用哪个终端（只在要挑的平台上出现，今天是 Linux）。
 *
 * 判定（探哪些 · 自动挑谁 · 设置怎么拼进命令）在壳的平台层，经 `terminal_choices` 交来；这里只画、只存 config.json 的 `terminal`
 * （空 ＝ 自动 · 一个探到的终端名 · 或一串命令前缀）。形状照恢复命令那一格：下拉（自动 · 探到的各个 · 自定义…）＋ 自定义展开一格。
 */
import { commands } from "../ipc/commands";
import { patchConfig, removeAt, setAt } from "../config";
import { copyText } from "../copy-table";
import { select, type SelectOption } from "../kit/select";
import type { TerminalChoices } from "../generated/TerminalChoices";

/** 「自定义…」那一项的值（不会和终端名撞：名字里没有 NUL）。 */
const CUSTOM = "\u0000custom";

/** 那一行今天该停在哪一项：空 ⇒ 自动；是探到的某个 ⇒ 它；别的 ⇒ 自定义（输入框里是原值）。纯函数。 */
export function selectedOf(c: TerminalChoices): { value: string; custom: string } {
  if (c.setting === "") return { value: "", custom: "" };
  if (c.found.includes(c.setting)) return { value: c.setting, custom: "" };
  return { value: CUSTOM, custom: c.setting };
}

/** 下拉里的几项：自动（灰字：它会挑谁 / 未找到）· 探到的各个 · 自定义…。纯函数。 */
export function optionsOf(c: TerminalChoices): SelectOption[] {
  const items: SelectOption[] = [
    { value: "", label: copyText("settingsPanel.terminal.auto"), note: c.auto ?? copyText("settingsPanel.terminal.autoNone") },
  ];
  for (const name of c.found) items.push({ value: name, label: name });
  items.push({ value: CUSTOM, label: copyText("settingsPanel.terminal.custom") });
  return items;
}

/** 画出那一行（不该出现 ⇒ `null`）。存失败 ⇒ 行下一句。 */
export async function buildTerminalRow(): Promise<HTMLElement | null> {
  const c = await commands.terminal_choices();
  if (!c.applies) return null;

  const box = document.createElement("div");
  box.dataset.anchor = "terminal";
  const row = document.createElement("label");
  row.className = "settings-row";
  const text = document.createElement("span");
  text.className = "settings-label";
  text.textContent = copyText("settingsPanel.terminal.label");
  const help = document.createElement("span");
  help.className = "settings-hint";
  help.textContent = copyText("settingsPanel.terminal.help");
  const textCol = document.createElement("span");
  textCol.className = "settings-label-col";
  textCol.append(text, help);
  row.appendChild(textCol);

  const start = selectedOf(c);
  const custom = document.createElement("input");
  custom.type = "text";
  custom.className = "settings-input settings-input-mono";
  custom.dataset.role = "terminal-custom";
  custom.placeholder = copyText("settingsPanel.terminal.customPlaceholder");
  custom.spellcheck = false;
  custom.autocomplete = "off";
  custom.value = start.custom;
  custom.hidden = start.value !== CUSTOM;
  const err = document.createElement("div");
  err.className = "settings-row-error";
  err.hidden = true;

  const save = (value: string): void => {
    err.hidden = true;
    const edit = value === "" ? removeAt(["terminal"]) : setAt(["terminal"], value);
    patchConfig([edit]).catch((e: unknown) => {
      err.textContent = copyText("settings.behavior.saveFailedLine", { why: String(e) });
      err.hidden = false;
    });
  };
  const pick = select({
    label: copyText("settingsPanel.terminal.label"),
    options: optionsOf(c),
    value: start.value,
    onChange: (v) => {
      custom.hidden = v !== CUSTOM;
      if (v !== CUSTOM) {
        save(v);
        return;
      }
      custom.focus();
      if (custom.value.trim() !== "") save(custom.value.trim());
    },
  });
  pick.el.dataset.role = "terminal-select";
  // 失焦 / 回车才存（不逐键写盘）；清空 ＝ 回到自动。
  custom.addEventListener("change", () => {
    const v = custom.value.trim();
    if (v === "") {
      pick.setValue("");
      custom.hidden = true;
    }
    save(v);
  });
  const control = document.createElement("span");
  control.className = "resume-select";
  control.append(pick.el, custom);
  row.appendChild(control);
  box.append(row, err);
  return box;
}
