/**
 * 对话框（C10）：全产品「要不要做 / 叫什么」只问这一处。只在要你做决定、不决定就不能往下走时用；告知类不用它。
 *
 * - 结构：标题（动宾、不带问号）· 正文（逐项 `中断` / `保留` / `改动`，或一段限高可滚的清单，超过 8 项只列前 8 ＋「另外 n 个」）·
 *   按钮行（右对齐，取消在左、确认在右，确认键写动作名）。
 * - 焦点：开时落在第一个输入框；没有输入框落在非危险的那颗（危险确认落「取消」）。Tab 只在框内循环；Esc ＝ 取消；
 *   关掉后焦点回到打开它的控件。
 * - 点遮罩：没填东西 ＝ 取消；填了东西不关（防丢输入）。
 * - 同一时刻只有一个：新开之前把旧的按取消结算（取消是安全方向）。
 * - 模态：压进弹层栈，开着时快捷键只放行 Esc。
 * - 不能用 `window.confirm`：真 app 里它被换成恒真的异步替身，从来不拦（判据钉生产代码零处原生 `confirm` / `prompt`）。
 */
import { dispatcher, type OverlayHandle } from "../keybindings/registry";
import { button, buttonRow, setBusy, setButtonLabel, setDisabled } from "./button";
import { banner } from "./banner";
import { icon } from "./icon";
import { copyText } from "../copy-table";
import s from "./dialog.module.css";

/** 一段逐项清单：`中断` · `保留` · `改动`，各带名字。 */
export interface DialogRow {
  label: string;
  items: string[];
}

export interface ConfirmSpec {
  title: string;
  /** 动作名（`结束` · `删除` · `仍然退出`），不写「确定」。 */
  action: string;
  danger?: boolean;
  /** 一句正文（只说后果）。 */
  body?: string;
  rows?: DialogRow[];
  /** 一批对象（限高可滚，超过 8 项只列前 8）。 */
  list?: string[];
  /** 取消键的名字（不给 ⇒「取消」）：不做那件事本身就是一个动作时用（「不连接」）。 */
  cancel?: string;
}

/** 「问一句要不要做」的注入缝（测试注入 `() => true`）；调用方一律 `await`。 */
export type ConfirmFn = (spec: ConfirmSpec) => boolean | Promise<boolean>;

export interface TextSpec {
  title: string;
  action: string;
  label?: string;
  initial?: string;
  /** 确认前校验：回一句错误 ⇒ 写在框里、不关。 */
  validate?: (value: string) => string | null;
}

/** 清单最多列几项。 */
export const LIST_MAX = 8;

let pendingCancel: (() => void) | null = null;

interface Built {
  backdrop: HTMLDivElement;
  panel: HTMLDivElement;
  body: HTMLDivElement;
  cancel: HTMLButtonElement;
  ok: HTMLButtonElement;
}

function build(title: string, action: string, danger: boolean): Built {
  const backdrop = document.createElement("div");
  backdrop.className = s.dialogBackdrop;
  const panel = document.createElement("div");
  panel.className = s.dialogPanel;
  panel.setAttribute("role", danger ? "alertdialog" : "dialog");
  panel.setAttribute("aria-modal", "true");
  const h = document.createElement("h2");
  h.className = s.dialogTitle;
  h.id = `kit-dialog-title`;
  h.textContent = title;
  panel.setAttribute("aria-labelledby", h.id);
  const body = document.createElement("div");
  body.className = s.dialogBody;
  const cancel = button({ label: copyText("kit.dialog.cancel") });
  const ok = button({ label: action, kind: danger ? "danger" : "primary" });
  panel.append(h, body, buttonRow(cancel, ok));
  backdrop.appendChild(panel);
  return { backdrop, panel, body, cancel, ok };
}

