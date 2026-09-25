/**
 * 〔W5-UI〕应用内对话框：全仓「要不要做 / 叫什么名字」只问这一处（`设计/70 §10 #1`「全仓 `window.prompt` /
 * `window.confirm` 收口」· `真相源/05 I5`「收口到一套自绘」）。
 *
 * # 为什么不能再用 `window.confirm`
 *
 * `tauri-plugin-dialog` 的 `init()` 往每个 webview 注入一段脚本，把 `window.confirm` 换成
 * `async function (m) { return await invoke("plugin:dialog|confirm", …) }` ——
 * 返回的是 **Promise，永远真值**；它调的那条命令还被拒（`REJ: Command plugin:dialog|confirm not allowed by ACL`）。
 * ⇒ 真 app 里 `if (!window.confirm(msg)) return;` 从来不拦：杀会话、删会话、换号重启……一个都没问就做了。
 * jsdom 里 `window.confirm` 是同步原生实现，所以单测一直是绿的。
 * 判据 `tests/ask-dialog.vitest.ts` 钉生产 TS 零处原生 `confirm` / `prompt`（D1），并钉本模块的结算语义（D2）。
 *
 * # 形状
 *
 * - [`askConfirm`] ⇒ `true`（确定）/ `false`（取消 · 点遮罩 · Esc）。
 * - [`askText`] ⇒ 文本框的值（原样，不 `trim`）/ `null`（取消）。`null` 与「答了空串」是两件事。
 * - 遮罩挂 `document.body`（`INVARIANTS §13`）；正文走 `textContent`（不解释 HTML），`\n` 原样换行。
 * - **Esc 走 overlay 栈**（`dispatcher.pushOverlay`）：对话框是栈顶，Esc 只关它，不连带关下面那层
 *   （INVARIANTS「别手搓 window 级 Esc 监听 —— 与栈内 overlay 的 Esc 双触发」）。
 * - **同一时刻只有一个**：新开之前把旧的**按取消结算**（`fork-ask.ts` 那条教训：只摘 DOM 会留一个永挂的 Promise）。
 *   取消是安全方向：破坏性动作不做。
 *
 * # 买不到
 *
 * 模态期间 Esc 以外的全局快捷键照旧会被 `dispatcher` 接住（它在 window 捕获阶段、先于这里的任何监听；
 * 焦点在文本框时单键由 `isEditableTarget` 挡住，按钮上不挡）。原生 `confirm` 是阻塞的，没有这个问题。
 */
import { copyText } from "./copy-table";
import { dispatcher, type OverlayHandle } from "./keybindings/registry";
import s from "./ask-dialog.module.css";

export interface AskConfirmOptions {
  /** 「确定」那颗按钮的字（默认取表）。 */
  okLabel?: string;
}

export interface AskTextOptions {
  /** 文本框初值（打开即全选）。 */
  initial?: string;
  /** 「确定」那颗按钮的字（默认取表）。 */
  okLabel?: string;
}

/**
 * 「问一句要不要做」的注入缝形状（`account-restart.ts` · `tab-session-actions.ts::killRemoteTmux` 用）。
 * 允许同步答（测试 / e2e 注入 `() => true`），调用方一律 `await`。
 */
export type ConfirmFn = (message: string) => boolean | Promise<boolean>;

/** 还没结算的那一个：新开之前按取消结算它。 */
let pendingCancel: (() => void) | null = null;

function open<T>(
  message: string,
  field: { initial: string } | null,
  okLabel: string | undefined,
  onOk: (input: HTMLInputElement | null) => T,
  cancelled: T,
): Promise<T> {
  pendingCancel?.();

  const backdrop = document.createElement("div");
  backdrop.className = s.backdrop;
  const panel = document.createElement("div");
  panel.className = s.panel;
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-modal", "true");
  backdrop.appendChild(panel);

  const text = document.createElement("div");
  text.className = s.message;
  text.textContent = message;
  panel.appendChild(text);

  let input: HTMLInputElement | null = null;
  if (field) {
    input = document.createElement("input");
    input.type = "text";
    input.className = s.field;
    input.value = field.initial;
    panel.appendChild(input);
  }

  const buttons = document.createElement("div");
  buttons.className = s.buttons;
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = copyText("askDialog.buttons.cancel");
  const ok = document.createElement("button");
  ok.type = "button";
  ok.textContent = okLabel ?? copyText("askDialog.buttons.ok");
  buttons.append(cancel, ok);
  panel.appendChild(buttons);

  const before = document.activeElement;

  return new Promise<T>((resolve) => {
    let settled = false;
    const overlay: OverlayHandle = {
      handleEsc: () => {
        finish(cancelled);
        return true;
      },
    };
    const finish = (v: T): void => {
      if (settled) return; // 点按钮与 Esc 可能同一轮到达；只认第一次
      settled = true;
      if (pendingCancel === cancelSelf) pendingCancel = null;
      dispatcher.popOverlay(overlay);
      backdrop.remove();
      if (before instanceof HTMLElement && before.isConnected) before.focus();
      resolve(v);
    };
    const cancelSelf = (): void => finish(cancelled);
    pendingCancel = cancelSelf;

    backdrop.addEventListener("click", (ev) => {
      if (ev.target === backdrop) finish(cancelled); // 点遮罩 = 取消
    });
    cancel.addEventListener("click", () => finish(cancelled));
    ok.addEventListener("click", () => finish(onOk(input)));
    input?.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) {
        ev.preventDefault();
        finish(onOk(input));
      }
    });

    dispatcher.pushOverlay(overlay);
    document.body.appendChild(backdrop);
    if (input) {
      input.focus();
      input.select();
    } else {
      ok.focus();
    }
  });
}

/** 问一句「要不要做」。确定 ⇒ `true`；取消 · 点遮罩 · Esc · 被下一个对话框顶掉 ⇒ `false`。 */
export function askConfirm(message: string, opts: AskConfirmOptions = {}): Promise<boolean> {
  return open(message, null, opts.okLabel, () => true, false);
}

/** 问一个名字 / 一行字。确定（或文本框里 Enter）⇒ 文本框的值（原样）；取消一类 ⇒ `null`。 */
export function askText(message: string, opts: AskTextOptions = {}): Promise<string | null> {
  return open(message, { initial: opts.initial ?? "" }, opts.okLabel, (inp) => inp?.value ?? "", null);
}
