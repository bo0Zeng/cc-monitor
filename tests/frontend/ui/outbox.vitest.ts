/**
 * 待发（`outbox.ts`）：说出去、那台还没回执的那几句，整个主窗口一份（落本地存储）。重交挂在「那台接上」那一下，
 * 不挂在打开哪条会话 · 哪一页上（同手机 A2-18-8）；同一个编号交几次那台都只收一条。读写换成假的（照真的形状回）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { Outbox } from "../../../src/frontend/ui/outbox";
import type { QueuePut } from "../../../src/frontend/ui/generated/QueuePut";
import type { SessionQueuePutReq } from "../../../src/frontend/ui/generated/SessionQueuePutReq";
import type { QueueCancelled } from "../../../src/frontend/ui/generated/QueueCancelled";
import type { QueueList } from "../../../src/frontend/ui/generated/QueueList";
import { ChanError } from "../../../src/comms/inward/chan";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

const refused = (code: string, message: string): ChanError =>
  new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code, said: message })) }, message);
// 回执前断了：没回执（同断线）。
const offline = (): ChanError => new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");
const queued: QueuePut = { outcome: "queued", seq: 1, said: "第 1" };
const emptyList = (sent: string[] = []): QueueList => ({ items: [], sent });
const never = (): Promise<never> => new Promise<never>(() => {});

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

  it("★ 那台拒了（不是离线）⇒ 留在待发、标上那台那一句（同手机）；重开还在", async () => {
    const box = new Outbox(vi.fn(async (): Promise<QueuePut> => Promise.reject(refused("queue_full", "队满了"))));
    const end = await box.say("devbox", "s-1", "x", []);
    expect(end).toEqual({ at: "refused", said: "队满了" });
    expect(box.of("devbox", "s-1").map((o) => [o.text, o.refused])).toEqual([["x", "队满了"]]);
    const again = new Outbox(vi.fn(async (): Promise<QueuePut> => queued));
    expect(again.of("devbox", "s-1").map((o) => o.refused)).toEqual(["队满了"]);
  });

  it("拒了的那一句：那台接上不重交；同一句再发 ⇒ 照原编号再交、回执到了就离开待发", async () => {
    let ok = false;
    const put = vi.fn(async (_o: string, _r: SessionQueuePutReq): Promise<QueuePut> => {
      if (!ok) throw refused("queue_full", "队满了");
      return queued;
    });
    const box = new Outbox(put);
    await box.say("devbox", "s-1", "x", []);
    box.machineUp("devbox");
    await flush();
    expect(put).toHaveBeenCalledTimes(1);
    ok = true;
    const end = await box.say("devbox", "s-1", "x", []);
    expect(end.at).toBe("receipt");
    expect(put.mock.calls[1]![1].id).toBe(put.mock.calls[0]![1].id);
    expect(box.of("devbox", "s-1")).toEqual([]);
  });

  it("编辑已发的那一句（带 rewind）同普通一句：当场被拒 · 接上重交被拒都留在待发标那台那一句", async () => {
    const rewind = { to: "u-1", how: "conversation", when: "after_turn" } as const;
    const now = new Outbox(vi.fn(async (): Promise<QueuePut> => Promise.reject(refused("queue_full", "队满了"))));
    expect(await now.say("devbox", "s-1", "x", [], rewind)).toEqual({ at: "refused", said: "队满了" });
    expect(now.of("devbox", "s-1").map((o) => o.refused)).toEqual(["队满了"]);
    localStorage.clear();
    let up = false;
    const put = vi.fn(async (): Promise<QueuePut> => {
      throw up ? refused("queue_full", "队满了") : offline();
    });
    const box = new Outbox(put);
    await box.say("devbox", "s-1", "x", [], rewind);
    up = true;
    box.machineUp("devbox");
    await flush();
    expect(box.of("devbox", "s-1").map((o) => o.refused)).toEqual(["队满了"]);
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

  it("撤待发首问没答复 ⇒ 本地拿掉，变了就叫订的那几处", async () => {
    const box = new Outbox(
      vi.fn(async (): Promise<QueuePut> => Promise.reject(offline())),
      vi.fn(async (): Promise<QueueCancelled> => Promise.reject(offline())),
    );
    const seen = vi.fn();
    box.onChange(seen);
    await box.say("devbox", "s-1", "x", []);
    const id = box.of("devbox", "s-1")[0]!.id;
    seen.mockClear();
    expect((await box.withdraw("devbox", "s-1", id))?.at).toBe("lost");
    expect(box.of("devbox", "s-1")).toEqual([]);
    expect(seen).toHaveBeenCalled();
  });
});

describe("待发认名单里的 sent", () => {
  it("★ put 一直不回、名单说它已送 ⇒ 待发与本地存储都没了、叫一次结局；之后 put 回没回执 ⇒ 不挂回、不再叫，那台接上不重交", async () => {
    let fail: (e: unknown) => void = () => {};
    const put = vi.fn(() => new Promise<QueuePut>((_ok, no) => (fail = no)));
    const box = new Outbox(put);
    const ends: string[] = [];
    box.onSettled((_o, _x, end) => ends.push(end.at));
    const said = box.say("devbox", "s-1", "x", []);
    const id = box.of("devbox", "s-1")[0]!.id;
    box.wentIn("devbox", "s-1", [id]);
    expect(box.of("devbox", "s-1")).toEqual([]);
    expect(new Outbox(vi.fn()).of("devbox", "s-1"), "本地存储也删了").toEqual([]);
    expect(ends).toEqual(["sent"]);
    fail(offline());
    await said;
    expect(box.of("devbox", "s-1"), "没回执那一问回来不挂回").toEqual([]);
    expect(ends, "不再叫结局").toEqual(["sent"]);
    box.machineUp("devbox");
    await flush();
    expect(put).toHaveBeenCalledTimes(1);
  });

  it("名单先说它已送、回执后到 ⇒ 结局只叫一次（sent）", async () => {
    let ok: (v: QueuePut) => void = () => {};
    const box = new Outbox(vi.fn(() => new Promise<QueuePut>((r) => (ok = r))));
    const ends: string[] = [];
    box.onSettled((_o, _x, end) => ends.push(end.at));
    const said = box.say("devbox", "s-1", "x", []);
    box.wentIn("devbox", "s-1", [box.of("devbox", "s-1")[0]!.id]);
    ok(queued);
    await said;
    expect(ends).toEqual(["sent"]);
  });

  it("★ 名单先说它已送、那一问回来没回执 ⇒ say 回的是送进去了（回执），不是没回执", async () => {
    let fail: (e: unknown) => void = () => {};
    const box = new Outbox(vi.fn(() => new Promise<QueuePut>((_ok, no) => (fail = no))));
    const said = box.say("devbox", "s-1", "x", [], { to: "u-1", how: "conversation", when: "after_turn" });
    box.wentIn("devbox", "s-1", [box.of("devbox", "s-1")[0]!.id]);
    fail(offline());
    expect(await said, "回执没到：那台没写那一句，这一端不替它写").toEqual({ at: "receipt", said: "" });
  });

  it("★ 撤着的那一句那台接上不重交；撤着时再撤不另问", async () => {
    const put = vi.fn(async (): Promise<QueuePut> => Promise.reject(offline()));
    const cancel = vi.fn(never);
    const box = new Outbox(put, cancel);
    await box.say("devbox", "s-1", "x", []);
    const id = box.of("devbox", "s-1")[0]!.id;
    void box.withdraw("devbox", "s-1", id);
    expect(await box.withdraw("devbox", "s-1", id), "撤着时再撤 ⇒ 不另起").toBeNull();
    expect(cancel).toHaveBeenCalledTimes(1);
    box.machineUp("devbox");
    await flush();
    expect(put).toHaveBeenCalledTimes(1);
    expect(cancel, "正在问的不另问").toHaveBeenCalledTimes(1);
  });

  it("★ 撤成了以后回执才到 ⇒ 不再叫结局；首问没答复、本地拿掉的回执后到 ⇒ 照常叫回执", async () => {
    let ok: (v: QueuePut) => void = () => {};
    let answer: (v: QueueCancelled) => void = () => {};
    let up = true;
    const box = new Outbox(
      vi.fn(() => new Promise<QueuePut>((r) => (ok = r))),
      vi.fn(() => (up ? new Promise<QueueCancelled>((r) => (answer = r)) : Promise.reject(offline()))),
    );
    const ends: string[] = [];
    box.onSettled((_o, _x, end) => ends.push(end.at));
    const a = box.say("devbox", "s-1", "a", []);
    void box.withdraw("devbox", "s-1", box.of("devbox", "s-1")[0]!.id);
    answer({ cancelled: true, said: "撤了", queue: emptyList() });
    await flush();
    ok(queued);
    await a;
    expect(ends).toEqual([]);
    up = false;
    const b = box.say("devbox", "s-1", "b", []);
    await box.withdraw("devbox", "s-1", box.of("devbox", "s-1")[0]!.id);
    expect(box.of("devbox", "s-1")).toEqual([]);
    ok(queued);
    await b;
    expect(ends).toEqual(["receipt"]);
  });

  it("★ 编辑已发的那一句在飞时撤成了、回执后到 ⇒ say 回撤了（编辑框留着字）", async () => {
    let ok: (v: QueuePut) => void = () => {};
    let answer: (v: QueueCancelled) => void = () => {};
    const box = new Outbox(
      vi.fn(() => new Promise<QueuePut>((r) => (ok = r))),
      vi.fn(() => new Promise<QueueCancelled>((r) => (answer = r))),
    );
    const said = box.say("devbox", "s-1", "x", [], { to: "u-1", how: "conversation", when: "after_turn" });
    void box.withdraw("devbox", "s-1", box.of("devbox", "s-1")[0]!.id);
    answer({ cancelled: true, said: "撤了", queue: emptyList() });
    await flush();
    ok(queued);
    expect((await said).at).toBe("cancelled");
  });

  it("交来的编号不是待发里的 ⇒ 什么都不动", async () => {
    const box = new Outbox(vi.fn(async (): Promise<QueuePut> => Promise.reject(offline())));
    await box.say("devbox", "s-1", "x", []);
    const seen = vi.fn();
    box.onChange(seen);
    box.wentIn("devbox", "s-1", ["monitor-别的"]);
    box.wentIn("devbox", "s-2", [box.of("devbox", "s-1")[0]!.id]);
    expect(box.of("devbox", "s-1").length).toBe(1);
    expect(seen).not.toHaveBeenCalled();
  });
});

describe("撤待发：撤着落盘，接上照同一个编号接着问（同手机 ChatOutbox）", () => {
  /** 一句交了没回执、点了撤、那一问还没答（窗口这时重开）：回那一句的编号。 */
  async function parkedInStorage(rewind?: { to: string; how: "conversation"; when: "after_turn" }): Promise<string> {
    const first = new Outbox(vi.fn(async (): Promise<QueuePut> => Promise.reject(offline())), vi.fn(never));
    await first.say("devbox", "s-1", "x", [], rewind);
    const id = first.of("devbox", "s-1")[0]!.id;
    void first.withdraw("devbox", "s-1", id);
    await flush();
    return id;
  }

  it("★ 撤那一问在路上 ⇒ 撤着已落盘：重开读回仍撤着、那台接上不重交，照同一个编号接着请它撤；那台已记撤过 ⇒ 撤成了", async () => {
    const id = await parkedInStorage();
    const put = vi.fn(async (): Promise<QueuePut> => queued);
    const cancel = vi.fn(async (_o: string, _s: string, _id: string): Promise<QueueCancelled> => ({ cancelled: true, said: "撤了", queue: emptyList() }));
    const again = new Outbox(put, cancel);
    expect(again.of("devbox", "s-1").map((o) => [o.id, o.withdrawing])).toEqual([[id, true]]);
    expect(again.parked(id)).toBe(true);
    const ends: string[] = [];
    again.onWithdrawn((_o, x, w) => ends.push(`${x.id}:${w.at}`));
    again.machineUp("devbox");
    await flush();
    expect(put, "撤着的不重交").not.toHaveBeenCalled();
    expect(cancel).toHaveBeenCalledWith("devbox", "s-1", id);
    expect(ends).toEqual([`${id}:back`]);
    expect(again.of("devbox", "s-1")).toEqual([]);
    expect(new Outbox(vi.fn()).of("devbox", "s-1"), "本地存储也删了").toEqual([]);
  });

  it("★ 接着问的那一句没答复 ⇒ 不拿掉、仍撤着，下一回接上再问；答没撤成 ⇒ 当送进去了", async () => {
    const id = await parkedInStorage();
    let down = true;
    const cancel = vi.fn(async (): Promise<QueueCancelled> => {
      if (down) throw offline();
      return { cancelled: false, said: "送过了", queue: emptyList() };
    });
    const again = new Outbox(vi.fn(async (): Promise<QueuePut> => queued), cancel);
    const ends: string[] = [];
    again.onWithdrawn((_o, _x, w) => ends.push(w.at));
    const settled: string[] = [];
    again.onSettled((_o, _x, e) => settled.push(e.at));
    again.machineUp("devbox");
    await flush();
    expect(ends).toEqual(["lost"]);
    expect(again.of("devbox", "s-1").map((o) => o.id), "接着问没答复：仍在").toEqual([id]);
    expect(again.parked(id)).toBe(true);
    down = false;
    again.machineUp("devbox");
    await flush();
    expect(cancel).toHaveBeenCalledTimes(2);
    expect(ends).toEqual(["lost", "in"]);
    expect(settled).toEqual(["sent"]);
    expect(again.of("devbox", "s-1")).toEqual([]);
  });

  it("★ 正在问的不算撤着待问；撤着的那一句同字再发 ⇒ 另起一条，不把撤着的那一句重交", async () => {
    const put = vi.fn(async (_o: string, _r: SessionQueuePutReq): Promise<QueuePut> => Promise.reject(offline()));
    const box = new Outbox(put, vi.fn(never));
    await box.say("devbox", "s-1", "x", []);
    const id = box.of("devbox", "s-1")[0]!.id;
    void box.withdraw("devbox", "s-1", id);
    expect(box.parked(id)).toBe(false);
    await box.say("devbox", "s-1", "x", []);
    expect(box.of("devbox", "s-1").length).toBe(2);
    expect(put.mock.calls.filter((c) => c[1].id === id).length, "撤着的那一句只交过头一回").toBe(1);
  });

  it("拒过的那一句撤 ⇒ 不问那台、当场撤成了", async () => {
    const cancel = vi.fn(never);
    const box = new Outbox(vi.fn(async (): Promise<QueuePut> => Promise.reject(refused("queue_full", "队满了"))), cancel);
    await box.say("devbox", "s-1", "x", []);
    expect(await box.withdraw("devbox", "s-1", box.of("devbox", "s-1")[0]!.id)).toEqual({ at: "back", queue: null });
    expect(cancel).not.toHaveBeenCalled();
    expect(box.of("devbox", "s-1")).toEqual([]);
  });

  it("编辑已发的那一句撤着 ⇒ editHeld 认得出它编辑的那一句；撤完不再认", async () => {
    await parkedInStorage({ to: "u-1", how: "conversation", when: "after_turn" });
    const again = new Outbox(
      vi.fn(async (): Promise<QueuePut> => queued),
      vi.fn(async (): Promise<QueueCancelled> => ({ cancelled: true, said: "撤了", queue: emptyList() })),
    );
    expect(again.editHeld("devbox", "s-1", "u-1")).toBe(true);
    expect(again.editHeld("devbox", "s-1", "u-9")).toBe(false);
    again.machineUp("devbox");
    await flush();
    expect(again.editHeld("devbox", "s-1", "u-1")).toBe(false);
  });

  it("撤着时在飞的那一问回来说那台拒了 ⇒ 没进它的队，编辑那一句不再算上一版撤着", async () => {
    let no: (e: unknown) => void = () => {};
    const box = new Outbox(vi.fn(() => new Promise<QueuePut>((_ok, n) => (no = n))), vi.fn(never));
    const said = box.say("devbox", "s-1", "x", [], { to: "u-1", how: "conversation", when: "after_turn" });
    void box.withdraw("devbox", "s-1", box.of("devbox", "s-1")[0]!.id);
    expect(box.editHeld("devbox", "s-1", "u-1")).toBe(true);
    no(refused("queue_full", "队满了"));
    await said;
    expect(box.editHeld("devbox", "s-1", "u-1")).toBe(false);
  });
});

