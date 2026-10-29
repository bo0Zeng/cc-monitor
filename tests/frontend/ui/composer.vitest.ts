/**
 * 消息流底下的输入框（`composer.ts`）：说一句只在这一处，不用开抽屉终端页。交出只走 `session-queue-put`（经待发）；
 * 上方一排照那台推来的整份名单画 ＋ 这一端的待发；能不能说只读脸上的 `can.say`。读写换成假的（照真的形状回），脸取金样。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { Composer, type ComposerReads } from "../../../src/frontend/ui/composer";
import { Outbox } from "../../../src/frontend/ui/outbox";
import type { QueueMoved } from "../../../src/frontend/ui/session-queue";
import type { AttachIo } from "../../../src/frontend/ui/attach-draft";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { SessionFace } from "../../../src/frontend/ui/generated/SessionFace";
import type { QueuePut } from "../../../src/frontend/ui/generated/QueuePut";
import type { QueueItem } from "../../../src/frontend/ui/generated/QueueItem";
import type { QueueList } from "../../../src/frontend/ui/generated/QueueList";
import type { SessionQueuePutReq } from "../../../src/frontend/ui/generated/SessionQueuePutReq";
import { ChanError } from "../../../src/comms/inward/chan";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { goldenFace, liveFace } from "../../test-support/session-faces";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

const item = (id: string, place: number): QueueItem => ({ seq: place, id, end: "mobile", text: `text of ${id}`, state: "queued", said: `place-${place}`, from: "phone-word" });
const list = (...items: QueueItem[]): QueueList => ({ items, sent: [] });
const tabOf = (face: SessionFace, sid = "s-1"): Tab => ({ origin: "devbox", sessionId: sid, face, label: "t", labelProject: null, background: false, forkedFromSessionId: null }) as unknown as Tab;
const down = (): ChanError => new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");

function rig(opts: { face?: SessionFace; first?: QueueList; put?: (o: string, r: SessionQueuePutReq) => Promise<QueuePut>; resume?: () => Promise<boolean> } = {}) {
  let tab: Tab | null = tabOf(opts.face ?? liveFace("idle", true));
  let push: (m: QueueMoved) => void = () => {};
  const put = vi.fn(opts.put ?? (async (): Promise<QueuePut> => ({ outcome: "queued", seq: 9, place: 1, said: "q-1" })));
  const reads: ComposerReads = {
    io: () => ({}) as AttachIo,
    read: vi.fn(async () => opts.first ?? list()),
    cancel: vi.fn(async (_o: string, _sid: string, id: string) => ({ cancelled: true, queue: list(...(opts.first?.items ?? []).filter((i) => i.id !== id)) })),
    watch: vi.fn((_o: string, on: (m: QueueMoved) => void) => {
      push = on;
      return () => {};
    }),
  };
  const resume = vi.fn(opts.resume ?? (async () => true));
  const outbox = new Outbox(put);
  const c = new Composer({ active: () => tab, resume, front: () => {}, focusStream: () => {} }, outbox, reads);
  document.body.replaceChildren(c.el);
  c.render(); // 第一次画由宿主叫（构造里不画，A2-17-10-1-2）
  return {
    c,
    put,
    reads,
    resume,
    outbox,
    push: (m: QueueMoved) => push(m),
    setTab: (t: Tab | null) => {
      tab = t;
      c.render();
    },
  };
}

const box = (c: Composer): HTMLTextAreaElement => c.el.querySelector("textarea") as HTMLTextAreaElement;
const enter = (c: Composer, text: string): void => {
  box(c).value = text;
  box(c).dispatchEvent(new Event("input"));
  box(c).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
};
const rows = (c: Composer): string[] => Array.from(c.el.querySelectorAll<HTMLElement>('[data-role="queue-line"]')).map((l) => l.textContent ?? "");

beforeEach(() => localStorage.clear());

describe("消息流底下的输入框", () => {
  it("★ 打到一半的草稿落本地存储：重开窗口（新起一个输入框）那条会话的框里还是那一句；发出去、回执到了 ⇒ 不再留", async () => {
    const first = rig();
    await flush();
    box(first.c).value = "半句-甲";
    box(first.c).dispatchEvent(new Event("input"));
    const again = rig();
    await flush();
    expect(box(again.c).value, "重开还在").toBe("半句-甲");
    enter(again.c, "半句-甲");
    await flush();
    const third = rig();
    await flush();
    expect(box(third.c).value, "回执到了 ⇒ 草稿删").toBe("");
  });

  it("★ 打开一条闲着的会话、不开抽屉 ⇒ 消息流底下有输入框，Enter 发出一问 session-queue-put；回执到了清框、写那台那一句", async () => {
    const { c, put } = rig();
    await flush();
    expect(c.el.hidden).toBe(false);
    expect(box(c).disabled).toBe(false);
    enter(c, "hello");
    await flush();
    expect(put).toHaveBeenCalledTimes(1);
    expect(put.mock.calls[0]![0]).toBe("devbox");
    expect(put.mock.calls[0]![1]).toMatchObject({ sid: "s-1", text: "hello" });
    expect(box(c).value).toBe("");
    expect(c.el.textContent).toContain("q-1");
  });

  it("Shift+Enter 不发（换行）", async () => {
    const { c, put } = rig();
    box(c).value = "a";
    box(c).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", shiftKey: true, bubbles: true }));
    await flush();
    expect(put).not.toHaveBeenCalled();
  });

  it("★ 脸上 can.say = no{why} ⇒ 框灰着、占位就是那一句；换成 yes ⇒ 可写", async () => {
    const no = goldenFace("unclear");
    expect(no.can.say.can).toBe("no");
    const { c, put, setTab } = rig({ face: no });
    expect(box(c).disabled).toBe(true);
    expect(box(c).placeholder).toBe(no.can.say.can === "no" ? no.can.say.why : "");
    enter(c, "x");
    await flush();
    expect(put).not.toHaveBeenCalled();
    setTab(tabOf(liveFace("idle", true)));
    expect(box(c).disabled).toBe(false);
    expect(box(c).placeholder).toBe(copyText("chat.input.placeholder"));
  });

  it("已结束能恢复 ⇒ 可写、占位说发一句会先恢复；Enter ⇒ 先恢复再交进队；恢复没成 ⇒ 不交、字留着", async () => {
    const r = rig({ face: goldenFace("ended") });
    expect(box(r.c).disabled).toBe(false);
    expect(box(r.c).placeholder).toBe(copyText("composer.input.ended"));
    enter(r.c, "again");
    await flush();
    expect(r.resume).toHaveBeenCalledWith("s-1");
    expect(r.put).toHaveBeenCalledTimes(1);
    const failed = rig({ face: goldenFace("ended"), resume: async () => false });
    enter(failed.c, "keep");
    await flush();
    expect(failed.put).not.toHaveBeenCalled();
    expect(box(failed.c).value).toBe("keep");
  });

  it("那台看不见 ⇒ 照常可写可发；没回执 ⇒ 上方那一排「发送中」、可撤；重开还在", async () => {
    const { c } = rig({ face: goldenFace("unseen"), put: async () => Promise.reject(down()) });
    expect(box(c).disabled).toBe(false);
    enter(c, "later");
    await flush();
    expect(box(c).value).toBe("later");
    expect(rows(c).length).toBe(1);
    expect(rows(c)[0]).toContain(copyText("terminal.queue.sending"));
    expect(rows(c)[0]).toContain("later");
    const again = new Outbox(async () => Promise.reject(down()));
    expect(again.of("devbox", "s-1").map((o) => o.text)).toEqual(["later"]);
  });

  it("那台拒了 ⇒ 字留着、写那台那一句", async () => {
    const refused = new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code: "queue_full", message: "full-said" })) }, "full-said");
    const { c } = rig({ put: async () => Promise.reject(refused) });
    enter(c, "x");
    await flush();
    expect(box(c).value).toBe("x");
    expect(c.el.textContent).toContain("full-said");
    expect(rows(c)).toEqual([]);
  });

  it("上方那一排照那台推来的名单画：别的一端撤掉第 1 条 ⇒ 当场少一条", async () => {
    const { c, push, reads } = rig({ first: list(item("a", 1), item("b", 2)) });
    await flush();
    expect(reads.read).toHaveBeenCalledWith("devbox", "s-1");
    expect(rows(c).length).toBe(2);
    push({ got: new Map([["s-1", list(item("b", 1))]]), ask: [], all: false });
    await flush();
    expect(rows(c).length).toBe(1);
    expect(rows(c)[0]).toContain("text of b");
  });

  it("［撤回］名单里那一条 ⇒ 照编号请那台撤，照回的名单画", async () => {
    const { c, reads } = rig({ first: list(item("a", 1)) });
    await flush();
    (c.el.querySelector('[data-role="queue-line"] button') as HTMLButtonElement).click();
    await flush();
    expect(reads.cancel).toHaveBeenCalledWith("devbox", "s-1", "a");
    expect(rows(c)).toEqual([]);
  });

  it("撤待发那一问回拒 ⇒ 写那台那一句（不吞）", async () => {
    const r = rig({ put: async () => Promise.reject(down()) });
    (r.reads.cancel as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "cancel-said"));
    enter(r.c, "x");
    await flush();
    (r.c.el.querySelector('[data-role="queue-line"] button') as HTMLButtonElement).click();
    await flush();
    expect(r.outbox.of("devbox", "s-1")).toEqual([]);
    expect(r.c.el.querySelector('[class*="composerNoteError"]')).not.toBeNull();
  });

  it("没有当前标签页 ⇒ 框收起；换会话 ⇒ 没发的字按会话各记一份", async () => {
    const { c, setTab } = rig();
    box(c).value = "draft-1";
    setTab(tabOf(liveFace("idle", true), "s-2"));
    expect(box(c).value).toBe("");
    setTab(tabOf(liveFace("idle", true), "s-1"));
    expect(box(c).value).toBe("draft-1");
    setTab(null);
    expect(c.el.hidden).toBe(true);
  });
});
