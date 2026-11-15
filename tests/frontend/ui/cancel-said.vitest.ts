/**
 * 撤没撤成照那台说（`composer.ts`）· 编辑框认得「撤了」（`said-edit.ts` ＋ `outbox.ts`）。
 * 撤待发那一句 · 撤名单里那一条，那台答没撤成 ⇒ 框下照撤回包的 `said` 写那台那一句；撤成了 ⇒ 框下不写。
 * 编辑已发的那一句撤成了（`say` 回 `{at:"cancelled"}`）⇒ 编辑框开着、字还在、卡上不挂「改后的一版待发」。
 * 读写换成假的（照真的形状回），脸取金样。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { Composer, type ComposerReads } from "../../../src/frontend/ui/composer";
import { Outbox } from "../../../src/frontend/ui/outbox";
import { SaidEdit, type SaidAt, type SaidEditIo } from "../../../src/frontend/ui/said-edit";
import type { PutEnd, QueueMoved } from "../../../src/frontend/ui/session-queue";
import type { AttachIo } from "../../../src/frontend/ui/attach-draft";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { QueuePut } from "../../../src/frontend/ui/generated/QueuePut";
import type { QueueCancelled } from "../../../src/frontend/ui/generated/QueueCancelled";
import type { QueueList } from "../../../src/frontend/ui/generated/QueueList";
import type { RewindOptions } from "../../../src/frontend/ui/generated/RewindOptions";
import { ChanError } from "../../../src/comms/inward/chan";
import { liveFace } from "../../test-support/session-faces";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

const empty = (sent: string[] = []): QueueList => ({ items: [], sent });
const down = (): ChanError => new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");

function rig(cancel: (id: string) => Promise<QueueCancelled>) {
  const tab = { origin: "devbox", sessionId: "s-1", face: liveFace("idle", true), label: "t", labelProject: null, background: false, forkedFromSessionId: null } as unknown as Tab;
  let push: (m: QueueMoved) => void = () => {};
  const put = vi.fn(async (): Promise<QueuePut> => Promise.reject(down()));
  const reads: ComposerReads = {
    io: () => ({}) as AttachIo,
    read: vi.fn(async () => empty()),
    cancel: vi.fn(async (_o: string, _sid: string, id: string) => cancel(id)),
    watch: vi.fn((_o: string, on: (m: QueueMoved) => void) => {
      push = on;
      return () => {};
    }),
  };
  const c = new Composer({ active: () => tab, resume: async () => true, focusStream: () => {} }, new Outbox(put, reads.cancel), reads);
  document.body.replaceChildren(c.el);
  c.render();
  return { c, push: (m: QueueMoved) => push(m) };
}

const box = (c: Composer): HTMLTextAreaElement => c.el.querySelector("textarea") as HTMLTextAreaElement;
const enter = (c: Composer, text: string): void => {
  box(c).value = text;
  box(c).dispatchEvent(new Event("input"));
  box(c).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
};
const clear = (c: Composer): void => {
  box(c).value = "";
  box(c).dispatchEvent(new Event("input"));
};
const lines = (c: Composer): HTMLElement[] => Array.from(c.el.querySelectorAll<HTMLElement>('[data-role="queue-line"]'));
const cancelIn = (c: Composer): void => (c.el.querySelector('[data-role="queue-line"] button') as HTMLButtonElement).click();
const note = (c: Composer): string | null => c.el.querySelector('[class*="composerNoteError"]')?.textContent ?? null;

beforeEach(() => localStorage.clear());

describe("撤没撤成照那台说", () => {
  it("★ 撤待发那一句、那台答没撤成 ⇒ 框下写那台那一句、字不回框、那一行没了", async () => {
    const r = rig(async () => ({ cancelled: false, said: "said-sent", queue: empty() }));
    await flush();
    enter(r.c, "已送进去的");
    await flush();
    clear(r.c);
    cancelIn(r.c);
    await flush();
    expect(note(r.c)).toBe("said-sent");
    expect(box(r.c).value).toBe("");
    expect(lines(r.c)).toEqual([]);
  });

  it("★ 撤名单里那一条、那台答没撤成 ⇒ 框下写那台那一句", async () => {
    const r = rig(async () => ({ cancelled: false, said: "said-sending", queue: empty() }));
    await flush();
    r.push({ got: new Map([["s-1", { items: [{ seq: 1, id: "q-1", end: "mobile", text: "别端那一句", state: "queued", said: "排着" }], sent: [] }]]), ask: [], all: false });
    await flush();
    expect(lines(r.c).length).toBe(1);
    cancelIn(r.c);
    await flush();
    expect(note(r.c)).toBe("said-sending");
    expect(box(r.c).value, "没撤成：字不回框").toBe("");
  });

  it("撤名单里那一条、那台答撤成了 ⇒ 框下不写", async () => {
    const r = rig(async () => ({ cancelled: true, said: "said-done", queue: empty() }));
    await flush();
    r.push({ got: new Map([["s-1", { items: [{ seq: 1, id: "q-1", end: "mobile", text: "别端那一句", state: "queued", said: "排着" }], sent: [] }]]), ask: [], all: false });
    await flush();
    cancelIn(r.c);
    await flush();
    expect(note(r.c)).toBeNull();
    expect(lines(r.c)).toEqual([]);
  });

  it("撤待发那一句、那台答撤成了 ⇒ 字回框（框里已有字另起一行）、框下不写", async () => {
    const r = rig(async () => ({ cancelled: true, said: "said-done", queue: empty() }));
    await flush();
    enter(r.c, "要撤的");
    await flush();
    box(r.c).value = "别的";
    box(r.c).dispatchEvent(new Event("input"));
    cancelIn(r.c);
    await flush();
    expect(box(r.c).value).toBe("别的\n要撤的");
    expect(note(r.c)).toBeNull();
  });
});

const AT: SaidAt = { origin: "devbox", sid: "s-1", id: "u-2", text: "第二句" };
const IDLE: RewindOptions = { how: [{ how: "conversation", text: "how-conv", default: true, enabled: true }], when: [] };

function card(): HTMLElement {
  const content = document.createElement("div");
  content.className = "stream-content";
  const c = document.createElement("div");
  c.className = "card card-user";
  const body = document.createElement("div");
  body.className = "card-body";
  body.textContent = "第二句";
  c.appendChild(body);
  content.appendChild(c);
  document.body.replaceChildren(content);
  return c;
}

const editBox = (): HTMLTextAreaElement | null => document.querySelector('[data-role="said-edit-box"]');
const confirm = (): void => (document.querySelector('[data-role="said-edit-confirm"]') as HTMLButtonElement).click();
const pendingMark = (c: HTMLElement): Element | null => c.querySelector('[data-role="said-edit-pending"]');

describe("编辑框认得「撤了」", () => {
  it("★ send 回撤了 ⇒ 编辑框开着、字还在、卡上不挂「改后的一版待发」", async () => {
    const c = card();
    const io: SaidEditIo = { options: vi.fn(async () => IDLE), send: vi.fn(async (): Promise<PutEnd> => ({ at: "cancelled" })) };
    const edit = new SaidEdit(io);
    edit.open(c, AT);
    await flush();
    editBox()!.value = "改过的";
    editBox()!.dispatchEvent(new Event("input"));
    confirm();
    await flush();
    expect(editBox()?.value).toBe("改过的");
    expect(pendingMark(c)).toBeNull();
  });

  it("★ 经待发交、在飞时撤成了、回执晚到 ⇒ 编辑框开着、不挂「待发」", async () => {
    const c = card();
    let ok: (v: QueuePut) => void = () => {};
    const outbox = new Outbox(
      vi.fn(() => new Promise<QueuePut>((res) => (ok = res))),
      vi.fn(async (): Promise<QueueCancelled> => ({ cancelled: true, said: "said-done", queue: empty() })),
    );
    const edit = new SaidEdit({ options: vi.fn(async () => IDLE), send: (origin, sid, text, rewind) => outbox.say(origin, sid, text, [], rewind) });
    edit.open(c, AT);
    await flush();
    confirm();
    await flush();
    await outbox.withdraw("devbox", "s-1", outbox.of("devbox", "s-1")[0]!.id);
    ok({ outcome: "queued", seq: 1, said: "q" });
    await flush();
    expect(editBox()?.value).toBe("第二句");
    expect(pendingMark(c)).toBeNull();
  });
});
