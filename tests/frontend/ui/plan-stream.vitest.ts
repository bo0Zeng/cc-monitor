/**
 * `plan-changed` 那条流在界面这一侧：一批格 ⇒ 哪几个工作区变了（同一个留最后一格）、要不要整台重问；两侧同一个串。期望手写。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { PLAN_CHANGED_KIND, planChangedItems } from "../../../src/frontend/ui/plan-stream.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

describe("plan-changed 流", () => {
  it("一格 ⇒ 那个工作区与新摘要；同一个工作区两格留后一格；读不出 / seen / gap ⇒ 整台重问", () => {
    expect(planChangedItems([{ t: "frame", seq: 1, body: '{"workspace":"/w","rev":"r1","needs":2}' }])).toEqual({
      moved: [{ workspace: "/w", rev: "r1", needs: 2 }],
      all: false,
      frames: 1,
    });
    expect(
      planChangedItems([
        { t: "frame", seq: 1, body: '{"workspace":"/w","rev":"r1","needs":null}' },
        { t: "frame", seq: 2, body: '{"workspace":"/w","rev":"r2"}' },
      ]),
    ).toEqual({ moved: [{ workspace: "/w", rev: "r2", needs: null }], all: false, frames: 2 });
    expect(planChangedItems([{ t: "frame", seq: 1, body: "not json" }])).toEqual({ moved: [], all: true, frames: 1 });
    expect(planChangedItems([{ t: "frame", seq: 1, body: '{"rev":"r1"}' }]), "缺工作区 ⇒ 不猜").toEqual({ moved: [], all: true, frames: 1 });
    expect(planChangedItems([{ t: "gap", fromSeq: 1, toSeq: 2 }])).toMatchObject({ all: true, frames: 0 });
  });

  it("两侧同一个串：Rust `event_replay.rs::PLAN_CHANGED_KIND` == TS", () => {
    const rs = readFileSync(resolve(REPO_ROOT, "src/frontend/shell/src/event_replay.rs"), "utf8");
    expect(rs).toContain(`pub const PLAN_CHANGED_KIND: &str = "${PLAN_CHANGED_KIND}";`);
  });
});
