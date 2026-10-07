/**
 * 要求：前端按形状严格收运行表（多一格 / 缺一格 / 类型不对 ⇒ 不收，不猜）；后端真出的那两行（全格 · 最少格）读得懂。
 * 金样由后端 `wire_tests` 用真序列化器写（异源：Rust 造、TS 解）。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { decodeRunsPayload } from "../../../src/frontend/ui/runs";

import { REPO_ROOT } from "../../test-support/repo-root";

/** 金样里 `session_runs` 那两行，翻成会话流里 `runs` 那一格的形状（`sid` ⇒ `session_id`）。 */
const rows = readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/session-stream.golden.jsonl"), "utf8")
  .split("\n")
  .filter((l) => l.trim() !== "")
  .map((l) => JSON.parse(l) as Record<string, unknown>)
  .filter((f) => f.kind === "session_runs")
  .map((f) => ({ session_id: f.sid, runs: f.runs, ended: f.ended }));

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

describe("运行表严格收", () => {
  it("金样的全格与最少格两行都读得懂、原样还原", () => {
    expect(rows.length).toBe(2);
    for (const r of rows) expect(decodeRunsPayload(r)).toEqual(r);
    const full = rows[0].runs as Record<string, unknown>[];
    expect(Object.keys(full[0]).sort()).toEqual(
      ["active_ms", "error", "ended_ms", "kind", "label", "last", "parent", "run", "started_ms", "state", "tool", "waiting", "why"].sort(),
    );
  });

  it("多一格 · 缺一格 · 类型不对 · 值域外 ⇒ 不收", () => {
    const base = rows[0];
    const bad: Array<(p: Record<string, unknown>) => void> = [
      (p) => ((p.runs as Record<string, unknown>[])[0].extra = 1),
      (p) => delete (p.runs as Record<string, unknown>[])[0].state,
      (p) => delete (p.runs as Record<string, unknown>[])[0].run,
      (p) => ((p.runs as Record<string, unknown>[])[0].started_ms = "1000"),
      (p) => ((p.runs as Record<string, unknown>[])[0].active_ms = -1),
      (p) => ((p.runs as Record<string, unknown>[])[0].why = "maybe"),
      (p) => ((p.runs as Record<string, unknown>[])[0].state = "paused"),
      (p) => ((p.runs as Record<string, unknown>[])[0].parent = 3),
      (p) => ((p.runs as Record<string, unknown>[])[0].last = { t: "tool" }),
      (p) => ((p.ended as Record<string, unknown>[])[0].extra = 1),
      (p) => delete (p.ended as Record<string, unknown>[])[0].tool,
      (p) => delete p.ended,
      (p) => (p.more = []),
    ];
    for (const f of bad) {
      const p = clone(base) as Record<string, unknown>;
      f(p);
      expect(decodeRunsPayload(p), JSON.stringify(p)).toBeNull();
    }
  });
});