function fillBody(body: HTMLElement, spec: ConfirmSpec): void {
  if (spec.body) {
    const p = document.createElement("p");
    p.className = s.dialogText;
    p.textContent = spec.body;
    body.appendChild(p);
  }
  const rows = (spec.rows ?? []).filter((r) => r.items.length > 0);
  if (rows.length > 0) {
    // 两列成表：标签一列（各行同宽），值一列左对齐。
    const table = document.createElement("div");
    table.className = s.dialogRows;
    for (const r of rows) {
      const row = document.createElement("div");
      row.className = s.dialogRow;
      const k = document.createElement("span");
      k.className = s.dialogRowLabel;
      k.textContent = r.label;
      const v = document.createElement("span");
      v.className = s.dialogRowValue;
      v.textContent = r.items.join(copyText("kit.text.sep"));
      row.append(k, v);
      table.appendChild(row);
    }
    body.appendChild(table);
  }
  if (spec.list?.length) {
    const box = document.createElement("div");
    box.className = s.dialogList;
    for (const it of spec.list.slice(0, LIST_MAX)) {
      const d = document.createElement("div");
      d.textContent = it;
      box.appendChild(d);
    }
    if (spec.list.length > LIST_MAX) {
      const more = document.createElement("div");
      more.className = s.dialogMore;
      more.textContent = copyText("kit.dialog.more", { n: spec.list.length - LIST_MAX });
      box.appendChild(more);
    }
    body.appendChild(box);
  }
}

function run<T>(
  b: Built,
  cancelled: T,
  onOk: () => T | undefined | Promise<T | undefined>,
  first: HTMLElement,
  dirty: () => boolean,
): Promise<T> {
  pendingCancel?.();
  const before = document.activeElement;
  return new Promise<T>((resolve) => {
    let settled = false;
    const overlay: OverlayHandle = {
      modal: true,
      handleEsc: () => {
        finish(cancelled);
        return true;
      },
    };
    const finish = (v: T): void => {
      if (settled) return;
      settled = true;
      if (pendingCancel === cancelSelf) pendingCancel = null;
      dispatcher.popOverlay(overlay);
      b.backdrop.remove();
      if (before instanceof HTMLElement && before.isConnected) before.focus();
      resolve(v);
    };
    const cancelSelf = (): void => finish(cancelled);
    pendingCancel = cancelSelf;
    b.backdrop.addEventListener("mousedown", (ev) => {
      if (ev.target === b.backdrop && !dirty()) finish(cancelled);
    });
    b.cancel.addEventListener("click", () => finish(cancelled));
    b.ok.addEventListener("click", () => {
      if (b.ok.getAttribute("aria-disabled") === "true" || b.ok.dataset.busy === "true") return;
      const v = onOk();
      if (!(v instanceof Promise)) {
        if (v !== undefined) finish(v);
        return;
      }
      setBusy(b.ok, b.ok.textContent ?? "");
      void v.then(
        (r) => {
          setBusy(b.ok, null);
          if (r !== undefined) finish(r);
        },
        () => setBusy(b.ok, null),
      );
    });
    b.panel.addEventListener("keydown", (ev) => {
      if (ev.key !== "Tab") return;
      const f = [...b.panel.querySelectorAll<HTMLElement>("button, input, textarea, select")].filter((e) => !e.hasAttribute("disabled"));
      if (f.length === 0) return;
      const i = f.indexOf(document.activeElement as HTMLElement);
      const next = ev.shiftKey ? (i <= 0 ? f.length - 1 : i - 1) : i === f.length - 1 ? 0 : i + 1;
      ev.preventDefault();
      f[next].focus();
    });
    dispatcher.pushOverlay(overlay);
    document.body.appendChild(b.backdrop);
    first.focus();
  });
}

