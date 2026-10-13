/**
 * 要求：cc-bus 那两条钩子装了没有由那台后端读它自己那份配置判（`hooks-diag`），界面不读、不写那份文件；
 * 成品的形状从后端类型生成（金样 `tests/__fixtures__/hooks-diag.golden.json` 钉线上形），四态不误说。
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describeState } from "../../../src/frontend/ui/cc-bus-hooks-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { copyText } from "../../../src/frontend/ui/copy-table";

describe("钩子状态：四态不误说", () => {
  it("显式路径且在 ⇒ 已装（不能说成有问题）；路径不在 ⇒ 绝不说已装；包装写法 ⇒ 中性", () => {
    const at = describeState({ kind: "installed-at-path", command: "x", path: "$HOME/.claude/skills/cc-bus/scripts/cc-register" });
    expect([at.tone, at.text === copyText("ccBusHooks.state.explicit", { path: "$HOME/.claude/skills/cc-bus/scripts/cc-register" })]).toEqual(["ok", true]);
    const gone = describeState({ kind: "path-missing", command: "x", path: "/gone/cc-register" });
    expect(gone.tone).toBe("bad");
    expect(gone.text).not.toMatch(/^已装/);
    expect(gone.text, "配了、指向的东西不在 ⇒ 已失效，带那条路径").toBe(copyText("ccBusHooks.state.brokenPath", { path: "/gone/cc-register" }));
    expect(gone.text.startsWith("已失效")).toBe(true);
    expect(gone.text).toContain("/gone/cc-register");
    const wrapped = describeState({ kind: "unknown", command: 'sh -c "cc-register"' });
    expect(wrapped.tone).toBe("unknown");
    expect(wrapped.text).not.toContain(copyText("ccBusHooks.state.missing"));
    expect(describeState({ kind: "not-installed" }).tone).toBe("bad");
    expect(describeState({ kind: "installed-via-path", command: "cc-register" }).tone).toBe("ok");
  });
});

describe("`hooks-diag` 读口", () => {
  it("读口只问 `hooks-diag` 那一条（只读），源码里没有任何别的通道调用与 Tauri 命令", () => {
    const code = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/cc-bus-hooks-reads.ts"), "utf8");
    const ops = [...code.matchAll(/(?:chan\.call|callTyped)\([^,]+,\s*"([a-z-]+)"/g)].map((m) => m[1]);
    expect(ops).toEqual(["hooks-diag"]);
    expect(code).not.toMatch(/\binvoke\s*\(|\bcommands\./);
  });
});
