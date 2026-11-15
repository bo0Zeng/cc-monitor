/**
 * 输入框上方那一排里的待发（`composer.ts` ＋ `outbox.ts`）：那一行只在那一句还撤得回时摆。
 * 名单里 `sent` 有它 ⇒ 当回执到了，那一行不画；撤待发等那台答了再回字，没撤成 ⇒ 拿掉那一行、字不回框。读写换成假的（照真的形状回），脸取金样。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { Composer, type ComposerReads } from "../../../src/frontend/ui/composer";
import { Outbox } from "../../../src/frontend/ui/outbox";
import type { QueueMoved } from "../../../src/frontend/ui/session-queue";
import type { AttachIo } from "../../../src/frontend/ui/attach-draft";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { QueuePut } from "../../../src/frontend/ui/generated/QueuePut";
import type { QueueCancelled } from "../../../src/frontend/ui/generated/QueueCancelled";
import type { QueueList } from "../../../src/frontend/ui/generated/QueueList";
import type { SessionQueuePutReq } from "../../../src/frontend/ui/generated/SessionQueuePutReq";
import { ChanError } from "../../../src/comms/inward/chan";
import { liveFace } from "../../test-support/session-faces";
import { LS_KEYS } from "../../../src/frontend/ui/local-storage";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

const empty = (sent: string[] = []): QueueList => ({ items: [], sent });
const queued = (): QueuePut => ({ outcome: "queued", seq: 1, said: "q" });
const down = (): ChanError => new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");

function rig(put: (o: string, r: SessionQueuePutReq) => Promise<QueuePut>, cancel?: (id: string) => Promise<QueueCancelled>) {
  const tabOf = (sid: string): Tab => ({ origin: "devbox", sessionId: sid, face: liveFace("idle", true), label: "t", labelProject: null, background: false, forkedFromSessionId: null }) as unknown as Tab;
  let tab = tabOf("s-1");
  let push: (m: QueueMoved) => void = () => {};
  const putFn = vi.fn(put);
  const reads: ComposerReads = {
    io: () => ({}) as AttachIo,
    read: vi.fn(async () => empty()),
    cancel: vi.fn(async (_o: string, _sid: string, id: string) => (cancel ? cancel(id) : { cancelled: true, said: "said-cancelled", queue: empty() })),
    watch: vi.fn((_o: string, on: (m: QueueMoved) => void) => {
      push = on;
      return () => {};
    }),
  };
  const outbox = new Outbox(putFn, reads.cancel);
  const c = new Composer({ active: () => tab, resume: async () => true, focusStream: () => {} }, outbox, reads);
  document.body.replaceChildren(c.el);
  c.render();
  const setSid = (sid: string): void => {
    tab = tabOf(sid);
    c.render();
  };
  return { c, put: putFn, reads, outbox, setSid, push: (m: QueueMoved) => push(m), id: (): string => putFn.mock.calls[0]![1].id };
}

const box = (c: Composer): HTMLTextAreaElement => c.el.querySelector("textarea") as HTMLTextAreaElement;
const enter = (c: Composer, text: string): void => {
  box(c).value = text;
  box(c).dispatchEvent(new Event("input"));
  box(c).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
};
const lines = (c: Composer): HTMLElement[] => Array.from(c.el.querySelectorAll<HTMLElement>('[data-role="queue-line"]'));
const cancelIn = (c: Composer): void => (c.el.querySelector('[data-role="queue-line"] button') as HTMLButtonElement).click();

beforeEach(() => localStorage.clear());

describe("待发那一行只在撤得回时摆", () => {
  it("★ 回执还没到、推来的名单 items 空 · sent 有它 ⇒ 那一排没有这一行、没有［撤回］；框清了", async () => {
    const r = rig(() => new Promise<QueuePut>(() => {}));
    await flush();
    enter(r.c, "已送出");
    await flush();
    expect(lines(r.c).length, "在飞：发送中 · 等确认那一行").toBe(1);
    r.push({ got: new Map([["s-1", empty([r.id()])]]), ask: [], all: false });
    await flush();
    expect(lines(r.c), "名单说已送 ⇒ 不再摆那一行").toEqual([]);
    expect(r.c.el.querySelector('[data-role="queue-line"] button')).toBeNull();
    expect(r.outbox.of("devbox", "s-1")).toEqual([]);
    expect(box(r.c).value, "送进去了：同回执到了，清框").toBe("");
  });

  it("★ 重问回来的名单 sent 有它（没回执、挂成无应答的那一句）⇒ 那一行收掉，不等那台接上", async () => {
    const r = rig(async () => Promise.reject(down()));
    await flush();
    enter(r.c, "挂着");
    await flush();
    expect(lines(r.c).length).toBe(1);
    (r.reads.read as ReturnType<typeof vi.fn>).mockResolvedValueOnce(empty([r.id()]));
    r.push({ got: new Map(), ask: ["s-1"], all: false });
    await flush();
    expect(lines(r.c)).toEqual([]);
    expect(new Outbox(vi.fn()).of("devbox", "s-1"), "本地存储也删了").toEqual([]);
  });

  it("推来的一帧里别的会话的名单 sent 有那条会话的待发 ⇒ 那一句离开待发", async () => {
    const r = rig(async () => Promise.reject(down()));
    await flush();
    await r.outbox.say("devbox", "s-2", "别处那一句", []);
    const id = r.put.mock.calls[0]![1].id;
    expect(r.outbox.of("devbox", "s-2").length).toBe(1);
    r.push({ got: new Map([["s-2", empty([id])]]), ask: [], all: false });
    await flush();
    expect(r.outbox.of("devbox", "s-2")).toEqual([]);
  });

  it("★［撤回］待发那一句、那台答「没撤成」⇒ 答之前那一行还在、字没回框；答了 ⇒ 那一行没了、框里的字不变", async () => {
    let answer: ((r: QueueCancelled) => void) | null = null;
    const r = rig(
      async () => Promise.reject(down()),
      () => new Promise<QueueCancelled>((ok) => (answer = ok)),
    );
    await flush();
    enter(r.c, "已送进会话的那一句");
    await flush();
    box(r.c).value = "";
    box(r.c).dispatchEvent(new Event("input"));
    cancelIn(r.c);
    await flush();
    expect(r.reads.cancel).toHaveBeenCalledWith("devbox", "s-1", r.id());
    expect(box(r.c).value, "那台答之前不回字").toBe("");
    expect(lines(r.c).length, "那台答之前那一行还在").toBe(1);
    cancelIn(r.c);
    await flush();
    expect(r.reads.cancel, "撤着时再点不另问").toHaveBeenCalledTimes(1);
    answer!({ cancelled: false, said: "said-sent", queue: empty() });
    await flush();
    expect(lines(r.c), "没撤成：拿掉那一行").toEqual([]);
    expect(box(r.c).value, "没撤成：字不回框").toBe("");
    expect(r.c.el.querySelector('[class*="composerNoteError"]')?.textContent, "没撤成：框下照摆撤回包里那台那一句（同手机）").toBe("said-sent");
    expect(r.outbox.of("devbox", "s-1")).toEqual([]);
  });

  it("回执没到、框里还是那一句时撤、那台答没撤成 ⇒ 同回执到了：清框", async () => {
    const r = rig(() => new Promise<QueuePut>(() => {}), async () => ({ cancelled: false, said: "said-sent", queue: empty() }));
    await flush();
    enter(r.c, "在送的那一句");
    await flush();
    expect(box(r.c).value).toBe("在送的那一句");
    cancelIn(r.c);
    await flush();
    expect(lines(r.c)).toEqual([]);
    expect(box(r.c).value).toBe("");
  });

  it("★［撤回］待发那一句、那台答撤成了 ⇒ 答了才回字", async () => {
    let answer: ((r: QueueCancelled) => void) | null = null;
    const r = rig(
      async () => Promise.reject(down()),
      () => new Promise<QueueCancelled>((ok) => (answer = ok)),
    );
    await flush();
    enter(r.c, "还排着");
    await flush();
    box(r.c).value = "";
    box(r.c).dispatchEvent(new Event("input"));
    cancelIn(r.c);
    await flush();
    expect(box(r.c).value).toBe("");
    answer!({ cancelled: true, said: "said-cancelled", queue: empty() });
    await flush();
    expect(box(r.c).value).toBe("还排着");
    expect(lines(r.c)).toEqual([]);
  });

  it("［撤回］那一问没答复（断着）⇒ 字不回框、框下写那一句", async () => {
    const r = rig(
      async () => Promise.reject(down()),
      async () => Promise.reject(new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "cancel-said")),
    );
    await flush();
    enter(r.c, "说不清");
    await flush();
    box(r.c).value = "别的";
    box(r.c).dispatchEvent(new Event("input"));
    cancelIn(r.c);
    await flush();
    expect(box(r.c).value, "字不回框").toBe("别的");
    expect(lines(r.c)).toEqual([]);
    expect(r.c.el.querySelector('[class*="composerNoteError"]')?.textContent).toBe("cancel-said");
  });

  it("★ 名单说已送（put 还在飞）⇒ 当场就能再发下一句", async () => {
    const r = rig(() => new Promise<QueuePut>(() => {}));
    await flush();
    enter(r.c, "第一句");
    await flush();
    r.push({ got: new Map([["s-1", empty([r.id()])]]), ask: [], all: false });
    await flush();
    enter(r.c, "第二句");
    await flush();
    expect(r.put).toHaveBeenCalledTimes(2);
  });

  it("★ 头一句名单说已送、又发同样一句（另一个编号）⇒ 头一句那一问回来不解开第二句的 ➤", async () => {
    const answers: ((v: QueuePut) => void)[] = [];
    const r = rig(() => new Promise<QueuePut>((res) => answers.push(res)));
    await flush();
    enter(r.c, "A");
    await flush();
    r.push({ got: new Map([["s-1", empty([r.id()])]]), ask: [], all: false });
    await flush();
    enter(r.c, "A");
    await flush();
    expect(r.put).toHaveBeenCalledTimes(2);
    answers[0]!({ outcome: "queued", seq: 1, said: "q" });
    await flush();
    enter(r.c, "B");
    await flush();
    expect(r.put, "第二句还在交：➤ 锁着").toHaveBeenCalledTimes(2);
  });

  it("★ 撤那一问没答复、之后回执到了 ⇒ 同回执到了：清框", async () => {
    let ok: (v: QueuePut) => void = () => {};
    const r = rig(
      () => new Promise<QueuePut>((res) => (ok = res)),
      async () => Promise.reject(new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "cancel-said")),
    );
    await flush();
    enter(r.c, "说不清的那一句");
    await flush();
    cancelIn(r.c);
    await flush();
    expect(box(r.c).value).toBe("说不清的那一句");
    expect(lines(r.c), "没答复 ⇒ 本地拿掉").toEqual([]);
    ok({ outcome: "queued", seq: 1, said: "q" });
    await flush();
    expect(box(r.c).value).toBe("");
  });

  it("★ 撤成了、答复回来时已切到别的会话 ⇒ 那一句回到那条会话的草稿", async () => {
    let answer: ((r: QueueCancelled) => void) | null = null;
    const r = rig(
      async () => Promise.reject(down()),
      () => new Promise<QueueCancelled>((ok) => (answer = ok)),
    );
    await flush();
    enter(r.c, "要撤的");
    await flush();
    box(r.c).value = "";
    box(r.c).dispatchEvent(new Event("input"));
    cancelIn(r.c);
    await flush();
    r.setSid("s-2");
    answer!({ cancelled: true, said: "said-cancelled", queue: empty() });
    await flush();
    r.setSid("s-1");
    expect(box(r.c).value).toBe("要撤的");
  });

  it("那台拒过的待发点［撤回］⇒ 不问那台、那一行没了、字回框", async () => {
    const no = new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code: "queue_full", said: "full" })) }, "full");
    const r = rig(async () => Promise.reject(no));
    await flush();
    enter(r.c, "拒了的");
    await flush();
    box(r.c).value = "";
    box(r.c).dispatchEvent(new Event("input"));
    cancelIn(r.c);
    await flush();
    expect(r.reads.cancel).not.toHaveBeenCalled();
    expect(lines(r.c)).toEqual([]);
    expect(box(r.c).value).toBe("拒了的");
  });

  it("编辑已发的那一句（带 rewind）撤成了 ⇒ 字留在编辑草稿，不回输入框", async () => {
    localStorage.setItem(LS_KEYS.saidEditDrafts, JSON.stringify({ "devbox\u0000s-1\u0000u-1": "改过的那一句" }));
    const r = rig(async () => Promise.reject(down()));
    await flush();
    await r.outbox.say("devbox", "s-1", "改过的那一句", [], { to: "u-1", how: "conversation", when: "after_turn" });
    await flush();
    expect(lines(r.c).length).toBe(1);
    cancelIn(r.c);
    await flush();
    expect(r.reads.cancel).toHaveBeenCalledTimes(1);
    expect(lines(r.c)).toEqual([]);
    expect(box(r.c).value).toBe("");
  });

  it("★ 编辑已发的那一句撤成了、它的编辑草稿却已不在（回执先到、照它删了）⇒ 字回输入框，不丢（同手机）", async () => {
    const r = rig(async () => Promise.reject(down()));
    await flush();
    await r.outbox.say("devbox", "s-1", "改过的那一句", [], { to: "u-1", how: "conversation", when: "after_turn" });
    await flush();
    cancelIn(r.c);
    await flush();
    expect(box(r.c).value).toBe("改过的那一句");
  });

  it("撤名单里那一条，撤回包名单的 sent 含某句待发 ⇒ 那一句离开待发、本地存储里也没了", async () => {
    const r = rig(async () => Promise.reject(down()));
    await flush();
    enter(r.c, "挂着的那一句");
    await flush();
    const id = r.id();
    r.push({ got: new Map([["s-1", { items: [{ seq: 1, id: "q-1", end: "mobile", text: "别端那一句", state: "queued", said: "排着" }], sent: [] }]]), ask: [], all: false });
    await flush();
    (r.reads.cancel as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ cancelled: true, said: "said-cancelled", queue: empty([id]) });
    r.c.el.querySelectorAll<HTMLButtonElement>('[data-role="queue-line"] button')[0]!.click();
    await flush();
    expect(r.reads.cancel).toHaveBeenCalledWith("devbox", "s-1", "q-1");
    expect(r.outbox.of("devbox", "s-1")).toEqual([]);
    expect(new Outbox(vi.fn()).of("devbox", "s-1")).toEqual([]);
  });
});

describe("撤着跨重开：照机器状态的 withdrawHeld 摆、接上接着问（同手机）", () => {
  /** 上一回窗口里：一句交了没回执、点了撤、那一问还没答就重开了。 */
  async function parkedBefore(text: string): Promise<string> {
    const first = new Outbox(async () => Promise.reject(down()), () => new Promise<QueueCancelled>(() => {}));
    await first.say("devbox", "s-1", text, []);
    const id = first.of("devbox", "s-1")[0]!.id;
    void first.withdraw("devbox", "s-1", id);
    await flush();
    return id;
  }
  const machine = (offline: boolean) => ({ offline, offlineSaid: "devbox 离线", heldSaid: "devbox held-said", withdrawHeld: "devbox withdraw-held" });

  it("★ 重开读回撤着的那一句、那台断着 ⇒ 那一行淡着、照那台成品的 withdrawHeld（不借 heldSaid）", async () => {
    await parkedBefore("撤着的");
    const r = rig(async () => Promise.reject(down()));
    r.c.machineChanged("devbox", machine(true));
    await flush();
    expect(lines(r.c).length).toBe(1);
    expect(lines(r.c)[0]!.dataset.state).toBe("held");
    expect(lines(r.c)[0]!.textContent).toContain("devbox withdraw-held");
    expect(lines(r.c)[0]!.textContent).not.toContain("devbox held-said");
  });

  it("★ 那台接上 ⇒ 不重交、照同一个编号接着问；那台已记撤过 ⇒ 撤成了：那一行收掉、字回输入框（不用打开再点）", async () => {
    const id = await parkedBefore("撤着的");
    const r = rig(async () => queued());
    r.outbox.machineUp("devbox");
    await flush();
    expect(r.put).not.toHaveBeenCalled();
    expect(r.reads.cancel).toHaveBeenCalledWith("devbox", "s-1", id);
    expect(lines(r.c)).toEqual([]);
    expect(box(r.c).value).toBe("撤着的");
  });

  it("接着问没答复 ⇒ 那一行仍在、仍撤着，框下写那一问回来的那一句；字不回框", async () => {
    await parkedBefore("撤着的");
    const r = rig(async () => queued(), async () => Promise.reject(new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "cancel-said")));
    r.outbox.machineUp("devbox");
    await flush();
    r.c.machineChanged("devbox", machine(true));
    expect(lines(r.c).length).toBe(1);
    expect(lines(r.c)[0]!.textContent).toContain("devbox withdraw-held");
    expect(r.c.el.querySelector('[class*="composerNoteError"]')?.textContent).toBe("cancel-said");
    expect(box(r.c).value).toBe("");
  });
});
