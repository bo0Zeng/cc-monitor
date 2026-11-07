/**
 * 待发（`outbox.ts`）：说出去、那台还没回执的那几句，整个主窗口一份（落本地存储）。重交挂在「那台接上」那一下，
 * 不挂在打开哪条会话 · 哪一页上（同手机 A2-18-8）；同一个编号交几次那台都只收一条。读写换成假的（照真的形状回）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { Outbox } from "../../../src/frontend/ui/outbox";
import type { QueuePut } from "../../../src/frontend/ui/generated/QueuePut";
import type { SessionQueuePutReq } from "../../../src/frontend/ui/generated/SessionQueuePutReq";
import { ChanError } from "../../../src/comms/inward/chan";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

const refused = (code: string, message: string): ChanError =>
  new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code, said: message })) }, message);
// 回执前断了：没回执（同断线）。
const offline = (): ChanError => new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");
const queued: QueuePut = { outcome: "queued", seq: 1, said: "第 1" };

beforeEach(() => localStorage.clear());

describe("待发：一台接上就把它名下各会话的待发照原编号重交", () => {
  it("★ 发送中时那台断了又接上 ⇒ 不打开那条会话也照原编号重交一次；回执到了就不在待发里", async () => {
    let up = false;
    const put = vi.fn(async (_o: string, _r: SessionQueuePutReq): Promise<QueuePut> => {
      if (!up) throw offline();
      return queued;
    });
    const box = new Outbox(put);
    const end = await box.say("devbox", "s-1", "hello", []);
    expect(end.at).toBe("none");
    expect(box.of("devbox", "s-1").map((o) => o.text)).toEqual(["hello"]);
    const id = put.mock.calls[0]![1].id;
    up = true;
    box.machineUp("devbox");
    await flush();
    expect(put).toHaveBeenCalledTimes(2);
    expect(put.mock.calls[1]![1].id).toBe(id);
    expect(box.of("devbox", "s-1")).toEqual([]);
  });

  it("两条会话各有一句待发 ⇒ 那台接上两条都重交；别的台接上不动它们", async () => {
    let up = false;
    const put = vi.fn(async (): Promise<QueuePut> => {
      if (!up) throw offline();
      return queued;
    });
    const box = new Outbox(put);
    await box.say("devbox", "s-1", "a", []);
    await box.say("devbox", "s-2", "b", []);
    up = true;
    box.machineUp("gpu-01");
    await flush();
    expect(put).toHaveBeenCalledTimes(2);
    box.machineUp("devbox");
    await flush();
    expect(put).toHaveBeenCalledTimes(4);
    expect(box.of("devbox", "s-1")).toEqual([]);
    expect(box.of("devbox", "s-2")).toEqual([]);
  });

  it("重开（新建一份、读本地存储）还在；接上照同一个编号交", async () => {
    const first = new Outbox(vi.fn(async (): Promise<QueuePut> => Promise.reject(offline())));
    await first.say("devbox", "s-1", "kept", []);
    const id = first.of("devbox", "s-1")[0]!.id;
    const put = vi.fn(async (_o: string, _r: SessionQueuePutReq): Promise<QueuePut> => queued);
    const again = new Outbox(put);
    expect(again.of("devbox", "s-1").map((o) => o.id)).toEqual([id]);
    again.machineUp("devbox");
    await flush();
    expect(put.mock.calls[0]![1]).toMatchObject({ id, text: "kept" });
  });

  it("那台拒了（不是离线）⇒ 不留在待发、回那台那一句", async () => {
    const box = new Outbox(vi.fn(async (): Promise<QueuePut> => Promise.reject(refused("queue_full", "队满了"))));
    const end = await box.say("devbox", "s-1", "x", []);
    expect(end).toEqual({ at: "refused", said: "队满了" });
    expect(box.of("devbox", "s-1")).toEqual([]);
  });

  it("在飞的那一句不重交（接上那一下撞上还没回来的那一问）", async () => {
    let release: (v: QueuePut) => void = () => {};
    const put = vi.fn(() => new Promise<QueuePut>((r) => (release = r)));
    const box = new Outbox(put);
    const p = box.say("devbox", "s-1", "x", []);
    box.machineUp("devbox");
    await flush();
    expect(put).toHaveBeenCalledTimes(1);
    release(queued);
    await p;
  });

  it("同一句没回执又点了一次 ⇒ 照它的编号再交，不另起一条", async () => {
    const put = vi.fn(async (_o: string, _r: SessionQueuePutReq): Promise<QueuePut> => Promise.reject(offline()));
    const box = new Outbox(put);
    await box.say("devbox", "s-1", "x", []);
    await box.say("devbox", "s-1", "x", []);
    expect(box.of("devbox", "s-1").length).toBe(1);
    expect(put.mock.calls[1]![1].id).toBe(put.mock.calls[0]![1].id);
  });

  it("★ 在飞那一刻 inFlight 是真、交完（没回执）就假，两下都叫订的那几处（那一行跟着换句）", async () => {
    let fail: (e: unknown) => void = () => {};
    const box = new Outbox(vi.fn(() => new Promise<QueuePut>((_ok, no) => (fail = no))));
    const seen = vi.fn();
    box.onChange(seen);
    const end = box.say("devbox", "s-1", "x", []);
    const id = box.of("devbox", "s-1")[0]!.id;
    expect(box.inFlight(id)).toBe(true);
    seen.mockClear();
    fail(offline());
    await end;
    expect(box.inFlight(id)).toBe(false);
    expect(seen).toHaveBeenCalled();
  });

  it("撤待发 ⇒ 本地拿掉，变了就叫订的那几处", async () => {
    const box = new Outbox(vi.fn(async (): Promise<QueuePut> => Promise.reject(offline())));
    const seen = vi.fn();
    box.onChange(seen);
    await box.say("devbox", "s-1", "x", []);
    const id = box.of("devbox", "s-1")[0]!.id;
    seen.mockClear();
    box.drop("devbox", "s-1", id);
    expect(box.of("devbox", "s-1")).toEqual([]);
    expect(seen).toHaveBeenCalled();
  });
});
