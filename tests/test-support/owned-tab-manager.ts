/**
 * 判据里建的 `TabManager` 交给这里：每条用例收场时照主人的拆法拆掉（`TabManager.dispose`）——
 * 冷读排班与空闲物化撤掉挂着的空闲回调，在途的读回来后不再排、不碰 `window`。
 * 不拆的话，环境拆掉之后才回来的那一趟会去碰已经不在的 `window`（负载高时整套跑偶发 `ReferenceError: window is not defined`）。
 */
import { afterEach } from "vitest";

const made = new Set<{ dispose(): void }>();

afterEach(() => {
  const all = [...made];
  made.clear(); // 先清：一份拆的时候抛了，只红这一条，不连累后面的用例
  for (const t of all) t.dispose();
});

/** 建出来就登记：这条用例收场时拆掉。 */
export function owned<T extends { dispose(): void }>(tm: T): T {
  made.add(tm);
  return tm;
}
