/**
 * xterm 键码全仓只一张表：`cmd_wire::keys`（`src/common/cmd-wire/src/keys.rs`）。
 * 后端托管终端送键、手机键条送输入 · 软键盘带修饰 · 旧转义按钮迁移（经 JNI 调手机那份核心）都从它出；
 * 再在 Kotlin 里拼一遍 `ESC [ …`，就是第二张表：一边加了键或改了修饰参数，另一边不跟，`kotlinc` 与 `rustc` 都不会说。
 *
 * ## 钉什么
 *
 * 1. 剥了注释的手机生产 Kotlin（各模块 `src/main/`，生成物除外）里 ESC 字面（`\u001b` · `0x1B`，大小写不论）零处。
 * 2. 剥了注释的生产 Rust（`src/` 下）里整串就是一颗键的字节串字面（`b"\x1b[A"` 那一族 · `b"\x1b[Z"` · `b"\x1b[5~"` · `b"\x1bOP"`）
 *    只剩 [`OPEN_RS`] 登记的那几处（屏幕控制序列 · 括号粘贴不是键，整串不止一颗键的不在此列）。
 * 3. 反空真：两种人群都非空；`keys.rs` 在人群里且有 `const KEYS` 那张表与编码 · 解码两个口。
 */
import { describe, expect, it } from "vitest";

import { productionKtFiles, productionRsFiles } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";
import { nonEmpty } from "../../test-support/non-empty";

const HOME = "src/common/cmd-wire/src/keys.rs";

/**
 * 还没收的：文件 ⇒ 命中行数（恒等计数，收掉一处就得改小，收完删那一格）。
 * - 瘦客户端的键表（`host/managed/keys.rs`）把外层终端送来的字节认成「前缀之后那一键」· 翻看里的上下翻页：
 *   该改调 `cmd_wire::keys::presses_of` 再照 `Press` 认；那一层这一阵在拆 crate 第 5 层手里，并回之后收。
 */
const OPEN_RS: Record<string, number> = { "src/backend/host/managed/keys.rs": 5 };

/** 一份剥过注释的生产文件里，命中 `re` 的每一行 `文件:行`。 */
function hits(files: readonly { file: string; code: string }[], re: RegExp): string[] {
  return files.flatMap((f) => f.code.split("\n").flatMap((line, i) => (re.test(line) ? [`${f.file}:${i + 1}`] : [])));
}

describe("xterm 键码只住 cmd-wire 一张表", () => {
  const kt = productionKtFiles("src/mobile")
    .filter((f) => !f.file.includes("/gen/"))
    .map((f) => ({ file: f.file, code: stripComments(f.text, "kotlin") }));
  const rs = productionRsFiles("src").map((f) => ({ file: f.file, code: stripComments(f.text, "rust") }));

  it("反空真：两种人群都非空，表与两个口在 keys.rs", () => {
    nonEmpty(kt);
    nonEmpty(rs);
    const home = rs.find((f) => f.file === HOME);
    expect(home, `人群里没有 ${HOME}`).toBeDefined();
    for (const decl of ["const KEYS:", "pub fn presses_bytes(", "pub fn presses_of("]) {
      expect(home!.code, `${HOME} 里没有 ${decl}`).toContain(decl);
    }
  });

  it("★ 手机生产 Kotlin 里 ESC 字面零处（送进终端的字节经 JNI 由核心算）", () => {
    expect(hits(kt, /\\u001b|0x1b\b/i), "Kotlin 里又在拼 xterm 序列：改调 `TermKeys`（core-rs）").toEqual([]);
  });

  it("★ 生产 Rust 里整串就是一颗键的字节串字面，只剩登记过的那几处（字节由 keys.rs 那张表出）", () => {
    const perFile = new Map<string, number>();
    for (const h of hits(rs, /b"\\x1b(\[[A-DFHZ]|\[[0-9]{1,2}~|O[A-DFHP-S])"/)) {
      const file = h.slice(0, h.lastIndexOf(":"));
      perFile.set(file, (perFile.get(file) ?? 0) + 1);
    }
    expect(Object.fromEntries(perFile), "Rust 里又写了一遍键序列：改调 `cmd_wire::keys`（还没收的那几处行数变了 ⇒ 改这里的登记）").toEqual(OPEN_RS);
  });
});
