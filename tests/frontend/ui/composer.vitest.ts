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
import type { TerminalSent } from "../../../src/frontend/ui/terminal-reads";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

const item = (id: string, place: number): QueueItem => ({ seq: place, id, end: "mobile", text: `text of ${id}`, state: "queued", said: `place-${place}` });
const list = (...items: QueueItem[]): QueueList => ({ items, sent: [] });
const tabOf = (face: SessionFace, sid = "s-1"): Tab => ({ origin: "devbox", sessionId: sid, face, label: "t", labelProject: null, background: false, forkedFromSessionId: null }) as unknown as Tab;
const down = (): ChanError => new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");

function rig(
  opts: {
    face?: SessionFace;
    first?: QueueList;
    put?: (o: string, r: SessionQueuePutReq) => Promise<QueuePut>;
    resume?: () => Promise<boolean>;
    interrupt?: () => Promise<TerminalSent>;
  } = {},
) {
  let tab: Tab | null = tabOf(opts.face ?? liveFace("idle", true));
  let push: (m: QueueMoved) => void = () => {};
  const put = vi.fn(opts.put ?? (async (): Promise<QueuePut> => ({ outcome: "queued", seq: 9, said: "q-1" })));
  const reads: ComposerReads = {
    io: () => ({}) as AttachIo,
    read: vi.fn(async () => opts.first ?? list()),
    cancel: vi.fn(async (_o: string, _sid: string, id: string) => ({ cancelled: true, queue: list(...(opts.first?.items ?? []).filter((i) => i.id !== id)) })),
    watch: vi.fn((_o: string, on: (m: QueueMoved) => void) => {
      push = on;
      return () => {};
    }),
  };
  const interrupt = vi.fn(opts.interrupt ?? (async (): Promise<TerminalSent> => ({ result: "delivered", note: "S-delivered", acts: [] })));
  const resume = vi.fn(opts.resume ?? (async () => true));
  const outbox = new Outbox(put);
  const c = new Composer({ active: () => tab, resume, focusStream: () => {} }, outbox, reads, interrupt);
  document.body.replaceChildren(c.el);
  c.render(); // 第一次画由宿主叫（构造里不画，A2-17-10-1-2）
  return {
    c,
    put,
    reads,
    interrupt,
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
/** 一轮在跑、说得了的那张脸：金样里「在干活」那一张（`can.stop = yes`），说一句那一格换成闲着那一张的（在终端宿主里活着）。 */
const runningFace = (): SessionFace => {
  const f = liveFace("working", true);
  f.can.say = liveFace("idle", true).can.say;
  return f;
};
const shown = (b: HTMLElement | null): boolean => b !== null && b.style.display !== "none";
const stopBtn = (c: Composer): HTMLButtonElement | null => c.el.querySelector<HTMLButtonElement>('[data-role="composer-stop"]');
const sendBtn = (c: Composer): HTMLButtonElement | null => c.el.querySelector<HTMLButtonElement>('[data-role="composer-send"]');
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

  it("★ 那台看不见 ⇒ 照常可写可发；没回执 ⇒ 连着时那一行「排队 · 无应答」、那台离线时照它成品的 heldSaid，框下不另起一行；重开还在", async () => {
    const { c } = rig({ face: goldenFace("unseen"), put: async () => Promise.reject(down()) });
    c.machineChanged("devbox", { offline: false, offlineSaid: "devbox 离线", heldSaid: "devbox held-said" });
    expect(box(c).disabled).toBe(false);
    enter(c, "later");
    await flush();
    expect(box(c).value).toBe("later");
    expect(rows(c).length).toBe(1);
    expect(rows(c)[0], "连着时不说离线").toContain(copyText("beQueue.said.noAnswer"));
    expect(rows(c)[0]).not.toContain("devbox held-said");
    c.machineChanged("devbox", { offline: true, offlineSaid: "devbox 离线", heldSaid: "devbox held-said" });
    expect(rows(c)[0]).toContain("devbox held-said");
    expect(rows(c)[0]).not.toContain(copyText("mobChat.delivery.sending"));
    expect(rows(c)[0]).toContain("later");
    expect(c.el.textContent, "没回执那一句不在框下再说一遍").not.toContain("no-answer");
    const again = new Outbox(async () => Promise.reject(down()));
    expect(again.of("devbox", "s-1").map((o) => o.text)).toEqual(["later"]);
  });

  it("★ 交出去、回执还没到 ⇒ 那一行「发送中 · 等确认」（不是无应答 · 离线那两句）", async () => {
    const { c } = rig({ put: () => new Promise<QueuePut>(() => {}) });
    enter(c, "flying");
    await flush();
    expect(rows(c).length).toBe(1);
    expect(rows(c)[0]).toContain(copyText("beQueue.said.confirming"));
    expect(rows(c)[0]).not.toContain(copyText("beQueue.said.noAnswer"));
  });

  it("★［撤回］待发那一句 ⇒ 字回到输入框：框里已有别的字 ⇒ 另起一行接在后面；还是那一句 ⇒ 不重一遍", async () => {
    const r = rig({ put: async () => Promise.reject(down()) });
    enter(r.c, "later");
    await flush();
    box(r.c).value = "other";
    (r.c.el.querySelector('[data-role="queue-line"] button') as HTMLButtonElement).click();
    await flush();
    expect(box(r.c).value).toBe("other\nlater");
    const same = rig({ put: async () => Promise.reject(down()) });
    enter(same.c, "kept");
    await flush();
    (same.c.el.querySelector('[data-role="queue-line"] button') as HTMLButtonElement).click();
    await flush();
    expect(box(same.c).value).toBe("kept");
  });

  it("★ 那台拒了 ⇒ 字留着；那一句留在那一排、那一行写那台那一句（错误色），框下不另写；［撤回］⇒ 拿掉", async () => {
    const refused = new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code: "queue_full", said: "full-said" })) }, "full-said");
    const { c, reads } = rig({ put: async () => Promise.reject(refused) });
    enter(c, "x");
    await flush();
    expect(box(c).value).toBe("x");
    const line = c.el.querySelector<HTMLElement>('[data-role="queue-line"]');
    expect(rows(c).length).toBe(1);
    expect(line?.dataset.state).toBe("undelivered");
    expect(line?.textContent).toContain("full-said");
    expect(c.el.textContent?.split("full-said").length, "只在那一行说一遍").toBe(2);
    line?.querySelector("button")?.click();
    await flush();
    expect(rows(c)).toEqual([]);
    expect(box(c).value, "字没重一遍").toBe("x");
    expect(reads.cancel, "拒了的没进那台的队：不去请那台撤").not.toHaveBeenCalled();
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

  it("★ 那台断开又接上 ⇒ 名单只重问一次：订阅重连那一格（`all`）问，「那台接上」那一下不另问", async () => {
    const { c, push, reads } = rig({ first: list(item("a", 1)) });
    await flush();
    expect(reads.read).toHaveBeenCalledTimes(1);
    // 接上那一刻两样一起来：机器状态翻成连着（宿主叫 machineUp）· 那台 `changed {session_queue}` 订阅收一格 `seen`（⇒ all）。
    c.machineUp("devbox");
    push({ got: new Map(), ask: [], all: true });
    await flush();
    expect(reads.read).toHaveBeenCalledTimes(2);
  });

  it("［撤回］名单里那一条 ⇒ 照编号请那台撤，照回的名单画", async () => {
    const { c, reads } = rig({ first: list(item("a", 1)) });
    await flush();
    (c.el.querySelector('[data-role="queue-line"] button') as HTMLButtonElement).click();
    await flush();
    expect(reads.cancel).toHaveBeenCalledWith("devbox", "s-1", "a");
    expect(rows(c)).toEqual([]);
    expect(box(c).value, "撤掉的那一句回到输入框").toBe("text of a");
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

  it("★ 脸上 can.stop = yes、框里没字 ⇒［发送］换成 ■，点了交一次 session-interrupt；框里有字 ⇒ 还是［发送］", async () => {
    const face = runningFace();
    expect(face.can.stop.can).toBe("yes");
    const { c, interrupt, put } = rig({ face });
    await flush();
    expect(shown(stopBtn(c)), "在跑、没字 ⇒ ■").toBe(true);
    expect(shown(sendBtn(c)), "■ 与［发送］二选一").toBe(false);
    expect(stopBtn(c)!.getAttribute("aria-label") ?? stopBtn(c)!.textContent).toContain(copyText("composer.input.stop"));
    stopBtn(c)!.click();
    await flush();
    expect(interrupt).toHaveBeenCalledTimes(1);
    expect(interrupt).toHaveBeenCalledWith("devbox", "s-1");
    expect(put).not.toHaveBeenCalled();
    box(c).value = "more";
    box(c).dispatchEvent(new Event("input"));
    expect(shown(stopBtn(c)), "有字 ⇒ 不是 ■").toBe(false);
    expect(shown(sendBtn(c))).toBe(true);
  });

  it("脸上 can.stop = no（闲着）⇒ 没有 ■、是［发送］", async () => {
    const face = liveFace("idle", true);
    expect(face.can.stop.can).toBe("no");
    const { c } = rig({ face });
    await flush();
    expect(shown(stopBtn(c))).toBe(false);
    expect(shown(sendBtn(c))).toBe(true);
  });

  it("说不了（can.say = no）、一轮在跑 ⇒ 框灰着、■ 照样点得了", async () => {
    const face = goldenFace("live-working-plain");
    expect(face.can.say.can).toBe("no");
    expect(face.can.stop.can).toBe("yes");
    const { c, interrupt } = rig({ face });
    await flush();
    expect(box(c).disabled).toBe(true);
    expect(shown(stopBtn(c))).toBe(true);
    stopBtn(c)!.click();
    await flush();
    expect(interrupt).toHaveBeenCalledTimes(1);
  });

  it("■ 那台拒了 ⇒ 框下写那台那一句；送不到（抛）⇒ 写那一句；送到了 ⇒ 不说", async () => {
    const refused = rig({ face: runningFace(), interrupt: async () => ({ result: "refused", note: "not-yours-note", said: "not-yours-said", screen: null, acts: [] }) });
    await flush();
    stopBtn(refused.c)!.click();
    await flush();
    expect(refused.c.el.textContent).toContain("not-yours-said");
    const gone = new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "stop-down-said");
    const thrown = rig({ face: runningFace(), interrupt: async () => Promise.reject(gone) });
    await flush();
    stopBtn(thrown.c)!.click();
    await flush();
    expect(thrown.c.el.querySelector('[class*="composerNoteError"]')).not.toBeNull();
    const ok = rig({ face: runningFace() });
    await flush();
    stopBtn(ok.c)!.click();
    await flush();
    expect(ok.c.el.querySelector('[class*="composerNoteError"]')).toBeNull();
  });
});
