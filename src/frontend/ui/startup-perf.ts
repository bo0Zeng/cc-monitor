/**
 * 启动管线读数（`window.__ccmPerf`）的出声处：首批行到前端那一句 · 整条 timeline 落盘（经后端写进 monitor 日志，`fe_perf`）。
 *
 * 首批行有两条来路，两条都在这里记、谁先到算谁：
 * - **重放批**：会话流里一段成批的行（`batch` 边界，`events.ts`）；
 * - **冷起补读**：一个会话刚进那台那条流的「在看」名单，前端按行号把上流点之下那一截取回来（`TabStreamView.catchUpTo`）。
 *   壳冷起不再整份重放（流里没有那一段批），冷起首个标签页的正文走的就是这一条 ⇒ 「首批到了」那一句与 timeline 也在这里打。
 */
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { commands } from "./ipc/commands";

/** 首批行到前端：记时刻、打一句（页面生命周期里只第一次）。 */
export function noteFirstBatch(): void {
  const perf = window.__ccmPerf;
  if (!perf || perf.firstJsonlBatch !== undefined) return;
  perf.firstJsonlBatch = performance.now();
  console.info(`[perf] first stream batch received @ ${perf.firstJsonlBatch.toFixed(0)}ms`);
}

/** 冷起补读喂进 tab 的行（累计；timeline 那一行的 `drained`）。 */
let coldFed = 0;
/** 冷起补读那一份 timeline 已经落过盘。 */
let coldSaid = false;

/** 按行号取回的一段行要喂进 tab 了（`TabStreamView.feedHistoryRows`）：首批那一句 ＋ 第一次派发的时刻。 */
export function noteHistoryArrived(): void {
  const perf = window.__ccmPerf;
  if (!perf) return;
  noteFirstBatch();
  perf.firstPayloadDrained ??= performance.now();
}

/** 那一段 `n` 行喂完了：累计、记第一次喂完的时刻。 */
export function noteHistoryFed(n: number): void {
  const perf = window.__ccmPerf;
  if (!perf) return;
  coldFed += n;
  perf.batchDrainEnd ??= performance.now();
}

/** 冷起那一趟补读落地（取完、当前 tab 物化过）：timeline 落一次盘（页面生命周期里只一次；重放批那一份照旧在它自己的切 live 处落）。 */
export function noteColdFilled(): void {
  const perf = window.__ccmPerf;
  if (!perf || coldSaid) return;
  coldSaid = true;
  perf.onBatchEndFired ??= performance.now();
  emitPerfSummary(perf, coldFed);
}

/**
 * 「这个东西可以 `.catch`」—— 下面那一跳唯一依赖的那条前提，抽出来给它一个名字。
 *
 * ⚠ 判的是 **`.catch`**，不是 `.then`。两者在「thenable」这个词下常被当成一件事，
 * 而调用点真正要的是前者 —— 一个只有 `.then` 的 thenable 照样会在 `.catch(` 那一行炸。
 * **判据要贴着调用点写，不贴着行话写。**
 */
function isCatchable(v: unknown): v is Promise<unknown> {
  return typeof (v as { catch?: unknown } | null | undefined)?.catch === "function";
}

/** 启动管线 perf timeline 输出（切 live 那一刻调一次：重放批的 onBatchEnd 真正 fire 时 · 冷起补读落地时）。 */
export function emitPerfSummary(
  p: typeof window.__ccmPerf,
  payloadCount: number,
): void {
  const dom = p.domContentLoaded ?? 0;
  const theme = p.themeLoaded ?? 0;
  const ready = p.frontendReadyEmit ?? 0;
  const firstBatch = p.firstJsonlBatch ?? 0;
  const firstDrain = p.firstPayloadDrained ?? 0;
  const drainEnd = p.batchDrainEnd ?? 0;
  const endFired = p.onBatchEndFired ?? performance.now();

  // 多窗口（查看器 / 拆出的窗口）各自落盘，带 label 免得读数混在一起
  const winLabel = getCurrentWebviewWindow().label;
  const lines = [
    `[perf] === 启动管线 timeline (前端 performance.now ms · window=${winLabel}) ===`,
    `[perf]   DOMContentLoaded       T+${dom.toFixed(0)}`,
    `[perf]   loadTheme done         T+${theme.toFixed(0)} (+${(theme - dom).toFixed(0)})`,
    `[perf]   emit frontend-ready    T+${ready.toFixed(0)} (+${(ready - theme).toFixed(0)})`,
    `[perf]   first jsonl-batch in   T+${firstBatch.toFixed(0)} (+${(firstBatch - ready).toFixed(0)}) ← 首批行到前端（重放批 · 冷起补读）`,
    `[perf]   first payload dispatch T+${firstDrain.toFixed(0)} (+${(firstDrain - firstBatch).toFixed(0)})`,
    `[perf]   batch payloads drained T+${drainEnd.toFixed(0)} (+${(drainEnd - firstDrain).toFixed(0)}) ← drain 全部 ${payloadCount} 条`,
    `[perf]   onBatchEnd fired       T+${endFired.toFixed(0)} (+${(endFired - drainEnd).toFixed(0)}) ← 切 live（重放批：300ms grace 后；冷起补读：取完、当前 tab 物化之后）`,
    // 建卡 vs 收纳对照；计数为页面生命周期累计（二次批不归零；批后 rIC 物化不在本行内）
    `[perf]   建卡 rendered=${p.recordsRendered ?? 0} · 收纳 deferred=${p.recordsDeferred ?? 0} · drained=${payloadCount}(累计)`,
    `[perf]   ── 总耗时（DOM → 加载完成）：${(endFired - dom).toFixed(0)}ms ──`,
  ];
  console.info(lines.join("\n"));
  // 没有 devtools 的环境里的读数通道：经后端写进 monitor 日志（grep fe_perf）。
  // 返回的不是 Promise（桩返回 undefined）⇒ 出声不炸：在 grace 那个 `setTimeout` 回调里抛，没有调用栈接得住。
  const sent: unknown = commands.frontend_perf_log({ lines: lines.join("\n") });
  if (isCatchable(sent)) {
    void sent.catch(() => {});
  } else {
    console.warn(
      `[perf] frontend_perf_log 的返回值没有 .catch（拿到 ${typeof sent}）——` +
        "本次 perf timeline 没能经后端落盘。\n" +
        "★ 在测试里看到这一行 = 那个桩返回的不是 Promise；" +
        "它此前的表现是一条点不出判据名的 unhandled error。",
    );
  }
}
