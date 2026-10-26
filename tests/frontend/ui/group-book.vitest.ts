/**
 * 组的账（`group-book.ts`）：读只问本机 `groups-read`（带上要问的那几台）；组跟着组员走，没有「家」。
 *
 * | # | 判据 |
 * |---|---|
 * | B1 | ★ 读：本机那一问带着记下的每一台；读回来交给订的人；读失败 ⇒ 留着上一份 |
 * | B2 | 写：建组只向本机要 id（回整份记录）· 进出组问组员那台 `groups-write move`（带那一组的记录）· 改名 · 解散问本机并带上各片 · 固定问会话那台的注解 |
 * | B3 | 那台推 `changed {groups}` 连着来 ⇒ 合成一问 |
 * | B4 | 改名 · 解散有没送到的那几台 ⇒ 抛出、说出哪几台与为什么 |
 */
import { describe, it, expect, vi } from "vitest";
import { GroupBook, type GroupCall } from "../../../src/frontend/ui/group-book";
import type { GroupsRead } from "../../../src/frontend/ui/generated/GroupsRead";

const READ: GroupsRead = {
  groups: [{ id: "g1", name: "开发", title: "开发", made: 1, named: 1, known: { is: "now" } as never, members: [], counts: [] }],
  pinned: [],
  machines: [],
};

const G9 = { id: "g9", name: "新", made: 2, named: 2 };

function book(answer: (cmd: string, req: unknown) => unknown): { b: GroupBook; calls: Array<[string, string, unknown]> } {
  const calls: Array<[string, string, unknown]> = [];
  const call: GroupCall = async (origin, cmd, req) => {
    calls.push([origin, cmd, req]);
    const r = answer(cmd, req);
    if (r instanceof Error) throw r;
    return r as never;
  };
  return { b: new GroupBook(call, 5), calls };
}

function writes(answerOp: (op: string) => unknown): ReturnType<typeof book> {
  return book((cmd, req) => {
    if (cmd === "groups-read") return READ;
    if (cmd === "history-annotate") return { entry: {}, prev: null };
    return answerOp((req as { op: string }).op);
  });
}

const sent = (missed: Array<{ origin: string; why: string }> = []) => ({ is: "sent", id: "g1", missed });

describe("组的账", () => {
  it("B1 读：本机一问带着每一台；读失败留着上一份", async () => {
    let fail = false;
    const { b, calls } = book(() => (fail ? new Error("本机后端没答") : READ));
    const seen: Array<GroupsRead | null> = [];
    b.subscribe(() => seen.push(b.read()));
    await b.refreshAll(["<local>", "devbox"]);
    expect(calls).toEqual([["<local>", "groups-read", { machines: ["<local>", "devbox"] }]]);
    expect(b.read()).toEqual(READ);
    expect(seen).toEqual([READ]);
    fail = true;
    await b.refreshAll([]);
    expect(b.read()).toEqual(READ);
  });

  it("B2 写：建组只要 id、进出组问组员那台、改名解散问本机带各片、固定问会话那台", async () => {
    const { b, calls } = writes((op) =>
      op === "make" ? { is: "made", ...G9 } : op === "move" ? { is: "moved", sid: "s1", groups: ["g9"] } : sent(),
    );
    expect(await b.make("新")).toEqual(G9);
    await b.move("devbox", "s1", G9, null);
    await b.move("<local>", "s2", null, "g1");
    await b.rename("g1", "改", ["<local>", "devbox"]);
    await b.dissolve("g1", ["devbox"]);
    await b.pin("<local>", "s2", true);
    expect(calls.filter((c) => c[1] !== "groups-read")).toEqual([
      ["<local>", "groups-write", { op: "make", name: "新" }],
      ["devbox", "groups-write", { op: "move", sid: "s1", join: G9 }],
      ["<local>", "groups-write", { op: "move", sid: "s2", leave: "g1" }],
      ["<local>", "groups-write", { op: "rename", id: "g1", name: "改", machines: ["<local>", "devbox"] }],
      ["<local>", "groups-write", { op: "dissolve", id: "g1", machines: ["devbox"] }],
      ["<local>", "history-annotate", { sid: "s2", patch: { pinned: true } }],
    ]);
  });

  it("B3 连着来的推送合成一问", async () => {
    vi.useFakeTimers();
    try {
      const { b, calls } = book(() => READ);
      await b.refreshAll(["<local>"]);
      calls.length = 0;
      b.poke("devbox");
      b.poke("devbox");
      b.poke("<local>");
      await vi.advanceTimersByTimeAsync(10);
      expect(calls).toEqual([["<local>", "groups-read", { machines: ["<local>", "devbox"] }]]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("B4 改名 · 解散有没送到的 ⇒ 抛出、说出哪几台", async () => {
    const { b } = writes(() => sent([{ origin: "laptop", why: "absent" }]));
    await expect(b.rename("g1", "改", ["<local>", "laptop"])).rejects.toThrow(/laptop.*absent/);
    await expect(b.dissolve("g1", ["laptop"])).rejects.toThrow(/laptop/);
  });
});
