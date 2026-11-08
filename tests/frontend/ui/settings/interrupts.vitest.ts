/**
 * 「会打断什么」：`machine-interrupts` 金样那几形排得出「中断 · 保留」两行（回包按生成类型用，形状由握手指纹管）。
 * Rust 那一侧（`tests/backend/accounts/manage/library_tests.rs`）由生产函数现产、与同一份金样逐格比。
 */
import { describe, expect, it, vi } from "vitest";

const { sent, reply } = vi.hoisted(() => ({
  sent: [] as Array<{ origin: string; cmd: string; req: unknown }>,
  reply: { now: null as Record<string, number | null> | null },
}));
vi.mock("../../../../src/frontend/ui/ipc/chan-caller", () => ({
  budgetWithin: () => ({ until: 0 }),
  callTyped: (origin: string, cmd: string, req: unknown) => {
    sent.push({ origin, cmd, req });
    return reply.now ? Promise.resolve(reply.now) : Promise.reject(new Error("问不到"));
  },
}));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: { filewin_interrupts: () => Promise.resolve({ windows: 1, unsaved: 1, running: 0 }) },
}));

import golden from "../../../__fixtures__/machine-interrupts.golden.json";
import { askAppExitInterrupts, askInterrupts, interruptRows } from "../../../../src/frontend/ui/settings/interrupts";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import type { Interrupts } from "../../../../src/frontend/ui/generated/Interrupts";

type Case = { name: string; reply: Interrupts };
const cases = golden.cases as Case[];

describe("会打断什么", () => {
  it("★ 两份并排：那台的会话三格 ＋ 本机的转发；什么都不断 ⇒ 不弹", () => {
    const r = (name: string): Interrupts => cases.find((c) => c.name === name)!.reply;
    const fw = { unsaved: 0, filewinRunning: 0 };
    expect(interruptRows({ ...r("什么都不断"), ...fw }, "devbox", "stop")).toEqual([]);
    const busy = r("两个经中转 · 一个说不清 · 一个不经 · 一个已结束 · 一行坏的");
    const rows = interruptRows({ ...busy, ...fw, forwards: r("只有通往那台的转发").forwards }, "devbox", "update");
    expect(rows[0]!.label).toBe(copyText("interrupts.row.brief"));
    expect(rows[0]!.items).toEqual([
      copyText("interrupts.item.relayed", { n: 2 }),
      copyText("interrupts.item.relayedMaybe", { n: 1 }),
      copyText("interrupts.item.liveBrief", { machine: "devbox" }),
      copyText("interrupts.item.forwards", { n: 2 }),
    ]);
    expect(rows[1]).toEqual({ label: copyText("kit.interrupts.keep"), items: [copyText("interrupts.item.sessionsRun")] });
  });
});

describe("会打断什么 · 文件窗口那一份", () => {
  const none = { relayedSessions: 0, relayedMaybe: 0, liveStreams: 0, forwards: 0 };
  it("★ 文件窗口里有没存的改动 ⇒ 列出来（只有它也弹）；在跑的照列；数不出 ⇒ 说数不出", () => {
    const rows = interruptRows({ ...none, unsaved: 1, filewinRunning: 0 }, copyText("restartNow.dialog.everywhere"), "stop");
    expect(rows[0]!.items).toEqual([copyText("interrupts.item.unsaved", { n: 1 })]);
    expect(interruptRows({ ...none, unsaved: 0, filewinRunning: 2 }, copyText("restartNow.dialog.everywhere"), "restart")[0]!.items).toEqual([
      copyText("interrupts.item.filewinRunning", { n: 2 }),
    ]);
    expect(interruptRows({ ...none, unsaved: null, filewinRunning: null }, copyText("restartNow.dialog.everywhere"), "stop")[0]!.items).toEqual([
      copyText("interrupts.item.filewinUnknown"),
    ]);
    expect(interruptRows({ ...none, unsaved: 0, filewinRunning: 0 }, copyText("restartNow.dialog.everywhere"), "stop")).toEqual([]);
  });
});

describe("会打断什么 · 只问本机后端一条", () => {
  const none = { relayedSessions: 0, relayedMaybe: 0, liveStreams: 0, forwards: 0 };

  it("★ 停 / 更新一台：只发一条 `machine-interrupts` 给本机后端（带那台的名字），那台问不到 ⇒ 会话那一格 null、转发照数", async () => {
    sent.length = 0;
    reply.now = { ...none, relayedSessions: null, forwards: 2 };
    const got = await askInterrupts("devbox");
    expect(sent).toEqual([{ origin: "<local>", cmd: "machine-interrupts", req: { machine: "devbox" } }]);
    expect(got).toEqual({ ...none, relayedSessions: null, forwards: 2 });
  });

  it("★ 本机那一台：同一条、不带名字", async () => {
    sent.length = 0;
    reply.now = { ...none, liveStreams: 1 };
    expect(await askInterrupts("<local>")).toEqual({ ...none, liveStreams: 1, forwards: 0 });
    expect(sent).toEqual([{ origin: "<local>", cmd: "machine-interrupts", req: {} }]);
  });

  it("★ 本机后端问不到 ⇒ 会话与转发都说数不出", async () => {
    sent.length = 0;
    reply.now = null;
    const got = await askInterrupts("devbox");
    expect(got.relayedSessions).toBeNull();
    expect(got.forwards).toBeNull();
  });

  it("★ 现在重启 / 退出：后端只发一条（`fleet`：本机后端照自己的机器表问各台），不再 1 ＋ N、不交名单", async () => {
    sent.length = 0;
    reply.now = { ...none, relayedSessions: 2, liveStreams: 4, forwards: 2 };
    const got = await askAppExitInterrupts();
    expect(sent).toEqual([{ origin: "<local>", cmd: "machine-interrupts", req: { appExit: true, fleet: true } }]);
    expect(got).toEqual({ ...none, relayedSessions: 2, liveStreams: 4, forwards: 2, unsaved: 1, filewinRunning: 0 });
  });
});
