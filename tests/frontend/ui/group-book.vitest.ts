/**
 * 组的账（`group-book.ts`）：读只问本机 `groups-read`（带上要问的那几台），写照住处问那一台。
 *
 * | # | 判据 |
 * |---|---|
 * | B1 | ★ 读：本机那一问带着记下的每一台；读回来交给订的人；读失败 ⇒ 留着上一份 |
 * | B2 | 写：建组问本机、改名 · 解散问家那台、进出组 · 固定问会话那台的注解 |
 * | B3 | 那台推 `changed {groups}` 连着来 ⇒ 合成一问 |
 */
import { describe, it, expect, vi } from "vitest";
import { GroupBook, type GroupCall } from "../../../src/frontend/ui/group-book";
import type { GroupsRead } from "../../../src/frontend/ui/generated/GroupsRead";

const READ: GroupsRead = {
  groups: [{ id: "g1", name: "开发", title: "开发", home: "devbox", made: 1, known: { is: "now" } as never, members: [], counts: [] }],
  pinned: [],
  machines: [],
};

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

  it("B2 写照住处：建组本机、改名解散问家、进出组与固定问会话那台", async () => {
    const { b, calls } = book((cmd) =>
      cmd === "groups-write" ? { is: "made", id: "g9", name: "新", made: 2 } : cmd === "groups-read" ? READ : { entry: {}, prev: null },
    );
    expect(await b.make("新")).toBe("g9");
    await b.rename("devbox", "g1", "改");
    await b.dissolve("devbox", "g1");
    await b.member("devbox", "s1", { join: ["g1"] });
    await b.member("<local>", "s2", { pinned: true });
    expect(calls.filter((c) => c[1] !== "groups-read")).toEqual([
      ["<local>", "groups-write", { op: "make", name: "新" }],
      ["devbox", "groups-write", { op: "rename", id: "g1", name: "改" }],
      ["devbox", "groups-write", { op: "dissolve", id: "g1" }],
      ["devbox", "history-annotate", { sid: "s1", patch: { join: ["g1"] } }],
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
});
