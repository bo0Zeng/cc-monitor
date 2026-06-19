/**
 * 远端健康提示的节流纯逻辑（SS-F，issue #32）。
 *
 * 单独成文件、**零 import**：这样 `remote-health.test.ts` 能用 `node` 直接跑而不必
 * 拉进 `remote-health.ts` 的 tauri/DOM 依赖（node ESM 不解析无扩展名的相对 import）。
 * 与 api-error.ts / diff.ts 同样的「纯逻辑可单测」约定。
 */

/** 同一 (origin,kind) 两次提示的默认最小间隔（ms），防拥塞期 toast 刷屏。 */
export const HEALTH_TOAST_THROTTLE_MS = 10_000;

/**
 * 纯函数：给定该 key 上次展示时刻 `lastShownMs`（`undefined`=从未弹过）、当前时刻
 * `nowMs`、最小间隔 `minIntervalMs`，决定现在是否该再弹。
 * 规则：从未弹过、或距上次 ≥ 间隔，才弹。
 */
export function shouldShowHealthToast(
  lastShownMs: number | undefined,
  nowMs: number,
  minIntervalMs: number = HEALTH_TOAST_THROTTLE_MS,
): boolean {
  return lastShownMs === undefined || nowMs - lastShownMs >= minIntervalMs;
}
