/**
 * 「会打断什么」：`machine-interrupts` 金样那几形排得出「中断 · 保留」两行（回包按生成类型用，形状由握手指纹管）。
 * Rust 那一侧（`tests/backend/observe/accounts_query_tests.rs`）由生产函数现产、与同一份金样逐格比。
 */
import { describe, expect, it } from "vitest";
import golden from "../../../__fixtures__/machine-interrupts.golden.json";
import { interruptRows } from "../../../../src/frontend/ui/settings/interrupts";
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
    expect(rows[1]).toEqual({ label: copyText("interrupts.row.keep"), items: [copyText("interrupts.item.sessionsRun")] });
  });
});

describe("会打断什么 · 文件窗口那一份", () => {
  const none = { relayedSessions: 0, relayedMaybe: 0, liveStreams: 0, forwards: 0 };
  it("★ 文件窗口里有没存的改动 ⇒ 列出来（只有它也弹）；在跑的照列；数不出 ⇒ 说数不出", () => {
    const rows = interruptRows({ ...none, unsaved: 1, filewinRunning: 0 }, "各机器", "stop");
    expect(rows[0]!.items).toEqual([copyText("interrupts.item.unsaved", { n: 1 })]);
    expect(interruptRows({ ...none, unsaved: 0, filewinRunning: 2 }, "各机器", "restart")[0]!.items).toEqual([
      copyText("interrupts.item.filewinRunning", { n: 2 }),
    ]);
    expect(interruptRows({ ...none, unsaved: null, filewinRunning: null }, "各机器", "stop")[0]!.items).toEqual([
      copyText("interrupts.item.filewinUnknown"),
    ]);
    expect(interruptRows({ ...none, unsaved: 0, filewinRunning: 0 }, "各机器", "stop")).toEqual([]);
  });
});
