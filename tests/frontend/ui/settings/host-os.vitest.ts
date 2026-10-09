/**
 * 本机能力（界面这一半）：只读壳注入的 `window.__CCM_HOST__`，不按 User-Agent 猜；读不到 / 形状不对 ⇒ 「认不出」那一份，
 * 失败方向往显示倒（↗ 照常显示、方言不猜）。
 */
import { describe, it, expect, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { hostFacts, hostOs, readHostFacts, UNKNOWN_HOST, __setHostFactsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { factsOn } from "../../../test-support/host-facts";
import { REPO_ROOT } from "../../../test-support/repo-root";

afterEach(() => __setHostFactsForTests(null));

describe("readHostFacts", () => {
  it("★ 壳给的三行（跨语言金样）原样读出", () => {
    for (const os of ["windows", "macos", "linux"] as const) {
      const f = factsOn(os);
      expect(readHostFacts(JSON.parse(JSON.stringify(f)))).toEqual(f);
      expect(f.os).toBe(os);
    }
  });

  it("★ 读不到 / 形状不对 ⇒ 认不出那一份：↗ 照常显示、方言不猜", () => {
    for (const v of [undefined, null, "linux", {}, { os: "linux" }, { os: "linux", terminalFront: "yes", ccmPathCache: true }]) {
      expect(readHostFacts(v), JSON.stringify(v)).toEqual(UNKNOWN_HOST);
    }
    expect(UNKNOWN_HOST).toMatchObject({ os: "unknown", terminalFront: true, shellDialect: null });
    // 系统 / 方言认不出的那一格单独落「认不出」，别的格照收。
    expect(readHostFacts({ os: "plan9", terminalFront: false, shellDialect: "fish", ccmPathCache: false })).toEqual({
      os: "unknown",
      terminalFront: false,
      shellDialect: null,
      ccmPathCache: false,
    });
  });

  it("页里读的就是壳注入的那一格（测试页缺省注入 Linux 那一行）", () => {
    expect(hostFacts()).toEqual(factsOn("linux"));
    __setHostFactsForTests(factsOn("windows"));
    expect(hostOs()).toBe("windows");
  });

  it("★ 界面生产代码里零处按 User-Agent 判系统（判定只住壳 platform/host_facts.rs）", () => {
    for (const f of ["src/frontend/ui/settings/host-os.ts", "src/frontend/ui/terminal-front.ts", "src/frontend/ui/settings/machine-aliases.ts", "src/frontend/ui/settings/backend-section.ts"]) {
      expect(readFileSync(resolve(REPO_ROOT, f), "utf8"), f).not.toMatch(/userAgent/);
    }
  });
});
