/**
 * 输入框（C2）：单行 / 多行 / 带前缀。标签在上、说明在下；出错时错误句**替换**说明句的位置（不另起、不跳位）。
 *
 * 校验时机由调用方定（失焦 · 按确认）；打字时不报错，报过之后边打边消由调用方调 [`FieldHandle.setError`]`(null)`。
 */
import { icon } from "./icon";
import { spinner } from "./progress";
import s from "./field.module.css";

export interface FieldSpec {
  label: string;
  value?: string;
  /** 只做示例，不写说明。 */
  placeholder?: string;
  help?: string;
  multiline?: boolean;
  prefix?: string;
  /** 没说明句时下方那一行不留空（错误句出现时才占一行）。默认留着，错误句出现不跳位。 */
  noteOnDemand?: boolean;
  /** 框与说明 / 错误句之间的一行（如列表行里那台的地址）。 */
  aside?: Node;
}

export interface FieldHandle {
  root: HTMLDivElement;
  input: HTMLInputElement | HTMLTextAreaElement;
  setError(message: string | null): void;
  setValidating(on: boolean): void;
  setReadonly(on: boolean): void;
  setDisabled(why: string | null): void;
}

let seq = 0;

export function field(spec: FieldSpec): FieldHandle {
  const id = `kit-field-${++seq}`;
  const root = document.createElement("div");
  root.className = s.field;
  if (spec.noteOnDemand) root.dataset.noteOnDemand = "true";
  const label = document.createElement("label");
  label.className = s.fieldLabel;
  label.htmlFor = id;
  label.textContent = spec.label;
  const box = document.createElement("div");
  box.className = s.fieldBox;
  if (spec.prefix !== undefined) {
    const p = document.createElement("span");
    p.className = s.fieldPrefix;
    p.textContent = spec.prefix;
    box.appendChild(p);
  }
  const input = spec.multiline ? document.createElement("textarea") : document.createElement("input");
  input.id = id;
  input.className = s.fieldInput;
  if (input instanceof HTMLInputElement) input.type = "text";
  input.value = spec.value ?? "";
  if (spec.placeholder) input.placeholder = spec.placeholder;
  box.appendChild(input);
  const note = document.createElement("div");
  note.className = s.fieldNote;
  note.id = `${id}-note`;
  input.setAttribute("aria-describedby", note.id);
  const help = spec.help ?? "";
  note.textContent = help;
  root.append(label, box);
  if (spec.aside) root.appendChild(spec.aside);
  root.appendChild(note);
  let sp: HTMLElement | null = null;
  return {
    root,
    input,
    setError(message) {
      note.replaceChildren();
      if (message === null) {
        delete root.dataset.error;
        input.removeAttribute("aria-invalid");
        note.textContent = help;
        return;
      }
      root.dataset.error = "true";
      input.setAttribute("aria-invalid", "true");
      const t = document.createElement("span");
      t.textContent = message;
      note.append(icon("error", "compact"), t);
    },
    setValidating(on) {
      if (on && !sp) {
        sp = spinner();
        box.appendChild(sp);
      } else if (!on && sp) {
        sp.remove();
        sp = null;
      }
    },
    setReadonly(on) {
      input.readOnly = on;
      if (on) root.dataset.readonly = "true";
      else delete root.dataset.readonly;
    },
    setDisabled(why) {
      input.disabled = why !== null;
      box.title = why ?? "";
    },
  };
}
