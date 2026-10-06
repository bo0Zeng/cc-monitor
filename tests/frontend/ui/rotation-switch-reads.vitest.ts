/**
 * `rotation-switch` 重启那一支的严格解码：后端 `rotation_face_tests.rs::restart_outcomes_match_the_cross_language_golden`
 * 产出的那一份（`tests/__fixtures__/rotation-switch-restart.golden.json`）逐格收下；多一格 / 缺一格 / 值不认识 ⇒ 抛。
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { decodeRestartOutcomes } from "../../../src/frontend/ui/quota-reads";

const golden = JSON.parse(readFileSync(join(__dirname, "../../__fixtures__", "rotation-switch-restart.golden.json"), "utf8")) as {
  sessions: Record<string, Record<string, unknown>>;
};

describe("重启切换那一格的严格解码（跨语言金样）", () => {
  it("后端产出的那几格逐格收下", () => {
    expect(Object.keys(golden.sessions).length).toBeGreaterThan(0);
    expect(decodeRestartOutcomes(golden)).toEqual(golden.sessions);
  });

  it("多一格 / 缺一格 / 值不认识 ⇒ 抛", () => {
    const one = (x: Record<string, unknown>) => () => decodeRestartOutcomes({ sessions: { s1: x } });
    expect(one({ ...golden.sessions.arrived, extra: 1 })).toThrow();
    expect(one({ state: "done" })).toThrow();
    expect(one({ ...golden.sessions.start_failed, old: "maybe" })).toThrow();
    expect(one({ state: "failed", code: "start_failed" })).toThrow();
    expect(one({ state: "skipped", code: "noRelay" })).toThrow();
  });
});
