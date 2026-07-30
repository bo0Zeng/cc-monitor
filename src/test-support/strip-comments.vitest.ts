/**
 * `stripComments` 自己的测试。
 *
 * ## 为什么这个 helper 需要专门的测试
 *
 * 它是**两个守卫（很快是更多）唯一的判据前处理**。C04a Phase D 审计用变异证明了：
 * 它剥错的时候，对**否定式**断言（`.not.toContain("bigint")` · `.not.toMatch(/import .*invoke/)`）
 * 就是**假绿** —— 守卫永远不叫，而这正是本工作区在治的病（静默削弱）。
 * 所以「守卫的前处理」自己必须有牙，否则整条链的根是软的。
 *
 * 下面每条都是**曾经真的会错**或**头注里声明了某个取舍**的形状，不是凑数的。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { REPO_ROOT } from "./repo-root";
import { stripComments } from "./strip-comments";

describe("stripComments", () => {
  it("剥掉行注释与块注释，且**逐行保留行结构**", () => {
    const src = ["let a = 1; // 尾注", "/* 块", " * 注释", " */", "let b = 2;"].join("\n");
    const out = stripComments(src, "ts");
    // 行数不变（旧实现把块注释连换行一起删，64 行剥成 16 行）
    expect(out.split("\n")).toHaveLength(5);
    expect(out).not.toContain("尾注");
    expect(out).not.toContain("块");
    // 代码逐字保留、还在原来那一行，且**每行长度也不变**（注释换成等量空格）
    expect(out.split("\n")[0]).toMatch(/^let a = 1; +$/);
    expect(out.split("\n")[4]).toBe("let b = 2;");
    src.split("\n").forEach((line, k) => {
      expect(out.split("\n")[k].length, `第 ${k + 1} 行长度变了`).toBe(line.length);
    });
  });

  it("**字符串字面量里的 `/*` 不启动块注释**（实测过的现存 bug：曾吞掉 521 行真代码）", () => {
    const src = ['let p = "~/.local/*/bin";', "let q = 1;", "/* 真注释 */", "let r = 2;"].join("\n");
    const out = stripComments(src, "rust");
    expect(out, "字符串里的 /* 被当成注释开始 ⇒ 后面的真代码被吞").toContain("let q = 1;");
    expect(out).toContain("let r = 2;");
    expect(out).not.toContain("真注释");
    // 字符串内容本身保留（它是代码的一部分，不该被剥）
    expect(out).toContain('"~/.local/*/bin"');
  });

  it("回归：`config_surface.rs` 剥完行数不变，且被吞过的那一段仍在", () => {
    const raw = readFileSync(resolve(REPO_ROOT, "src-tauri/src/config_surface.rs"), "utf8");
    const out = stripComments(raw, "rust");
    expect(out.split("\n"), "行结构必须逐行对齐").toHaveLength(raw.split("\n").length);
    // 旧实现从 :775 的 "~/.local/*/bin" 一路吞到 :1296，521 行隐形
    const swallowed = raw.split("\n").slice(800, 1290).join("\n");
    const fnNames = [...swallowed.matchAll(/\bfn\s+([a-z_0-9]+)/g)].map((m) => m[1]);
    expect(fnNames.length, "取样区间里应当有真函数，否则这条回归测不到东西").toBeGreaterThan(5);
    for (const name of fnNames) {
      expect(out, `曾被吞掉的 fn ${name} 现在必须对守卫可见`).toContain(`fn ${name}`);
    }
  });

  it("字符串里的 `//` 不启动行注释；`https://` 也不能被当注释", () => {
    const src = 'let sep = "//"; let u = "https://x/y"; let k = 3;';
    const out = stripComments(src, "ts");
    expect(out).toContain('"//"');
    expect(out).toContain("https://x/y");
    expect(out).toContain("let k = 3;");
  });

  it("Rust 的 `'a` 生命周期不被当字符串定界符（本仓 85 处 `&'static`）", () => {
    const src = "fn f<'a>(s: &'a str) -> &'static str { s }\nlet after = 1;";
    const out = stripComments(src, "rust");
    // 把 ' 当定界符会从 'a> 一路吃到下一个 ' ⇒ 中间的真代码消失
    expect(out).toBe(src);
  });

  it("Rust 原始字符串里的注释起始符不生效", () => {
    const src = ['let g = r"a/*b";', 'let h = r#"c//d"#;', "let i = 4;"].join("\n");
    const out = stripComments(src, "rust");
    expect(out).toContain("let i = 4;");
    expect(out).toContain('r"a/*b"');
    expect(out).toContain('r#"c//d"#');
  });

  it("TS 模板字符串按头注取舍 2 整体当不透明字符串（内容不被当注释）", () => {
    const src = ["const t = `a/*b//c`;", "const u = 5;"].join("\n");
    const out = stripComments(src, "ts");
    expect(out).toContain("const u = 5;");
    expect(out).toContain("`a/*b//c`");
  });

  it("块注释不认嵌套（TS 语义）：第一个 `*/` 收尾", () => {
    // TS 里 `/* a /* b */` 到第一个 */ 结束，之后是代码
    const out = stripComments("/* a /* b */ let z = 6;", "ts");
    expect(out).toContain("let z = 6;");
    expect(out).not.toContain("a");
  });

  it("`lang` 是必填的：Rust 与 TS 对单引号的处理必须不同", () => {
    const src = "x = 'a/*b' + 1;";
    // TS：'…' 是字符串 ⇒ 里面的 /* 无效，整行留住
    expect(stripComments(src, "ts")).toBe(src);
    // Rust：' 不是定界符 ⇒ /*b' + 1; 被当注释开始（无 */ ⇒ 吃到尾）。
    // 这是**刻意**的方言差异，写成断言是为了让「给 lang 一个默认值」的改动当场红。
    expect(stripComments(src, "rust")).not.toBe(src);
  });
});
