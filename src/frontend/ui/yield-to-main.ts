/**
 * 〔从 `events.ts` 搬来〕**让出主线程一跳**：`MessageChannel` 宏任务（规范没给它嵌套钳制），
 * 探不到才退回 `setTimeout(0)`。重放 drain（`events.ts`）与长回复分片渲染（`cards/index.ts`）共用这一份。
 *
 * ⚠ **必须带特性探测**（`§2.4` 逐字要求）。两个生产壳（WebView2 / WebKitGTK）都有它，
 * 但本模块也在 jsdom / node 里被跑，而且"两个壳都有"是**今天**的事实，不是一条不变量。
 * 探不到就退回 `setTimeout` —— 慢，但不会静默地一条都不排。
 */
export function makeYieldToMain(run: () => void): () => void {
  if (typeof MessageChannel === "function") {
    try {
      const ch = new MessageChannel();
      ch.port1.onmessage = (): void => run();
      return (): void => ch.port2.postMessage(null);
    } catch {
      // 建不出来（某些受限环境）⇒ 落到下面的降级，不抛。
    }
  }
  return (): void => {
    setTimeout(run, 0);
  };
}
