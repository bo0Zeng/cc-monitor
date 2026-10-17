/**
 * 计划页三件里界面那一半：接过仓库那一问的形状 · 退回框对已结束的接手改成［恢复并退回］· 设置里 pb 路径那一行（先问那台能不能用，能用才存）。
 * 判定都在后端；这里只钉界面把人的那一下原样交给后端、按回包画。期望手写，夹具是合成的中性例子。
 */
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));

const calls: { op: string; body: unknown }[] = [];
let reply: unknown = {};

const typed: { op: string; args: unknown }[] = [];
let checked: { state: string; said: string } = { state: "ok", said: "能用" };
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (orig) => ({
  ...(await orig<object>()),
  callTyped: (_o: string, op: string, args: unknown) => {
    typed.push({ op, args });
    if (op === "plan-pb-check") return Promise.resolve(checked);
    if (op.startsWith("plan-")) {
      calls.push({ op, body: args });
      return Promise.resolve(reply);
    }
    return Promise.resolve({ state: "ok", reason: null, detail: null, path: null, prefs: { launchers: {}, resumeInTmux: false, models: {}, permissionModes: {}, contextLimits: {}, agentHomes: {}, pbDir: (args as { pbDir?: string }).pbDir ?? "/opt/pb" } });
  },
}));

import { takeRepo } from "../../../src/frontend/ui/plan-reads.ts";
import { openReturn, type ReviewHost, type ReturnTarget } from "../../../src/frontend/ui/views/plan-review.ts";
import { pbDirRow } from "../../../src/frontend/ui/settings/pb-dir-row.ts";
import type { PlanSlice, PlanWho } from "../../../src/frontend/ui/plan-reads.ts";

afterEach(() => {
  calls.length = 0;
  typed.length = 0;
  document.body.replaceChildren();
});

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

describe("接过仓库", () => {
  it("带片名交 `take`，回包里接到的那一片原样给出", async () => {
    reply = { rc: 0, said: "接过来了", path: null, took: { slice: "ledger", path: "repos/ledger", switched: true } };
    const r = await takeRepo("<local>", "/work", "ledger");
    expect(calls).toEqual([{ op: "plan-command", body: { workspace: "/work", cmd: "take", slice: "ledger" } }]);
    expect(r.took).toEqual({ slice: "ledger", path: "repos/ledger", switched: true });
  });
  it("回包没说接到哪一片 ⇒ 不当成了", async () => {
    reply = { rc: 0, said: "x", path: null, took: null };
    await expect(takeRepo("<local>", "/work", "ledger")).rejects.toThrow();
  });
});

const who = (alive: boolean): PlanWho => ({ kind: "session", sid: "s-1", parent: null, needs: null, face: { live: alive ? { is: "yes", activity: "idle" } : { is: "no" } } }) as unknown as PlanWho;
const host: ReviewHost = {
  origin: () => "<local>",
  workspace: () => "/work",
  who: () => document.createElement("span"),
  sessionName: () => "甲会话",
  switchTo: () => {},
  resume: () => {},
  ack: () => {},
  next: null,
  reread: () => {},
};
const slice = { name: "ledger" } as unknown as PlanSlice;
const target = (alive: boolean): ReturnTarget => ({ id: "A1", title: "导出", done: true, blockTitle: "底座", owner: who(alive), signer: false });

async function sendReturn(alive: boolean): Promise<{ action: string; note: HTMLElement | null }> {
  reply = { line: "人 · A1 导出：改", to: who(true), result: "delivered", why: null, said: null, screen: null, resumed: !alive };
  openReturn(host, slice, target(alive));
  const ta = document.querySelector<HTMLTextAreaElement>("textarea")!;
  ta.value = "改";
  ta.dispatchEvent(new Event("input"));
  const buttons = [...document.querySelectorAll<HTMLButtonElement>("button")];
  const action = buttons.map((b) => b.textContent ?? "").find((t) => t.startsWith("plan.return.")) ?? "";
  const note = document.querySelector<HTMLElement>('[data-note="resume"]');
  ta.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  await flush();
  return { action, note };
}

describe("退回框：接手已结束", () => {
  it("主按钮是［恢复并退回］、框里说一句，送出带 `resume`", async () => {
    const { action, note } = await sendReturn(false);
    expect(action).toBe("plan.return.resumeSend");
    expect(note?.hidden).toBe(false);
    expect(note?.textContent).toBe("sessionState.planReturn.resume(session=甲会话)");
    expect(calls).toHaveLength(1);
    expect(calls[0].body).toMatchObject({ id: "A1", to: "owner", resume: true });
  });
  it("接手还活着 ⇒ 照旧［退回］、不带 `resume`", async () => {
    const { action, note } = await sendReturn(true);
    expect(action).toBe("plan.return.send");
    expect(note?.hidden).toBe(true);
    expect(calls[0].body).not.toHaveProperty("resume");
  });
});

describe("设置里 pb 路径那一行", () => {
  const change = async (row: HTMLElement, v: string): Promise<void> => {
    const input = row.querySelector("input")!;
    input.value = v;
    input.dispatchEvent(new Event("change"));
    await flush();
    await flush();
  };
  it("那台说不能用 ⇒ 不存、行下是那台那一句", async () => {
    const r = pbDirRow("<local>");
    r.load();
    await flush();
    checked = { state: "no_entry", said: "下面没有" };
    await change(r.row, "/opt/other");
    expect(typed.map((t) => t.op)).toEqual(["prefs-read", "plan-pb-check"]);
    expect(r.row.textContent).toContain("下面没有");
  });
  it("能用 ⇒ 存那一格；清空 ⇒ 交空串、不问能不能用", async () => {
    const r = pbDirRow("<local>");
    r.load();
    await flush();
    checked = { state: "ok", said: "能用" };
    await change(r.row, " /opt/pb2 ");
    expect(typed.slice(1)).toEqual([
      { op: "plan-pb-check", args: { dir: "/opt/pb2" } },
      { op: "prefs-set", args: { pbDir: "/opt/pb2" } },
    ]);
    await change(r.row, "");
    expect(typed.slice(3)).toEqual([{ op: "prefs-set", args: { pbDir: "" } }]);
  });
  it("带锚，计划页「未装」那一屏跳得过来", () => {
    expect(pbDirRow("<local>").row.dataset.anchor).toBe("pb-dir");
  });
});
