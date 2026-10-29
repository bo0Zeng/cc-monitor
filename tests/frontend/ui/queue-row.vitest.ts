/**
 * 输入框上方那一排输入队（`queue-row.ts`）：每条一行不写几点；那台离线 ⇒ 照手上那一份淡着画、头上一行说名单是上次所知（前半句是连接层那句离线）；
 * 输入框照机器成品交来的离线与否切这一态（`composer.ts::machineChanged`）。限高三行是排版，截图台架量（`tests/shots/scenes/layout-check.ts`）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { QueueRow } from "../../../src/frontend/ui/queue-row";
import { Composer, type ComposerReads } from "../../../src/frontend/ui/composer";
import { Outbox } from "../../../src/frontend/ui/outbox";
import { outgoing } from "../../../src/frontend/ui/session-queue";
import type { AttachIo } from "../../../src/frontend/ui/attach-draft";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { QueueItem } from "../../../src/frontend/ui/generated/QueueItem";
import type { QueueList } from "../../../src/frontend/ui/generated/QueueList";
import { liveFace } from "../../test-support/session-faces";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

const item = (n: number): QueueItem => ({ seq: n, id: `q-${n}`, end: "mobile", atMs: 1_700_000_000_000, text: `text ${n}`, state: "queued", said: `place-${n}`, from: "phone-word" });
const five = (): QueueList => ({ items: [1, 2, 3, 4, 5].map(item), sent: [] });
const head = (root: HTMLElement): string | null => root.querySelector<HTMLElement>('[data-role="queue-head"]')?.textContent ?? null;

beforeEach(() => localStorage.clear());

describe("输入框上方那一排输入队", () => {
  it("★ 那台离线 ⇒ 头上一行是「devbox 离线 · 名单为上次所知」，那一排标淡；名单与待发照手上那一份画", () => {
    const row = new QueueRow({ cancel: () => {}, cancelPending: () => {} });
    row.paint(five(), [outgoing("s-1", "pending one", [])], "devbox 离线");
    expect(head(row.el)).toBe("devbox 离线 · 名单为上次所知");
    expect(row.el.dataset.stale).toBe("true");
    const lines = row.el.querySelectorAll('[data-role="queue-lines"] > [data-role="queue-line"]');
    expect(lines.length, "五条名单 ＋ 一句发送中，都在那一框里").toBe(6);
  });

  it("连着 ⇒ 没有头上那一行、不标淡；一条都没有 ⇒ 收起（离线也不出头上那一行）", () => {
    const row = new QueueRow({ cancel: () => {}, cancelPending: () => {} });
    row.paint(five(), [], null);
    expect(head(row.el)).toBeNull();
    expect(row.el.dataset.stale).toBeUndefined();
    row.paint({ items: [], sent: [] }, [], "devbox 离线");
    expect(row.el.hidden).toBe(true);
    expect(head(row.el)).toBeNull();
  });

  it("每条一行只摆那台写好的那几样：那一句 · 谁发的 · 开头 ·［撤回］，不写几点", () => {
    const row = new QueueRow({ cancel: () => {}, cancelPending: () => {} });
    row.paint({ items: [item(1)], sent: [] }, [], null);
    const spans = Array.from(row.el.querySelectorAll('[data-role="queue-line"] > span')).map((s) => s.textContent);
    expect(spans).toEqual(["place-1", "phone-word", "text 1"]);
  });

  it("输入框照机器成品切：那台离线 ⇒ 那一排出头上那一行；接上 ⇒ 拿掉；别台离线不碰这一条", async () => {
    const tab = { origin: "devbox", sessionId: "s-1", face: liveFace("idle", true), label: "t", labelProject: null, background: false, forkedFromSessionId: null } as unknown as Tab;
    const reads: ComposerReads = {
      io: () => ({}) as AttachIo,
      read: vi.fn(async () => five()),
      cancel: vi.fn(async () => ({ cancelled: true, queue: five() })),
      watch: vi.fn(() => () => {}),
    };
    const c = new Composer({ active: () => tab, resume: async () => true, front: () => {}, focusStream: () => {} }, new Outbox(vi.fn()), reads);
    document.body.replaceChildren(c.el);
    c.render();
    await flush();
    expect(head(c.el)).toBeNull();
    c.machineChanged("other", { offline: true, offlineSaid: "other 离线" });
    expect(head(c.el)).toBeNull();
    c.machineChanged("devbox", { offline: true, offlineSaid: "devbox 离线" });
    expect(head(c.el)).toBe("devbox 离线 · 名单为上次所知");
    c.machineChanged("devbox", { offline: false, offlineSaid: "devbox 离线" });
    expect(head(c.el)).toBeNull();
  });
});
