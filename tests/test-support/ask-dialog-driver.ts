/**
 * 在 jsdom 里「当用户」答 `src/frontend/ui/ask-dialog.ts` 弹出来的应用内对话框。
 *
 * 判据点的是**真按钮**、等的是**真 Promise** —— 与真 app 同形（答案是异步到的）。
 * 从前各处 `vi.spyOn(window, "confirm").mockReturnValue(false)` 测的是一个同步的原生 `confirm`，
 * 而真 app 里那个函数是插件注入的 async 替身、返回 Promise（恒真值）⇒ 那些判据绿着、真 app 里根本不问。
 */
import { expect } from "vitest";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

function current(): HTMLElement {
  const d = document.querySelector<HTMLElement>('[role="dialog"]');
  expect(d, "没有弹出应用内对话框").not.toBeNull();
  return d!;
}

/** 当前对话框的正文。 */
export function askDialogText(): string {
  return current().firstElementChild?.textContent ?? "";
}

/** 点「确定」（`true`）或「取消」（`false`），再让调用方那一串 `await` 走完。 */
export async function answerAskDialog(ok: boolean): Promise<void> {
  const btns = current().querySelectorAll("button");
  (ok ? btns[1] : btns[0]).click();
  await flush();
}

/** 文本框填 `value` 后点「确定」；`null` = 点「取消」。 */
export async function answerAskText(value: string | null): Promise<void> {
  const d = current();
  if (value !== null) d.querySelector("input")!.value = value;
  await answerAskDialog(value !== null);
}

/** 页面上没有对话框（没问、或已答完）。 */
export function noAskDialog(): boolean {
  return document.querySelector('[role="dialog"]') === null;
}
