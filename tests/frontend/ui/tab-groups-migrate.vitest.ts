/**
 * 桌面旧的那三格（`tabCollections` · `tabBar.groupOf` · `tabBar.pinned`）迁进各台一次（`tab-groups-migrate.ts`）。
 *
 * | # | 判据 |
 * |---|---|
 * | M1 | ★ 一组「开发」、两个标签指着它（一台本机一台远端）⇒ 向本机要一组「开发」、两台各一条进组（带那一组的记录），全成了才删那三格 |
 * | M2 | 有一条没写成 ⇒ 那三格原样留着（下次再迁），不说「迁完」 |
 * | M3 | 固定那几条照记录里的机器写 `pinned`；指着不存在的组 · 认不出机器的那几条丢掉、交回丢了哪几条 |
 * | M4 | 收着的组迁过去照新 id 记成收着；盘上没有那三格 ⇒ 一条都不发 |
 */
import { describe, it, expect } from "vitest";
import { migrateTabGroups, oldTabGroups, type MigrateIo } from "../../../src/frontend/ui/tab-groups-migrate";

interface Sent {
  made: string[];
  patched: Array<{ origin: string; sid: string; patch: Record<string, unknown> }>;
  dropped: number;
}

function io(fail: (what: string) => boolean = () => false): { io: MigrateIo; sent: Sent } {
  const sent: Sent = { made: [], patched: [], dropped: 0 };
  let n = 0;
  return {
    sent,
    io: {
      make: async (name) => {
        if (fail(`make:${name}`)) throw new Error("写不下去");
        sent.made.push(name);
        n += 1;
        return { id: `g-new-${n}`, name, made: n, named: n };
      },
      join: async (origin, sid, group) => {
        if (fail(`annotate:${sid}`)) throw new Error("那台没答");
        sent.patched.push({ origin, sid, patch: { join: group.id, name: group.name } });
      },
      pin: async (origin, sid) => {
        if (fail(`annotate:${sid}`)) throw new Error("那台没答");
        sent.patched.push({ origin, sid, patch: { pinned: true } });
      },
      dropOld: async () => {
        sent.dropped += 1;
      },
    },
  };
}

const CFG = {
  tabCollections: [{ id: "c1", name: "开发", collapsed: true }],
  tabBar: {
    order: ["s-local", "s-remote"],
    groupOf: { "s-local": "c1", "s-remote": "c1", "s-gone": "c-missing" },
    pinned: [{ sid: "s-pin", origin: "devbox", jsonlPath: "/r/s-pin.jsonl", title: "x" }],
  },
};

const ORIGINS: Record<string, string> = { "s-local": "<local>", "s-remote": "devbox" };

describe("旧的分组三格迁进各台", () => {
  it("M1 一组两个组员：向本机要组、两台各一条进组、全成了才删那三格", async () => {
    const { io: x, sent } = io();
    const got = await migrateTabGroups(oldTabGroups(CFG), new Map(Object.entries(ORIGINS)), x);
    expect(got.done).toBe(true);
    expect(sent.made).toEqual(["开发"]);
    expect(sent.patched).toEqual(
      expect.arrayContaining([
        { origin: "<local>", sid: "s-local", patch: { join: "g-new-1", name: "开发" } },
        { origin: "devbox", sid: "s-remote", patch: { join: "g-new-1", name: "开发" } },
      ]),
    );
    expect(sent.dropped).toBe(1);
  });

  it("M2 有一条没写成：三格原样留着，不说迁完", async () => {
    const { io: x, sent } = io((w) => w === "annotate:s-remote");
    const got = await migrateTabGroups(oldTabGroups(CFG), new Map(Object.entries(ORIGINS)), x);
    expect(got.done).toBe(false);
    expect(sent.dropped).toBe(0);
  });

  it("M3 固定照记录里的机器写；指着没有的组、认不出机器的丢掉并交回", async () => {
    const { io: x, sent } = io();
    const cfg = { ...CFG, tabBar: { ...CFG.tabBar, groupOf: { ...CFG.tabBar.groupOf, "s-nowhere": "c1" } } };
    const got = await migrateTabGroups(oldTabGroups(cfg), new Map(Object.entries(ORIGINS)), x);
    expect(sent.patched).toContainEqual({ origin: "devbox", sid: "s-pin", patch: { pinned: true } });
    expect([...got.lost].sort()).toEqual(["s-gone", "s-nowhere"]);
    expect(got.done).toBe(true);
  });

  it("M4 收着的组照新 id 记成收着；盘上没有那三格 ⇒ 一条都不发", async () => {
    const { io: x } = io();
    const got = await migrateTabGroups(oldTabGroups(CFG), new Map(Object.entries(ORIGINS)), x);
    expect(got.collapsed).toEqual(["g-new-1"]);
    const { io: y, sent } = io();
    const none = await migrateTabGroups(oldTabGroups({ tabBar: { order: ["a"] } }), new Map([["a", "<local>"]]), y);
    expect(none).toEqual({ done: false, lost: [], collapsed: [] });
    expect(sent).toEqual({ made: [], patched: [], dropped: 0 });
  });
});
