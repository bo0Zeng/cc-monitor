/**
 * 命令表里有生成类型的帧命令，界面一律经 `callTyped` 问（入参 · 回包按生成类型用，形状由握手指纹管）；
 * 不许再经裸的 `chan.call` 问、旁边配一份手写形状与手写解码。
 * 两侧异源：一侧是生成的命令表（`generated/TypedCommands.ts`，Rust 出的），一侧是生产 TS 里 `chan.call(` 调用点的操作名。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../../test-support/repo-root";
import { productionTsFiles } from "../../../test-support/production-sources";
import { stripComments } from "../../../test-support/strip-comments";

/** 一段（已剥注释的）TS 里 `chan.call(` 第二个实参是字面量的那几处操作名；`callTyped` 自己里面那一处（操作名是参数）不算。 */
function chanCallOps(code: string): string[] {
  const out: string[] = [];
  for (const m of code.matchAll(/(?<![\w$])chan\.call\(\s*[^,]+,\s*"([^"]+)"/g)) out.push(m[1]);
  return out;
}

const typed = new Set(
  [...readFileSync(resolve(REPO_ROOT, "src/frontend/ui/generated/TypedCommands.ts"), "utf8").matchAll(/^ {2}"([a-z0-9-]+)": \{ req:/gm)].map((m) => m[1]),
);

describe("有生成类型的命令经 callTyped 问", () => {
  it("★ 生产 TS 里经裸 chan.call 问的操作名 ∩ 命令表 == ∅", () => {
    const hits: string[] = [];
    for (const f of productionTsFiles("src")) {
      for (const op of chanCallOps(stripComments(f.text, "ts"))) if (typed.has(op)) hits.push(`${f.file}: ${op}`);
    }
    expect(hits).toEqual([]);
  });

  it("正控：认得出字面量操作名、不把 mychan.call 当成它；命令表读得出这几条", () => {
    expect(chanCallOps(`await chan.call(origin, "accounts-list", body, b);\nmychan.call(o, "x", p);\nchan.call(origin, cmd, body, budget);`)).toEqual(["accounts-list"]);
    for (const op of ["accounts-list", "plan-read", "terminal-follow-ack", "machine-interrupts"]) expect(typed.has(op), op).toBe(true);
  });
});
