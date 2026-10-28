/**
 * 终端面板的输入队那一排：照会话那台推来的整份名单画（`changed {session_queue}`，不各自记）＋ 这一端还没回执的待发；
 * 每条能撤；输入行送出的那一句（带不带附件）一律投进队，常用键与 Ctrl+回车 仍往终端里打字。读写换成假的（照真的形状回）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { TerminalPage, type AttachReads, type TerminalReads, type Watched } from "../../../src/frontend/ui/terminal-page";
import type { QueueMoved } from "../../../src/frontend/ui/session-queue";
import type { AttachIo } from "../../../src/frontend/ui/attach-draft";
import type { TerminalRow, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import type { QueuePut } from "../../../src/frontend/ui/generated/QueuePut";
import type { QueueItem } from "../../../src/frontend/ui/generated/QueueItem";
import type { QueueList } from "../../../src/frontend/ui/generated/QueueList";
import { ChanError } from "../../../src/comms/inward/chan";
import { copyText } from "../../../src/frontend/ui/copy-table";

const SLOT: Watched = { kind: "slot", origin: "devbox", terminal: "%7", name: "web-1", title: "web-console" };
const ROW: TerminalRow = { terminal: "%7", name: "web-1", sid: "s-1", clients: 0, input: "shared", programExited: false, inputNo: null };
const SHOT: TerminalShot = { lines: [], text: "$", screen: "fp", atText: "12:00:00" };

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

/** 名单里的一条（`said` 是会话那台写好的那一句：这里就是排第几）。 */
const item = (id: string, place: number): QueueItem => ({ seq: place, id, end: "mobile", atMs: 1_700_000_000_000, text: `text of ${id}`, state: "queued", said: `place-${place}`, from: "phone-word" });
const list = (...items: QueueItem[]): QueueList => ({ items, sent: [] });

function rig(opts: { first?: QueueList; put?: () => Promise<QueuePut> } = {}) {
  const io = {} as AttachIo;
  let push: (m: QueueMoved) => void = () => {};
  const put = vi.fn(opts.put ?? (async (): Promise<QueuePut> => ({ outcome: "queued", seq: 9, place: 1, said: "q-1" })));
  const read = vi.fn(async () => opts.first ?? list());
  const cancel = vi.fn(async (_o: string, _sid: string, id: string) => ({ cancelled: true, queue: list(...(opts.first?.items ?? []).filter((i) => i.id !== id)) }));
  const watch = vi.fn((_o: string, on: (m: QueueMoved) => void) => {
    push = on;
    return () => {};
  });
  const reads = {
    list: vi.fn(async () => [ROW]),
    shot: vi.fn(async () => SHOT),
    send: vi.fn(async () => ({ result: "delivered" as const, acts: [] })),
    follow: vi.fn(() => ({ stop: () => {} })),
  };
  const attach: AttachReads = { put, io: () => io, read, cancel, watch };
  const page = new TerminalPage({ active: () => SLOT, front: () => {}, focusStream: () => {} }, reads as unknown as TerminalReads, attach);
  document.body.replaceChildren(page.el);
  return { page, reads, put, read, cancel, push: (m: QueueMoved) => push(m) };
}

async function opened(opts?: Parameters<typeof rig>[0]) {
  const r = rig(opts);
  r.page.sessionChanged();
  r.page.setVisible(true);
  await flush();
  return r;
}

/** 那一排里的每一条（排第几 / 发送中 · 那一句）。 */
const rows = (page: TerminalPage): string[] => Array.from(page.el.querySelectorAll<HTMLElement>('[data-role="queue-line"]')).map((l) => l.textContent ?? "");
const cancelOf = (page: TerminalPage, i: number): HTMLButtonElement =>
  page.el.querySelectorAll<HTMLElement>('[data-role="queue-line"]')[i]!.querySelector("button") as HTMLButtonElement;
const box = (page: TerminalPage): HTMLTextAreaElement => page.el.querySelector("textarea") as HTMLTextAreaElement;

beforeEach(() => localStorage.clear());

