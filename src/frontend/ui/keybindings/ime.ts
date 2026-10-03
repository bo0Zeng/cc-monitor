/** 输入法正在组字：这一下 Enter / 方向键 / Esc 归输入法（`keyCode 229` 是部分 webview 组字时的形）。 */
export function imeComposing(e: KeyboardEvent): boolean {
  return e.isComposing || e.keyCode === 229;
}
