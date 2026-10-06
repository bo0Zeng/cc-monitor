/**
 * 主窗口缩放：`Ctrl+=` 放大 · `Ctrl+-` 缩小 · `Ctrl+0` 还原；一档一档走，记住（下次打开照这一档）。
 * 缩的是整个网页（webview 自己的缩放），1px 线与字形在各档下照常。
 */
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { LS_KEYS, safeGet, safeSet } from "./local-storage";

export const ZOOM_STEPS: readonly number[] = [0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];

/** 记住的那一档（不在档上 / 读不出 ⇒ 1）。 */
export function savedZoom(): number {
  const v = Number(safeGet(LS_KEYS.mainZoom));
  return ZOOM_STEPS.includes(v) ? v : 1;
}

/** 从 `from` 走一档（`dir` 0 ⇒ 还原到 1）；到头停在头上。 */
export function nextZoom(from: number, dir: -1 | 0 | 1): number {
  if (dir === 0) return 1;
  const i = ZOOM_STEPS.indexOf(from);
  const at = i < 0 ? ZOOM_STEPS.indexOf(1) : i;
  return ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, at + dir))];
}

function apply(f: number): void {
  void getCurrentWebview()
    .setZoom(f)
    .catch((e: unknown) => console.warn("set zoom failed:", e));
}

/** 起来时照记住的那一档缩（1 ⇒ 不动）。 */
export function restoreZoom(): void {
  const f = savedZoom();
  if (f !== 1) apply(f);
}

/** 快捷键 / 命令面板：走一档并记住。 */
export function stepZoom(dir: -1 | 0 | 1): void {
  const f = nextZoom(savedZoom(), dir);
  safeSet(LS_KEYS.mainZoom, String(f));
  apply(f);
}