/** 确认一件事。确认 ⇒ `true`；取消 · 点遮罩 · Esc · 被下一个对话框顶掉 ⇒ `false`。 */
export function confirmDialog(spec: ConfirmSpec): Promise<boolean> {
  const b = build(spec.title, spec.action, spec.danger === true);
  if (spec.cancel) setButtonLabel(b.cancel, spec.cancel);
  fillBody(b.body, spec);
  // 危险的、或逐项列出会断 / 会改什么的框：默认焦点在［取消］。
  const listsEffects = (spec.rows ?? []).some((r) => r.items.length > 0);
  return run(b, false, () => true, spec.danger || listsEffects ? b.cancel : b.ok, () => false);
}

/** 填一个值。确认（或单行框里 Enter）⇒ 框里的值（原样，不 trim）；取消一类 ⇒ `null`。校验不过 ⇒ 错误写在框里、不关。 */
export function askText(spec: TextSpec): Promise<string | null> {
  const b = build(spec.title, spec.action, false);
  const wrap = document.createElement("div");
  wrap.className = s.dialogFieldWrap;
  const input = document.createElement("input");
  input.type = "text";
  input.className = s.dialogInput;
  input.value = spec.initial ?? "";
  if (spec.label) input.setAttribute("aria-label", spec.label);
  const err = document.createElement("div");
  err.className = s.dialogError;
  err.hidden = true;
  wrap.append(input, err);
  b.body.appendChild(wrap);
  const submit = (): string | undefined => {
    const why = spec.validate?.(input.value) ?? null;
    if (why === null) return input.value;
    err.replaceChildren(icon("error", "compact"), document.createTextNode(why));
    err.hidden = false;
    input.setAttribute("aria-invalid", "true");
    input.focus();
    return undefined;
  };
  input.addEventListener("input", () => {
    if (err.hidden) return;
    err.hidden = true;
    input.removeAttribute("aria-invalid");
  });
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" && !ev.isComposing) {
      ev.preventDefault();
      b.ok.click();
    }
  });
  const p = run<string | null>(b, null, submit, input, () => input.value !== (spec.initial ?? ""));
  input.select();
  return p;
}

export interface FormSpec {
  title: string;
  /** 主按钮上的字（可随内容改：`添加 2 台`）。 */
  action: string;
  body: HTMLElement;
  /** 宽框（580）：表单 · 清单。 */
  wide?: boolean;
  /** 打开时焦点落在哪；缺省第一个可填的格。 */
  first?: () => HTMLElement | null;
  /** 主按钮可不可点：`null` ⇒ 可；一句话 ⇒ 禁用、悬停说为什么。内容变了由宿主调 `refresh()`。 */
  blocked?: () => string | null;
  /** 点主按钮：`null` ⇒ 成了、关框；一句话 ⇒ 框顶一条错误、不关、填的都在。 */
  submit: () => Promise<string | null>;
  /** 填过东西：点遮罩不关。 */
  dirty?: () => boolean;
}

export interface FormHandle {
  /** 成了 ⇒ `true`；取消 · Esc · 遮罩 ⇒ `false`。 */
  done: Promise<boolean>;
  refresh(): void;
  setAction(label: string): void;
}

/** 一张表单对话框（添加机器这一类）：内容由调用方建，框管按钮 · 错误条 · Esc · 焦点。 */
export function formDialog(spec: FormSpec): FormHandle {
  const b = build(spec.title, spec.action, false);
  if (spec.wide) b.panel.dataset.size = "wide";
  const errBox = document.createElement("div");
  b.body.append(errBox, spec.body);
  const refresh = (): void => setDisabled(b.ok, spec.blocked?.() ?? null);
  refresh();
  const submit = async (): Promise<boolean | undefined> => {
    const why = await spec.submit();
    if (why === null) return true;
    errBox.replaceChildren(banner("error", why));
    return undefined;
  };
  const first =
    spec.first?.() ?? spec.body.querySelector<HTMLElement>("input:not([disabled]), textarea, select") ?? b.ok;
  const done = run<boolean>(b, false, submit, first, spec.dirty ?? (() => false));
  return { done, refresh, setAction: (label) => setButtonLabel(b.ok, label) };
}
