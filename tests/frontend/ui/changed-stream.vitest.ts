/**
 * `changed/<topic>` 那一种流在界面这一侧：一批格 ⇒ 变了的那几格 / 要不要整份重问；计划那一主题读出工作区与要你看的数；
 * 主题名是后端生成的 `Topic`。期望手写。
 */
import { describe, expect, it } from "vitest";

import { changedItems, changedStream, planMoves, pushedProducts } from "../../../src/frontend/ui/changed-stream.ts";

const f = (seq: number, body: string) => ({ t: "frame" as const, seq, body });

describe("changed 流", () => {
  it("一格一个 key（同一个留后一格）；不带 key 的一格 key 为 null；读不懂 / seen / gap ⇒ 整份重问；unseen / closed 不算", () => {
    expect(changedItems([f(0, '{"key":"s1"}'), f(1, '{"key":"s2","rev":"r"}'), f(2, '{"key":"s1","body":{"a":1}}')])).toEqual({
      cells: [
        { key: "s2", rev: "r", body: undefined },
        { key: "s1", rev: null, body: { a: 1 } },
      ],
      all: false,
      frames: 3,
    });
    expect(changedItems([f(0, "{}")])).toEqual({ cells: [{ key: null, rev: null, body: undefined }], all: false, frames: 1 });
    for (const bad of ["not json", "[]", '{"key":1}', '{"rev":false}']) expect(changedItems([f(0, bad)]), bad).toMatchObject({ cells: [], all: true, frames: 1 });
    expect(changedItems([{ t: "seen", from: null }])).toEqual({ cells: [], all: true, frames: 0 });
    expect(changedItems([{ t: "gap", fromSeq: 1, toSeq: 3 }])).toEqual({ cells: [], all: true, frames: 0 });
    expect(changedItems([{ t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" }])).toEqual({ cells: [], all: false, frames: 0 });
  });

  it("计划：key ＝ 工作区、rev ＝ 摘要、body.needs ＝ 要你看的数（没给 ⇒ null）；缺工作区或摘要的不猜", () => {
    const c = changedItems([f(0, '{"key":"/w","rev":"r1","body":{"needs":2}}'), f(1, '{"key":"/v","rev":"r2"}'), f(2, '{"rev":"r3"}')]);
    expect(planMoves(c)).toEqual([
      { workspace: "/w", rev: "r1", needs: 2 },
      { workspace: "/v", rev: "r2", needs: null },
    ]);
  });

  it("帧里带了成品的进 got（按主题的回包类型）、没带的 key 进 ask；all 照传", () => {
    const list = { tasks: [] };
    const p = pushedProducts({ cells: [{ key: "a", rev: null, body: list }, { key: "b", rev: null, body: undefined }], all: false }, "tasks");
    expect([...p.got]).toEqual([["a", list]]);
    expect(p.ask).toEqual(["b"]);
    expect(pushedProducts({ cells: [], all: true }, "quota").all).toBe(true);
  });

  it("每个带体的主题都经这一处读：机器状态 · 会话隐藏 · 计划交那一臂的体，没带的进 ask", () => {
    const hidden = { hidden: null };
    const h = pushedProducts({ cells: [{ key: "s1", rev: null, body: hidden }, { key: "s2", rev: null, body: undefined }], all: false }, "session_hidden");
    expect([...h.got]).toEqual([["s1", hidden]]);
    expect(h.ask).toEqual(["s2"]);
    const m = pushedProducts({ cells: [{ key: "box", rev: null, body: undefined }], all: false }, "machine");
    expect([...m.got]).toEqual([]);
    expect(m.ask).toEqual(["box"]);
    const p = pushedProducts({ cells: [{ key: "/w", rev: "r", body: { needs: 4 } }], all: false }, "plan");
    expect(p.got.get("/w")?.needs).toBe(4);
  });

  it("流名 changed/<topic>", () => {
    expect(changedStream("rotation_strategies")).toBe("changed/rotation_strategies");
  });
});
