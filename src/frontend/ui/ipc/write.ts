/**
 * 写一笔的统一口：全界面**只这一处**判一次写的结局 —— 三种：
 *
 * - 成了 ⇒ `{ ok: true, value }`，并重读一次（页面拿新的那份画，草稿该清就清）；
 * - 别处改过（对端拒了、码是 `stale`）⇒ `{ ok: false, stale: true, said }`：那一句是后端写好的，
 *   照样重读一次（让人看到别处改成了什么），手上的草稿页面留着、照新版再交；
 * - 别的失败 ⇒ `{ ok: false, stale: false, said }`：说那一句，不重读，草稿留着。
 *
 * 那一句怎么上屏（toast · 状态行 · 卡上那一行）是页面的事；`error` 原样带回给要［复制详情］的。
 * 判据：`tests/frontend/ui/ipc/write.vitest.ts`。
 */
import { saidOf } from "../kit/detail";
import { refusalIn } from "./chan-caller";

/** 「看过之后被别处改过」的拒绝码。 */
const STALE = "stale";

export type WriteEnd<T> = { ok: true; value: T } | { ok: false; stale: boolean; said: string; error: unknown };

/** 发一笔写（`run`）。成了 ⇒ 先 `clear`（清掉这一笔交上去的草稿）再 `reread`；撞上别处改过 ⇒ 只 `reread`（草稿留着）。 */
export async function write<T>(run: () => Promise<T>, then: { reread?: () => Promise<unknown>; clear?: () => void } = {}): Promise<WriteEnd<T>> {
  let end: WriteEnd<T>;
  try {
    end = { ok: true, value: await run() };
  } catch (error) {
    end = { ok: false, stale: refusalIn(error)?.code === STALE, said: saidOf(error), error };
  }
  if (end.ok) then.clear?.();
  if (then.reread && (end.ok || end.stale)) await then.reread();
  return end;
}
