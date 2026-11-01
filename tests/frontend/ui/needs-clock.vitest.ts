/**
 * 需手动浮层会话那一行的第二行：那台写好的会走的那一句（`sessions-needs` 会话那一项的 `clock.text`，`{dur}` 空位），
 * 已等 ＝ `waitedMs`（那台答出那一刻）＋ 本机从收到那一份起走过的；没有那一句 ⇒ 照抄 `needs.text`。在不在标签页里一样。
 *
 * 夹具只造结构（占位句子），不采真正文。
 */
import { describe, it, expect } from "vitest";
import { waitingNow } from "../../../src/frontend/ui/needs-panel";
import { fmtDur } from "../../../src/frontend/ui/duration-format";
import type { NeedsRow } from "../../../src/frontend/ui/generated/NeedsRow";

const RECEIVED = 1_000_000;

function row(waitedMs: number | null): NeedsRow {
  return { needs: { text: "核心·种类", waitedMs } } as unknown as NeedsRow;
}

describe("需手动浮层那一行会走的那一句", () => {
  it("照那台写好的那一句填，过 60 s 多 1m", () => {
    const b = { session: row(12_000), clock: { text: "核心·t {dur}", head: "核心·h {dur}" }, receivedAt: RECEIVED };
    expect(waitingNow(b, RECEIVED)).toBe(`核心·t ${fmtDur(12)}`);
    expect(waitingNow(b, RECEIVED + 60_000)).toBe(`核心·t ${fmtDur(72)}`);
  });

  it("没有那一句（不知道等了多久）⇒ 照抄那台写的种类那一句", () => {
    expect(waitingNow({ session: row(null), clock: null, receivedAt: RECEIVED }, RECEIVED + 60_000)).toBe("核心·种类");
  });
});
