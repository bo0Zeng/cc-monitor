/**
 * 进度（C14）：不确定进度的 14px 转圈。
 *
 * 转圈不到 300ms 不画、画了至少留 400ms（防闪）由 [`delayedSpinner`] 管。
 */
import s from "./progress.module.css";

/** 14px 转圈（纯装饰，读屏器不念；进行中的字由所在控件给）。 */
export function spinner(): HTMLSpanElement {
  const sp = document.createElement("span");
  sp.className = s.progressSpinner;
  sp.setAttribute("aria-hidden", "true");
  return sp;
}
