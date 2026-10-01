/**
 * 要求：cc-bus 那两条钩子装了没有由那台后端读它自己那份配置判（`hooks-diag`），界面不读、不写那份文件；
 * 成品按形状严格收（金样 `tests/__fixtures__/hooks-diag.golden.json`，后端产出同一份），四态不误说。
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { decodeHooksReport, describeState } from "../../../src/frontend/ui/cc-bus-hooks-reads";
import golden from "../../__fixtures__/hooks-diag.golden.json";
import { REPO_ROOT } from "../../test-support/repo-root";

describe("钩子状态：四态不误说", () => {
  it("显式路径且在 ⇒ 已装（不能说成有问题）；路径不在 ⇒ 绝不说已装；包装写法 ⇒ 中性", () => {
    const at = describeState({ kind: "installed-at-path", command: "x", path: "$HOME/.claude/skills/cc-bus/scripts/cc-register" });
    expect([at.tone, at.text.includes("已装")]).toEqual(["ok", true]);
    const gone = describeState({ kind: "path-missing", command: "x", path: "/gone/cc-register" });
    expect(gone.tone).toBe("bad");
    expect(gone.text).not.toMatch(/^已装/);
    const wrapped = describeState({ kind: "unknown", command: 'sh -c "cc-register"' });
    expect(wrapped.tone).toBe("unknown");
    expect(wrapped.text).not.toContain("未装");
    expect(describeState({ kind: "not-installed" }).tone).toBe("bad");
    expect(describeState({ kind: "installed-via-path", command: "cc-register" }).tone).toBe("ok");
  });
});

describe("`hooks-diag` 成品按形状严格收", () => {
  it("金样原样收下；多一格 / 缺一格 / 认不出的态 / 要加的内容不是串 ⇒ 抛「对不上」；没装 cc-bus ⇒ 要加的内容是 null", () => {
    expect(decodeHooksReport(golden)).toEqual(golden);
    type Obj = Record<string, unknown>;
    const at = (o: Obj, k: string): Obj => o[k] as Obj;
    const bad = (mut: (g: Obj) => void) => {
      const g = JSON.parse(JSON.stringify(golden)) as Obj;
      mut(g);
      return () => decodeHooksReport(g);
    };
    expect(bad((g) => (g.extra = 1))).toThrow(/shape mismatch/);
    expect(bad((g) => delete g.source)).toThrow(/shape mismatch/);
    expect(bad((g) => (at(g, "diagnosis").stop = { kind: "sixth-state" }))).toThrow(/shape mismatch/);
    expect(bad((g) => (at(at(g, "diagnosis"), "session_start").path = 1))).toThrow(/shape mismatch/);
    expect(bad((g) => (g.snippet = 0))).toThrow(/shape mismatch/);
    expect(bad((g) => (g.supported = "yes"))).toThrow(/shape mismatch/);
    expect(decodeHooksReport({ ...golden, snippet: null }).snippet).toBeNull();
  });

  it("读口只问 `hooks-diag` 那一条（只读），源码里没有任何别的通道调用与 Tauri 命令", () => {
    const code = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/cc-bus-hooks-reads.ts"), "utf8");
    const ops = [...code.matchAll(/chan\.call\([^,]+,\s*"([a-z-]+)"/g)].map((m) => m[1]);
    expect(ops).toEqual(["hooks-diag"]);
    expect(code).not.toMatch(/\binvoke\s*\(|\bcommands\./);
  });
});