describe("待发读回照形状核：缺了算合法，有字就得是对的形状；坏的那一句丢掉、记一行说丢了哪一格", () => {
  const good = { id: "monitor-ok", sid: "s-1", text: "好的", attachments: [] };
  const seed = (bad: Record<string, unknown>): void =>
    localStorage.setItem("cc-monitor.queue.pending", JSON.stringify({ "devbox\u0000s-1": [{ ...good, id: "monitor-bad", ...bad }, good] }));

  it.each([
    ["refused", { refused: 3 }],
    ["withdrawing", { withdrawing: "yes" }],
    ["withdrawing", { withdrawing: false }],
    ["rewind", { rewind: "u-1" }],
    ["rewind", { rewind: { to: 1, how: "conversation", when: "now" } }],
    ["rewind", { rewind: { to: "u-1", how: "everything", when: "now" } }],
    ["rewind", { rewind: { to: "u-1", how: "conversation", when: "later" } }],
    ["attachments", { attachments: [7] }],
  ])("★ %s 形状不对 ⇒ 那一句丢掉，同会话好的那一句照读", (field, bad) => {
    seed(bad);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(new Outbox(vi.fn()).of("devbox", "s-1").map((o) => o.id)).toEqual(["monitor-ok"]);
    expect(warn.mock.calls.some((c) => c.join(" ").includes(field))).toBe(true);
    warn.mockRestore();
  });

  it("那几格都在且形状对 ⇒ 照读，不记日志", () => {
    seed({ refused: "队满了", withdrawing: true, rewind: { to: "u-1", how: "code_and_conversation", when: "after_turn" } });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(new Outbox(vi.fn()).of("devbox", "s-1").map((o) => o.id)).toEqual(["monitor-bad", "monitor-ok"]);
    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });
});
