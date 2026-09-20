/**
 * `设计/70 §1.3 E`（第一刀 · 步 4 的后半）：**点击侧给 pending**。
 *
 * # 它治的是什么
 *
 * `真相源/06` 那份普查里，设置面板上一排按钮点下去**没有任何回执**：命令是异步的，
 * 而按钮在整个往返期间照常可点。两个后果，都真发生过：
 * - **双起窗口**（`backend-section.ts` 那条补审 `A1` 逐字记的就是这个）：
 *   连点两下 = 两次往返，第二次的结果会盖掉第一次，而屏幕上看不出来；
 * - 用户以为没点上，于是再点一次 —— 对「重新扫描」这类只读操作只是浪费，
 *   对写操作就是两次写。
 *
 * # 为什么是这个形状
 *
 * `§1.3 E` 逐字：**`await` ＋ disable 按钮 ＋ spinner**。这里的「spinner」落成
 * **换一行字**而不是转圈动画 —— 转圈要一条 CSS 动画规则，而新类名会撞
 * `css-ledger` 的两条棘轮（`设计/41 §7` 给的正解是 `data-*`，不是新类）。
 * ⇒ 身份走 `data-pending`，可见回执走按钮自己的文字。
 *
 * ⚠ **它不做超时**。远端断流之后对面什么时候回，我们在本机看不见
 *（同 `backend-section.ts::act` 那条诚实边界）——「卡住不放」与「还在跑」
 * 在这一层分不开，替用户猜一个期限只会在慢机器上骗他。
 */

/**
 * 跑一次异步动作，期间把这个按钮**按住**。
 *
 * - 已经在跑 ⇒ 直接返回。⚠ **诚实标注**：真正挡住连点的是下面那句
 *   `btn.disabled = true`（浏览器与 jsdom 都不给 disabled 控件派发 click）——
 *   死值验实打过：把这一行去掉，那条「按住期间往返恒等于 1」的判据**照样绿**。
 *   它挡的是**程序性的**第二次调用（有人直接 `withPending(...)` 两遍），
 *   不是用户的第二下点击。写清楚，免得下一个人以为它是那条判据的承重墙。
 * - 不管成失败，`finally` 一定把按钮与文字还原 —— 失败就永远按住的话，
 *   用户连重试都做不到，而那比没有 pending 更糟。
 */
export async function withPending(
  btn: HTMLButtonElement,
  busyLabel: string,
  run: () => Promise<void>,
): Promise<void> {
  if (btn.disabled) return;
  const label = btn.textContent;
  btn.disabled = true;
  btn.dataset.pending = "1";
  btn.textContent = busyLabel;
  try {
    await run();
  } finally {
    btn.textContent = label;
    delete btn.dataset.pending;
    btn.disabled = false;
  }
}
