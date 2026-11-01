/**
 * 统一写口（`ipc/write.ts`）的三种结局：成了 ⇒ 清草稿、重读；别处改过 ⇒ 说后端那句、重读（手上的草稿由页面留着）；别的失败 ⇒ 说那句、不重读。
 */
import { describe, it, expect, vi } from "vitest";
import { ChanError } from "../../../../src/comms/inward/chan";
import { write } from "../../../../src/frontend/ui/ipc/write";

const enc = (o: unknown): Uint8Array => new TextEncoder().encode(JSON.stringify(o));
const refused = (code: string): ChanError => new ChanError({ layer: "peer", why: "refused", body: enc({ code, said: `said ${code}` }) }, `said ${code}`);

describe("write：三种结局", () => {
  it("成了 ⇒ ok · 带回包 · 重读一次", async () => {
    const reread = vi.fn(async () => {});
    const clear = vi.fn();
    const end = await write(async () => 7, { reread, clear });
    expect(clear).toHaveBeenCalledTimes(1);
    expect(end).toEqual({ ok: true, value: 7 });
    expect(reread).toHaveBeenCalledTimes(1);
  });

  it("别处改过（码 stale）⇒ stale · 那一句是后端说的 · 重读一次", async () => {
    const code = "stale";
    const reread = vi.fn(async () => {});
    const end = await write(async () => {
      throw refused(code);
    }, { reread });
    expect(end).toEqual({ ok: false, stale: true, said: `said ${code}`, error: expect.any(ChanError) });
    expect(reread).toHaveBeenCalledTimes(1);
  });

  it("别的失败 ⇒ 不是 stale · 那一句照说 · 不重读", async () => {
    const reread = vi.fn(async () => {});
    const end = await write(async () => {
      throw refused("bad_input");
    }, { reread });
    expect(end).toMatchObject({ ok: false, stale: false, said: "said bad_input" });
    expect(reread).not.toHaveBeenCalled();
    const js = await write(async () => {
      throw new Error("断了");
    });
    expect(js).toMatchObject({ ok: false, stale: false, said: "断了" });
  });
});
