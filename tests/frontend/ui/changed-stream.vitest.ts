/**
 * `changed/<topic>` 那一种流在界面这一侧：一批格 ⇒ 变了的那几格 / 要不要整份重问；计划那一主题读出工作区与要你看的数；
 * 主题名与后端生成的 `Topic` 同一份。期望手写。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { changedItems, changedStream, planMoves, pushedProducts } from "../../../src/frontend/ui/changed-stream.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

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

  it("流名 changed/<topic>；主题名就是后端生成的那一份（从 Rust 主题表抠，异源）", () => {
    expect(changedStream("rotation_strategies")).toBe("changed/rotation_strategies");
    const gen = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/generated/Topic.ts"), "utf8");
    const rs = readFileSync(resolve(REPO_ROOT, "src/backend/wire/topic.rs"), "utf8");
    const names = [...rs.matchAll(/name: "([a-z_]+)",/g)].map((m) => m[1]);
    // 10 → 11：`machines`（本机机器表变了，第 7 批）；11 → 12：`session_hidden`（某个活会话隐不隐藏变了）；12 → 13：`machine`（某台远端的状态成品）；13 → 14：`files_dir`（正被浏览的某个目录动过）。
    expect(names.length).toBe(14);
    for (const n of names) expect(gen, n).toContain(`"${n}"`);
  });
});
