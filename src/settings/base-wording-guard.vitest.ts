/**
 * S8：「基座」这个词不许再出现在**用户可见文案**里。
 *
 * # 为什么换词
 *
 * 「基座」是内部叫法，指「不做账号隔离、直接用继承来的 `CLAUDE_CONFIG_DIR`」那条路径。
 * 对用户它什么都没说明 —— 既不是一个账号名，也不提示会用哪个身份。
 *
 * 换成**「不指定账号」**：它说的正是用户在做的选择（不指定，用登录时已有的那个）。
 * 选这个说法而不是新造词，还有一条现成依据 —— `machine-card.ts` 里那条本来就写着
 * 「不指定（…）」，跟着既有措辞走比再发明一个词好。
 *
 * # 这条守卫扫什么
 *
 * **剥掉整行注释后**的生产源码。内部标识符（`kind: "base"`、`useBase`、`__base__`）
 * 与注释里的「基座」**刻意不管** —— 那是实现词汇，改它们只会制造无谓的改动面。
 * 守的是**字符串字面量**里的那个词。
 */
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";

const SRC = resolve(process.cwd(), "src");

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = resolve(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "generated" || name === "__fixtures__") continue;
      out.push(...walk(p));
    } else if (
      name.endsWith(".ts") &&
      !name.includes(".vitest.") &&
      !name.includes(".test.")
    ) {
      out.push(p);
    }
  }
  return out;
}

/**
 * 单趟扫描抠出字符串字面量。
 *
 * **为什么要写这二十行而不是拿正则凑**：两种偷懒扫法各自被对方的问题绊住 ——
 * ①「剥掉注释再 grep 全文」剥不干净**行尾**注释（`const x = 1; // 落基座`）；
 * ②「直接正则抠字面量」会被**注释里的引号和反引号**骗（注释里写着
 *   「恒含"基座（不隔离）"逃生口」，正则照样当成字面量抓出来）。
 * 实测两种都产生了假红。一个记录 in-string / in-comment 状态的单趟扫描才两者都对。
 */
function stringLiterals(src: string): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    const next = src[i + 1];
    if (c === "/" && next === "/") {
      while (i < src.length && src[i] !== "\n") i++;
      continue;
    }
    if (c === "/" && next === "*") {
      i += 2;
      while (i < src.length && !(src[i] === "*" && src[i + 1] === "/")) i++;
      i += 2;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      const quote = c;
      i++;
      let buf = "";
      while (i < src.length && src[i] !== quote) {
        if (src[i] === "\\") {
          i += 2;
          continue;
        }
        buf += src[i];
        i++;
      }
      i++;
      out.push(buf);
      continue;
    }
    i++;
  }
  return out;
}

describe("S8：UI 文案里不再出现「基座」", () => {
  const files = walk(SRC);

  it("反向自检：确实扫到了一批生产源文件", () => {
    // 不写 `> 0` —— 空转也满足。用一个真实规模的下界。
    expect(files.length).toBeGreaterThan(30);
  });

  it("★ 没有任何字符串字面量含「基座」", () => {
    const hits: string[] = [];
    for (const f of files) {
      for (const lit of stringLiterals(readFileSync(f, "utf8"))) {
        if (lit.includes("基座")) {
          hits.push(`${f.replace(`${process.cwd()}/`, "")}: ${lit.slice(0, 40)}`);
        }
      }
    }
    expect(hits, `这些字面量里仍有「基座」：\n${hits.join("\n")}`).toEqual([]);
  });

  it("抠字面量这一步本身有效（注释里的「基座」不算，字面量里的要抓到）", () => {
    // 直接测机制 —— 否则「全绿」可能只是因为正则一个都没抠出来。
    const decoy = [
      "const x = 1; // 行尾注释里的基座不算",
      " * 块注释里的基座也不算",
      'const label = "基座（不隔离）";',
    ].join("\n");
    const lits = stringLiterals(decoy);
    expect(lits.some((l) => l.includes("基座"))).toBe(true);
    expect(lits).toHaveLength(1); // 只抠出那一个字面量，没把注释也算进来
  });

  it("扫描器认得块注释与转义（这两处是前两版扫法翻车的地方）", () => {
    const src = [
      '/* 块注释里的 "基座（不隔离）" 不算 */',
      'const a = "他说\\"基座\\"";', // 转义引号不该提前结束字面量
      "const b = `模板里的不指定账号`;",
    ].join("\n");
    const lits = stringLiterals(src);
    expect(lits).toContain("模板里的不指定账号");
    // 块注释整段被跳过 —— 它里面那个「基座」不该出现在结果里
    expect(lits.filter((l) => l.includes("不隔离"))).toEqual([]);
  });
});
