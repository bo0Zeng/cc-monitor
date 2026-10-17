// 严格收的两个小件只住 `ipc/decode.ts` 一处：键恰好这几格 · 必有几格 ＋ 可缺几格。
import { describe, it, expect } from "vitest";
import { exactKeys, optionalKeys } from "../../../../src/frontend/ui/ipc/decode";
import { productionTsFiles } from "../../../test-support/production-sources.ts";

describe("ipc/decode", () => {
  it("exactKeys：多一格 / 缺一格都不收，顺序不论", () => {
    expect(exactKeys({ a: 1, b: 2 }, ["b", "a"])).toBe(true);
    expect(exactKeys({ a: 1 }, ["a", "b"])).toBe(false);
    expect(exactKeys({ a: 1, b: 2, c: 3 }, ["a", "b"])).toBe(false);
    expect(exactKeys({}, [])).toBe(true);
  });
  it("optionalKeys：必有的一格不缺，其余只许在可缺那几格里", () => {
    expect(optionalKeys({ a: 1 }, ["a"], ["b"])).toBe(true);
    expect(optionalKeys({ a: 1, b: 2 }, ["a"], ["b"])).toBe(true);
    expect(optionalKeys({ b: 2 }, ["a"], ["b"])).toBe(false);
    expect(optionalKeys({ a: 1, c: 3 }, ["a"], ["b"])).toBe(false);
  });
});

describe("严格收的小件只有一个家", () => {
  it("界面生产段里零处自己定义 isObj / 键集合比较（正控：decode.ts 里认得出）", () => {
    const own = /\b(const|function)\s+(isObj|sameKeys|exactKeys|exactly)\b/;
    const sortedKeys = /Object\.keys\([^)]*\)\.sort\(\)/;
    const hits: string[] = [];
    let control = false;
    for (const s of productionTsFiles("src/frontend/ui")) {
      const mine = own.test(s.text) || sortedKeys.test(s.text);
      if (s.file === "src/frontend/ui/ipc/decode.ts") control = mine;
      else if (mine) hits.push(s.file);
    }
    expect(control, "正控：decode.ts 里没认出定义 —— 识别器坏了或住址搬了").toBe(true);
    expect(hits, "这几份又自己写了一份").toEqual([]);
  });
});

/**
 * 走通道的回包 · 帧 · 拒绝的 `data` 都随形状指纹核过；壳命令 · 壳事件 · 本机后端与界面是同一个构建 ⇒ 一律照生成类型用、不手核。
 * 这几样小件只许下面这几份用（各写明收的是什么）。新添一份先答：它收的东西真的既不经指纹、也不是同一个构建吗？
 */
const USERS: Record<string, string> = {
  "src/frontend/ui/copy-table.ts": "调用方交的插值参数名对表（不是回包）",
};

describe("回包照生成类型用、不手核", () => {
  it("用严格收小件的恰好是登记的那几份", () => {
    const imports = /from\s+"[./]*(?:ipc\/)?decode"/;
    const users = productionTsFiles("src/frontend/ui")
      .filter((s) => s.file !== "src/frontend/ui/ipc/decode.ts" && imports.test(s.text))
      .map((s) => s.file)
      .sort();
    expect(users).toEqual(Object.keys(USERS).sort());
  });
});
