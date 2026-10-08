/**
 * 远端健康提示的标题按壳给的类别挑（`stream_source/version.rs::version_health_kind` 判好要更新 · 较新 · 不可比），
 * 界面不自己猜哪一种；壳那边三个类别字每个都在这里有自己那一句，不落到通用「远端提示」。
 */
import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { headlineFor } from "../../../src/frontend/ui/remote-health";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { REPO_ROOT } from "../../test-support/repo-root";

const src = readFileSync(resolve(REPO_ROOT, "src/frontend/shell/src/stream_source/version.rs"), "utf8");
const kind = (name: string): string => {
  const m = new RegExp(`const ${name}: &str = "([^"]+)";`).exec(src);
  if (!m) throw new Error(`version.rs 里没有 ${name}`);
  return m[1];
};

describe("远端健康提示：版本那条的标题", () => {
  it("要更新 · 较新 · 不可比 各说各的，都不落到通用标题", () => {
    expect(headlineFor(kind("VERSION_KIND_OLDER"))).toBe(copyText("remoteHealth.head.backendOld"));
    expect(headlineFor(kind("VERSION_KIND_NEWER"))).toBe(copyText("remoteHealth.head.backendNewer"));
    expect(headlineFor(kind("VERSION_KIND_INCOMPARABLE"))).toBe(copyText("remoteHealth.head.versionIncomparable"));
    expect(headlineFor("version"), "旧的总称类别不再有").toBe(copyText("remoteHealth.head.notice"));
  });
});
