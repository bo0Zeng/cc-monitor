/**
 * base64 全仓 Rust 只住一处：`cmd_wire::b64`（`src/common/cmd-wire/src/b64.rs`）。
 * 后端 CLI 入参 · 出网口的代理鉴权 · 壳给 PowerShell 的编码命令都调它；再手写一份，就是第二个字母表、第二套补位规则。
 *
 * ## 钉什么
 *
 * 1. 剥了注释的生产 Rust（`src/` 下）里，base64 字母表那一串字面恰好一处，在 `b64.rs`。
 * 2. 先前那几份的函数名（`fn base64(` · `fn base64_encode(`）在生产 Rust 里零处。
 * 3. 反空真：人群非空，`b64.rs` 在人群里且有 `pub fn b64_encode` · `pub fn b64_decode`。
 */
import { describe, expect, it } from "vitest";

import { productionRsFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";
import { nonEmpty } from "../../test-support/non-empty";

const HOME = "src/common/cmd-wire/src/b64.rs";

/** 拼出来：本文件自己不在人群里（住 tests/），这样写读者也看得出要找的是哪一串。 */
const ALPHABET = ["ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz", "0123456789+/"].join("");

describe("base64 只住 cmd-wire 一处", () => {
  const files = productionRsFiles("src").map((f) => ({ file: f.file, code: stripComments(f.text, "rust") }));

  it("反空真：人群非空，家在人群里且两个口都在", () => {
    nonEmpty(files);
    const home = files.find((f) => f.file === HOME);
    expect(home, `人群里没有 ${HOME}`).toBeDefined();
    for (const decl of ["pub fn b64_encode(", "pub fn b64_decode("]) expect(home!.code, `${HOME} 里没有 ${decl}`).toContain(decl);
  });

  it("★ 字母表字面恰好一处，在 b64.rs", () => {
    const hits = files.flatMap((f) =>
      f.code.split("\n").flatMap((line, i) => (line.includes(ALPHABET) ? [`${f.file}:${i + 1}`] : [])),
    );
    expect(
      hits.filter((h) => !h.startsWith(`${HOME}:`)),
      `base64 字母表在 ${HOME} 以外又写了一遍：改调 \`cmd_wire::b64\``,
    ).toEqual([]);
    expect(hits, `${HOME} 里字母表应恰好一处`).toHaveLength(1);
  });

  it("手写那几份的函数名零处", () => {
    const hits = files.filter((f) => /\bfn base64(_encode)?\s*\(/.test(f.code)).map((f) => f.file);
    expect(hits, "生产 Rust 里又长出一个手写的 base64").toEqual([]);
  });
});
