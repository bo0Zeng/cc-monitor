/**
 * 要求：cc-bus 那条钩子装了没有由那台后端读它自己那份配置判（`hooks-diag`），界面不读、不写那份文件；
 * 成品的形状从后端类型生成（金样 `tests/__fixtures__/hooks-diag.golden.json` 钉线上形）；各态那一句与色调由后端写（`tests/backend/assets/cc_bus_hooks_tests.rs`），界面照抄。
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../test-support/repo-root";

describe("`hooks-diag` 读口", () => {
  it("读口只问 `hooks-diag` 那一条（只读），源码里没有任何别的通道调用与 Tauri 命令", () => {
    const code = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/cc-bus-hooks-reads.ts"), "utf8");
    const ops = [...code.matchAll(/(?:chan\.call|callTyped)\([^,]+,\s*"([a-z-]+)"/g)].map((m) => m[1]);
    expect(ops).toEqual(["hooks-diag"]);
    expect(code).not.toMatch(/\binvoke\s*\(|\bcommands\./);
  });
});
