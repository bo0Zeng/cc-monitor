/**
 * 「会打断什么」：界面只问本机后端一条 `machine-interrupts`（带 `act`），画的就是回包里排好的两行（字由后端 `faces/read_face.rs::rows_of` 写），
 * 不另拼。各形怎么排由 Rust 那一侧的判据管。
 */
import { describe, expect, it, vi } from "vitest";

const { sent, reply } = vi.hoisted(() => ({
  sent: [] as Array<{ origin: string; cmd: string; req: unknown }>,
  reply: { now: null as Record<string, unknown> | null },
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

import { askForwards, askInterruptRows } from "../../../../src/frontend/ui/settings/interrupts";
import { copyText } from "../../../../src/frontend/ui/copy-table";

/** 后端排好的两行（字由后端写）。 */
const ROWS = [
  { label: "中断（后端写的）", items: ["会话请求 2（后端写的）", "端口转发 1（后端写的）"] },
  { label: "保留（后端写的）", items: ["会话照跑（后端写的）"] },
];
const hub = (rows: typeof ROWS, forwards = 0) => ({ relayedSessions: 2, relayedMaybe: 0, liveStreams: 1, forwards, rows });

describe("会打断什么 · 只问本机后端一条、照摆回包里的两行", () => {
  it("★ 停 / 更新一台：带那台的名字与 act；画的就是回包的 rows（什么都不断 ⇒ 空，不弹）", async () => {
    sent.length = 0;
    reply.now = hub(ROWS);
    expect(await askInterruptRows("devbox", "update")).toEqual(ROWS);
    expect(sent).toEqual([{ origin: "<local>", cmd: "machine-interrupts", req: { machine: "devbox", act: "update" } }]);
    reply.now = hub([]);
    expect(await askInterruptRows("devbox", "stop")).toEqual([]);
  });

  it("★ 本机那一台：同一条、不带名字", async () => {
    sent.length = 0;
    reply.now = hub(ROWS);
    await askInterruptRows("<local>", "uninstall");
    expect(sent).toEqual([{ origin: "<local>", cmd: "machine-interrupts", req: { act: "uninstall" } }]);
  });

  it("★ 本机后端问不到 ⇒ 框里只那一句（与壳关主窗那一下同一句），不另拼数不出的几项", async () => {
    reply.now = null;
    expect(await askInterruptRows("devbox", "stop")).toEqual([{ label: copyText("quitNow.dialog.unknown"), items: [] }]);
  });

  it("删一台那一句要的转发数：照回包那一格；问不到 ⇒ 0", async () => {
    reply.now = hub([], 3);
    expect(await askForwards("devbox")).toBe(3);
    reply.now = null;
    expect(await askForwards("devbox")).toBe(0);
  });
});
