/**
 * `quota-changed` 那条流在界面这一侧：一批格 ⇒ 额度账要不要重读、哪几个会话要重问；两侧同一个串。期望手写。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { QUOTA_CHANGED_KIND, quotaChangedItems } from "../../../src/frontend/ui/quota-stream.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

describe("quota-changed 流", () => {
  it("额度账一格 ⇒ quota；会话一格 ⇒ 那个 sid；规则表一格 ⇒ rules；读不出 / seen / gap ⇒ 整台重问", () => {
    expect(quotaChangedItems([{ t: "frame", seq: 1, body: '{"quota":true}' }])).toEqual({ quota: true, sids: [], all: false, rules: false, frames: 1 });
    expect(
      quotaChangedItems([
        { t: "frame", seq: 1, body: '{"sid":"s1"}' },
        { t: "frame", seq: 2, body: '{"sid":"s1"}' },
      ]),
    ).toEqual({ quota: false, sids: ["s1"], all: false, rules: false, frames: 2 });
    expect(quotaChangedItems([{ t: "frame", seq: 1, body: '{"rules":true}' }]), "规则表变了 ⇒ 只重读规则表").toEqual({ quota: false, sids: [], all: false, rules: true, frames: 1 });
    expect(quotaChangedItems([{ t: "frame", seq: 1, body: "not json" }])).toMatchObject({ quota: true, all: true, frames: 1 });
    expect(quotaChangedItems([{ t: "seen", from: null }])).toMatchObject({ quota: true, all: true, rules: true, frames: 0 });
  });

  it("两侧同一个串：Rust `event_replay.rs::QUOTA_CHANGED_KIND` == TS", () => {
    const rs = readFileSync(resolve(REPO_ROOT, "src/frontend/shell/src/event_replay.rs"), "utf8");
    expect(rs).toContain(`pub const QUOTA_CHANGED_KIND: &str = "${QUOTA_CHANGED_KIND}";`);
  });
});
