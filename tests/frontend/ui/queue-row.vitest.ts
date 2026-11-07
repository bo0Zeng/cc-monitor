/**
 * 输入框上方那一排输入队（`queue-row.ts`）：每条一行只摆那一句此刻的样子 · 开头 ·［撤回］，不写第几、不写哪一端；
 * 那台离线 ⇒ 照手上那一份淡着画、排头单独一行「名单为上次所知」（不拼连接层那句）。待发那一句：在飞 ⇒「发送中 · 等确认」；
 * 交了没回执 ⇒ 那台离线照它成品的 `heldSaid`、连着说「排队 · 无应答」（淡）。输入框照机器成品交来的离线与否切这一态（`composer.ts::machineChanged`）。
 * 限高三行是排版，截图台架量（`tests/shots/scenes/layout-check.ts`）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { QueueRow } from "../../../src/frontend/ui/queue-row";
import { Composer, type ComposerReads } from "../../../src/frontend/ui/composer";
import { Outbox } from "../../../src/frontend/ui/outbox";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { AttachIo } from "../../../src/frontend/ui/attach-draft";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { QueueItem } from "../../../src/frontend/ui/generated/QueueItem";
import type { QueueList } from "../../../src/frontend/ui/generated/QueueList";
import { liveFace } from "../../test-support/session-faces";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

const item = (n: number): QueueItem => ({ seq: n, id: `q-${n}`, end: "mobile", text: `text ${n}`, state: "queued", said: `said-${n}` });
const five = (): QueueList => ({ items: [1, 2, 3, 4, 5].map(item), sent: [] });
const head = (root: HTMLElement): string | null => root.querySelector<HTMLElement>('[data-role="queue-head"]')?.textContent ?? null;
const noHost = { cancel: () => {}, cancelPending: () => {} };
const machine = (offline: boolean) => ({ offline, offlineSaid: "devbox 离线", heldSaid: "devbox held-said" });

beforeEach(() => localStorage.clear());

describe("输入框上方那一排输入队", () => {
  it("★ 那台离线 ⇒ 排头单独一行「名单为上次所知」、不拼连接层那句，那一排标淡；名单与待发照手上那一份画", () => {
    const row = new QueueRow(noHost);
    row.paint(five(), [{ id: "p-1", text: "pending one", said: "held", held: true }], true);
    expect(head(row.el)).toBe(copyText("terminal.queue.stale"));
    expect(row.el.dataset.stale).toBe("true");
    const lines = row.el.querySelectorAll('[data-role="queue-lines"] > [data-role="queue-line"]');
    expect(lines.length, "五条名单 ＋ 一句待发，都在那一框里").toBe(6);
  });

  it("连着 ⇒ 没有排头那一行、不标淡；一条都没有 ⇒ 收起（离线也不出排头那一行）", () => {
    const row = new QueueRow(noHost);
    row.paint(five(), [], false);
    expect(head(row.el)).toBeNull();
    expect(row.el.dataset.stale).toBeUndefined();
    row.paint({ items: [], sent: [] }, [], true);
    expect(row.el.hidden).toBe(true);
    expect(head(row.el)).toBeNull();
  });

  it("★ 每条一行只摆那一句 · 开头 ·［撤回］：不写哪一端、不写几点", () => {
    const row = new QueueRow(noHost);
    row.paint({ items: [item(1)], sent: [] }, [], false);
    const spans = Array.from(row.el.querySelectorAll('[data-role="queue-line"] > span')).map((s) => s.textContent);
    expect(spans).toEqual(["said-1", "text 1"]);
  });

  it("★ 待发那一句照交来的那一句摆；没回执的那一行标 held（淡），在飞的不标", () => {
    const row = new QueueRow(noHost);
    row.paint(null, [
      { id: "p-1", text: "in flight", said: "confirming", held: false },
      { id: "p-2", text: "no receipt", said: "held-said", held: true },
    ], false);
    const lines = Array.from(row.el.querySelectorAll<HTMLElement>('[data-role="queue-line"]'));
    expect(lines.map((l) => [l.dataset.state, l.querySelector("span")?.textContent])).toEqual([
      ["sending", "confirming"],
      ["held", "held-said"],
    ]);
  });

  it("［撤回］交回那一句的字（字回到输入框由宿主做）", () => {
    const cancel = vi.fn();
    const cancelPending = vi.fn();
    const row = new QueueRow({ cancel, cancelPending });
    row.paint({ items: [item(1)], sent: [] }, [{ id: "p-1", text: "mine", said: "s", held: false }], false);
    const buttons = row.el.querySelectorAll<HTMLButtonElement>('[data-role="queue-line"] button');
    buttons[0]?.click();
    buttons[1]?.click();
    expect(cancel).toHaveBeenCalledWith("q-1", "text 1");
    expect(cancelPending).toHaveBeenCalledWith("p-1", "mine");
  });

  it("输入框照机器成品切：那台离线 ⇒ 那一排出排头那一行；接上 ⇒ 拿掉；别台离线不碰这一条", async () => {
    const tab = { origin: "devbox", sessionId: "s-1", face: liveFace("idle", true), label: "t", labelProject: null, background: false, forkedFromSessionId: null } as unknown as Tab;
    const reads: ComposerReads = {
      io: () => ({}) as AttachIo,
      read: vi.fn(async () => five()),
      cancel: vi.fn(async () => ({ cancelled: true, queue: five() })),
      watch: vi.fn(() => () => {}),
    };
    const c = new Composer({ active: () => tab, resume: async () => true, focusStream: () => {} }, new Outbox(vi.fn()), reads);
    document.body.replaceChildren(c.el);
    c.render();
    await flush();
    expect(head(c.el)).toBeNull();
    c.machineChanged("other", { ...machine(true), offlineSaid: "other 离线" });
    expect(head(c.el)).toBeNull();
    c.machineChanged("devbox", machine(true));
    expect(head(c.el)).toBe(copyText("terminal.queue.stale"));
    c.machineChanged("devbox", machine(false));
    expect(head(c.el)).toBeNull();
  });
});