describe("终端面板的输入队那一排", () => {
  it("★ 照那台推来的名单画：别的一端撤掉第 1 条 ⇒ 当场少一条、原第 2 条排第 1", async () => {
    const { page, read, push } = await opened({ first: list(item("a", 1), item("b", 2)) });
    expect(read).toHaveBeenCalledWith("devbox", "s-1");
    expect(rows(page).length).toBe(2);
    expect(rows(page)[0]).toContain("place-1");
    expect(rows(page)[0]).toContain("text of a");
    push({ got: new Map([["s-1", list(item("b", 1))]]), ask: [], all: false });
    await flush();
    expect(rows(page).length).toBe(1);
    expect(rows(page)[0]).toContain("place-1");
    expect(rows(page)[0]).toContain("text of b");
  });

  it("别的会话的那一份不动这一排；没带名单 ⇒ 重问", async () => {
    const { page, read, push } = await opened({ first: list(item("a", 1)) });
    push({ got: new Map([["s-other", list()]]), ask: [], all: false });
    await flush();
    expect(rows(page).length).toBe(1);
    push({ got: new Map(), ask: ["s-1"], all: false });
    await flush();
    expect(read).toHaveBeenCalledTimes(2);
  });

  it("［撤］⇒ 照编号请那台撤（session-queue-cancel），照回的名单画", async () => {
    const { page, cancel } = await opened({ first: list(item("a", 1), item("b", 2)) });
    cancelOf(page, 0).click();
    await flush();
    expect(cancel).toHaveBeenCalledWith("devbox", "s-1", "a");
    expect(rows(page).length).toBe(1);
    expect(rows(page)[0]).toContain("text of b");
  });

  it("★ 输入行送出不带附件的那一句也投进队（不走送字），回执到了清框", async () => {
    const { page, reads, put } = await opened();
    box(page).value = "go on";
    box(page).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await flush();
    expect(put).toHaveBeenCalledWith("devbox", expect.objectContaining({ machine: "<local>", sid: "s-1", text: "go on", end: "monitor" }));
    expect(reads.send).not.toHaveBeenCalled();
    expect(box(page).value).toBe("");
  });

  it("Ctrl+回车 只往终端里打字（terminal-input、不补回车），不投队", async () => {
    const { page, reads, put } = await opened();
    box(page).value = "abc";
    box(page).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true }));
    await flush();
    expect(reads.send).toHaveBeenCalledWith("devbox", "%7", { text: "abc", enter: false }, "fp");
    expect(put).not.toHaveBeenCalled();
  });

  it("★ 没回执的待发在那一排写「发送中」、可撤；重开还在，那台连上照同一个编号再交", async () => {
    const down = new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");
    let up = false;
    const answer = async (): Promise<QueuePut> => (up ? { outcome: "queued", seq: 3, place: 1, said: "q-1" } : Promise.reject(down));
    const first = await opened({ put: answer });
    box(first.page).value = "接着做";
    box(first.page).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await flush();
    expect(rows(first.page).length).toBe(1);
    expect(rows(first.page)[0]).toContain(copyText("terminal.queue.sending"));
    expect(rows(first.page)[0]).toContain("接着做");
    const id = (first.put.mock.calls[0] as unknown[])[1] as { id: string };

    // 重开（同一份本地存储）：待发还在；连上 ⇒ 照同一个编号再交，回执到了那一条离开待发。
    const again = await opened({ put: answer });
    expect(rows(again.page)[0]).toContain("接着做");
    up = true;
    again.page.machineUp("devbox");
    await flush();
    expect(((again.put.mock.calls[0] as unknown[])[1] as { id: string }).id).toBe(id.id);
    expect(rows(again.page).some((r) => r.includes(copyText("terminal.queue.sending")))).toBe(false);
  });

  it("撤待发里那一句 ⇒ 本地删掉、照编号请那台撤", async () => {
    const down = new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");
    const { page, cancel, put } = await opened({ put: async () => Promise.reject(down) });
    box(page).value = "算了";
    box(page).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await flush();
    cancelOf(page, 0).click();
    await flush();
    const id = ((put.mock.calls[0] as unknown[])[1] as { id: string }).id;
    expect(cancel).toHaveBeenCalledWith("devbox", "s-1", id);
    expect(rows(page)).toEqual([]);
    expect(box(page).value).toBe("算了");
  });
});
