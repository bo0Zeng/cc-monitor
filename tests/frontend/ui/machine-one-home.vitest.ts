/**
 * 「本机」那个线上值 `"<local>"` 全仓只住一处：`cmd_wire::machine::LOCAL`（`src/common/cmd-wire/src/machine.rs`）。
 * 界面经生成物 `generated/judgment-rules.ts` 的 `LOCAL` 拿到它，手机经生成的 `gen.LOCAL`，壳与后端经 `cmd_wire::LOCAL`；
 * 别处再写一遍这个字面量，就是第二个源头：一边改了另一边不跟，`tsc` 与 `rustc` 都不会说。
 *
 * ## 钉什么
 *
 * 1. 剥了注释的生产代码（`src/` 下的 Rust · TS · 手机 `src/main/` 的 Kotlin；生成物除外）里，`"<local>"` 字面量
 *    （双引号 · 单引号 · 无插值的模板串三形）只出现一次，就在 `machine.rs`。
 * 2. 反空真：`machine.rs` 剥注释后恰有 `pub const LOCAL: &str = "<local>";` 那一行（不然第 1 条可能只是人群里一处都没扫到）。
 * 3. 生成物 `generated/Origin.ts` 不在；生产 TS 里 `Origin` · `LOCAL_ORIGIN` · `originFromWire` 三个名字零命中。
 */
import { existsSync } from "node:fs";
import { resolve } from "node:path";

import { describe, it, expect } from "vitest";

import { productionKtFiles, productionRsFiles, productionTsFiles } from "../../test-support/production-sources";
import { REPO_ROOT } from "../../test-support/repo-root";
import { stripComments, type SourceLang } from "../../test-support/strip-comments";
import { nonEmpty } from "../../test-support/non-empty";

const HOME = "src/common/cmd-wire/src/machine.rs";

/** 拼出来：本文件自己不在人群里（住 tests/），这样写读者也看得出要找的是哪个值。 */
const VALUE = ["<", "local", ">"].join("");

/** 一个字面量 `VALUE` 的三形：`"…"` · `'…'` · `` `…` ``。 */
const LITERAL = new RegExp(`(["'\`])${VALUE}\\1`, "g");

interface Code {
  file: string;
  lang: SourceLang;
  code: string;
}

/** 生产代码剥过注释。Kotlin 的生成物住 `…/gen/`（`productionKtFiles` 只按 `generated/` 目录名排），这里另排。 */
function productionCode(): Code[] {
  const rs = productionRsFiles("src").map((f) => ({ file: f.file, lang: "rust" as const, code: stripComments(f.text, "rust") }));
  const ts = productionTsFiles("src").map((f) => ({ file: f.file, lang: "ts" as const, code: stripComments(f.text, "ts") }));
  const kt = productionKtFiles("src/mobile")
    .filter((f) => !f.file.includes("/gen/"))
    .map((f) => ({ file: f.file, lang: "kotlin" as const, code: stripComments(f.text, "kotlin") }));
  return [...rs, ...ts, ...kt];
}

/** 每处命中一行 `文件:行`。 */
function literalHits(files: readonly Code[]): string[] {
  const out: string[] = [];
  for (const f of files) {
    f.code.split("\n").forEach((line, i) => {
      for (const _ of line.matchAll(LITERAL)) out.push(`${f.file}:${i + 1}`);
    });
  }
  return out;
}

describe("本机那个线上值只住 cmd-wire 一处", () => {
  const files = productionCode();

  it("人群三种语言都扫到了", () => {
    for (const lang of ["rust", "ts", "kotlin"] as const) nonEmpty(files.filter((f) => f.lang === lang));
    expect(files.some((f) => f.file === HOME), `人群里没有 ${HOME}`).toBe(true);
  });

  it("★ 剥了注释的生产代码里，这个字面量只在 machine.rs 出现一次", () => {
    const hits = literalHits(files);
    expect(
      hits.filter((h) => !h.startsWith(`${HOME}:`)),
      `「本机」的线上值在 ${HOME} 以外又写了一遍：改成引 \`cmd_wire::LOCAL\`（Rust）· 生成物的 \`LOCAL\`（TS · Kotlin）`,
    ).toEqual([]);
    expect(hits, `${HOME} 里这个字面量应恰好一处`).toHaveLength(1);
  });

  it("反空真：machine.rs 恰有 `pub const LOCAL` 那一行", () => {
    const home = files.find((f) => f.file === HOME)!;
    const decl = home.code.split("\n").filter((l) => l.trim() === `pub const LOCAL: &str = "${VALUE}";`);
    expect(decl, `${HOME} 里找不到 \`pub const LOCAL: &str = "${VALUE}";\``).toHaveLength(1);
  });

  it("过渡名不在：generated/Origin.ts 没有，生产 TS 里三个旧名零命中", () => {
    expect(existsSync(resolve(REPO_ROOT, "src/frontend/ui/generated/Origin.ts")), "generated/Origin.ts 又出现了").toBe(false);
    const ts = files.filter((f) => f.lang === "ts");
    for (const name of [["Ori", "gin"], ["LOCAL_", "ORIGIN"], ["origin", "FromWire"]].map((p) => p.join(""))) {
      const re = new RegExp(`\\b${name}\\b`);
      const hits = ts.filter((f) => re.test(f.code)).map((f) => f.file);
      expect(hits, `生产 TS 里还有旧名 \`${name}\``).toEqual([]);
    }
  });
});
