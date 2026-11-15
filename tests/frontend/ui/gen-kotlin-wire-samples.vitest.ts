/**
 * 回包的结构夹具（`node tests/scripts/gen-kotlin-wire.mjs --samples`）：每条手机回包的那一份，照生成 Kotlin 用的同一份声明
 * 逐格核得上 —— 必有的格在、非空的不是 null、字面量对得上、并集至少合一形（Kotlin 那头 `Wire.read` 就不出 null）；
 * 递归 / 自引用的类型只往里套一层（一条路径上同一个类型至多两层），截断处给可解的末端。
 * 夹具是测试里现写的几行 TS 声明；另跑一遍仓里真导出的那一份。
 */
import { describe, expect, it } from "vitest";
// @ts-expect-error 纯 JS 脚本，没有类型声明
import { declsFrom, endOf, replySamples, sampleOf } from "../../scripts/gen-kotlin-wire.mjs";

/** 生成器读声明出的类型式（`gen-kotlin-wire.mjs` 的 Parser 那几形）。 */
type Ty =
  | { k: "prim"; v: string }
  | { k: "lit"; v: string }
  | { k: "boollit"; v: boolean }
  | { k: "numlit"; v: string }
  | { k: "array" | "map"; of: Ty }
  | { k: "tuple"; items: Ty[] }
  | { k: "union"; alts: Ty[] }
  | { k: "obj"; fields: { key: string; opt: boolean; ty: Ty }[] }
  | { k: "inter"; parts: Ty[] }
  | { k: "ref"; name: string; args: Ty[] }
  | { k: "raw" };
type Decl = { name: string; params: string[]; ty: Ty; bad?: string };
/** 核的结果：对不上的每一处；一条路径上同一个类型最多出现了几层。 */
type Acc = { misfits: string[]; deepest: number };

/** `v` 照类型 `t` 核（对不上的记 `路径: 为什么`）。 */
function check(v: unknown, t: Ty, decls: Map<string, Decl>): Acc {
  const acc: Acc = { misfits: [], deepest: 0 };
  const isObj = (x: unknown): x is Record<string, unknown> => typeof x === "object" && x !== null && !Array.isArray(x);
  const go = (v: unknown, t: Ty, path: string, env: Record<string, Ty>, stack: string[], out: Acc): void => {
    const bad = (why: string): void => {
      out.misfits.push(`${path || "$"}: ${why}（${JSON.stringify(v)?.slice(0, 60)}）`);
    };
    switch (t.k) {
      case "prim":
        if (t.v === "unknown" || t.v === "any") return;
        if (t.v === "null") return void (v === null || bad("要 null"));
        if (t.v === "number" || t.v === "bigint") return void (typeof v === "number" || bad("要数"));
        return void (typeof v === t.v || bad(`要 ${t.v}`));
      case "lit":
      case "boollit":
        return void (v === t.v || bad(`要 ${JSON.stringify(t.v)}`));
      case "numlit":
        return void (v === Number(t.v) || bad(`要 ${t.v}`));
      case "array":
        if (!Array.isArray(v)) return bad("要数组");
        return v.forEach((x, i) => go(x, t.of, `${path}[${i}]`, env, stack, out));
      case "map":
        if (!isObj(v)) return bad("要键集合");
        return Object.entries(v).forEach(([k, x]) => go(x, t.of, `${path}.${k}`, env, stack, out));
      case "tuple":
        if (!Array.isArray(v) || v.length !== t.items.length) return bad(`要 ${t.items.length} 项的元组`);
        return t.items.forEach((x: Ty, i: number) => go(v[i], x, `${path}[${i}]`, env, stack, out));
      case "union": {
        // 每一形各自核、各自记层数；合上的那一形才算数（没合上的那几形套了几层不算）。
        const tries: Acc[] = t.alts.map((a: Ty) => {
          const o: Acc = { misfits: [], deepest: 0 };
          go(v, a, path, env, stack, o);
          return o;
        });
        const hit = tries.find((o) => o.misfits.length === 0) ?? tries[0];
        out.misfits.push(...hit.misfits);
        out.deepest = Math.max(out.deepest, hit.deepest);
        return;
      }
      case "obj":
        if (!isObj(v)) return bad("要对象");
        for (const f of t.fields) {
          if (!(f.key in v)) {
            if (!f.opt) out.misfits.push(`${path}.${f.key}: 必有的格缺了`);
            continue;
          }
          go(v[f.key], f.ty, `${path}.${f.key}`, env, stack, out);
        }
        return;
      case "inter":
        return t.parts.forEach((p: Ty) => go(v, p, path, env, stack, out));
      case "ref": {
        const name = t.name;
        if (env[name]) return go(v, env[name], path, {}, stack, out);
        const d = decls.get(name);
        if (!d) return bad(`声明里没有 ${name}`);
        // 声明没读懂（落成 raw）⇒ 夹具那一格出不了可解的值，照实记红。
        if (d.ty.k === "raw") return bad(`${name} 的声明没读懂（${d.bad ?? "raw"}）`);
        const e: Record<string, Ty> = {};
        d.params.forEach((p, i) => {
          const a = t.args[i];
          e[p] = a?.k === "ref" && env[a.name] ? env[a.name] : a;
        });
        // 层数按「名 ＋ 代换后的实参」数：`W<W<string>>` 里的两个 W 不是同一个类型。
        const key = `${name}<${JSON.stringify(d.params.map((p) => e[p]))}>`;
        const s = [...stack, key];
        // 截断处给的是这个声明自己的末端（数组声明的 `[]` · 键集合声明的 `{}` · 可空声明的 null）⇒ 不算一层；
        // 对象声明没有末端，`{}` 也照算一层。
        const own = endOf(d.ty, decls);
        const end = own !== undefined && JSON.stringify(v) === JSON.stringify(own);
        if (!end) out.deepest = Math.max(out.deepest, s.filter((n) => n === key).length);
        return go(v, d.ty, path, e, s, out);
      }
      case "raw":
        return bad("认不出的形 raw");
    }
  };
  go(v, t, "", {}, [], acc);
  return acc;
}

const ref = (name: string): Ty => ({ k: "ref", name, args: [] });

describe("递归 / 自引用的类型只往里套一层，截断处给可解的末端", () => {
  const cases: [string, string[], string][] = [
    ["自引用的可缺数组（AgentPick.efforts 那一形）", [`export type Pick = { value: string, efforts?: Array<Pick>, note?: string, };`], "Pick"],
    ["经可空格的自引用", [`export type Tree = { name: string, parent: Tree | null, };`], "Tree"],
    ["两型互引", [`export type A = { b?: B, };`, `export type B = { a: Array<A>, n: number, };`], "A"],
    ["并集里第一形就回到自己", [`export type E = { "t": "add", l: E, r: E, } | { "t": "num", n: number, };`], "E"],
    ["经别名的数组", [`export type Kids = Array<N>;`, `export type N = { name: string, kids: Kids, };`], "N"],
    ["经别名的键集合", [`export type Idx = Record<string, M>;`, `export type M = { id: string, idx: Idx, };`], "M"],
    ["元组里的数组", [`export type T = { name: string, t: [string, Array<T>], };`], "T"],
    ["别名自己就是数组、经对象回到它", [`export type Top = Array<L>;`, `export type L = { more: Top, };`], "Top"],
  ];
  for (const [what, texts, top] of cases) {
    it(`${what} ⇒ 照声明核得上、同一个类型至多两层`, () => {
      const decls = declsFrom(texts);
      const v = sampleOf(ref(top), decls, new Map());
      const r = check(v, ref(top), decls);
      expect(r.misfits).toEqual([]);
      expect(r.deepest).toBeLessThanOrEqual(2);
    });
  }

  it("自引用的可缺数组：外一层带着那一格（套一层），里一层不给那一格", () => {
    const decls = declsFrom([`export type Pick = { value: string, efforts?: Array<Pick>, note?: string, };`]);
    expect(sampleOf(ref("Pick"), decls, new Map())).toEqual({
      value: "value-x",
      efforts: [{ value: "value-x", note: "note-x" }],
      note: "note-x",
    });
  });

  it("经可空格的自引用：里一层那一格给 null", () => {
    const decls = declsFrom([`export type Tree = { name: string, parent: Tree | null, };`]);
    expect(sampleOf(ref("Tree"), decls, new Map())).toEqual({ name: "name-x", parent: { name: "name-x", parent: null } });
  });

  it("可缺又可空的格回到自己：里一层不给那一格（不是给 null）", () => {
    const decls = declsFrom([`export type P = { name: string, p?: P | null, };`]);
    expect(sampleOf(ref("P"), decls, new Map())).toEqual({ name: "name-x", p: { name: "name-x" } });
  });

  it("经别名的键集合：里一层那一格给 {}", () => {
    const decls = declsFrom([`export type Idx = Record<string, M>;`, `export type M = { id: string, idx: Idx, };`]);
    expect(sampleOf(ref("M"), decls, new Map())).toEqual({ id: "id-x", idx: { k: { id: "id-x", idx: {} } } });
  });

  it("别名自己就是数组、经对象回到它：套足两层，第三层是那个别名自己的末端 []", () => {
    const decls = declsFrom([`export type Top = Array<L>;`, `export type L = { more: Top, };`]);
    expect(sampleOf(ref("Top"), decls, new Map())).toEqual([{ more: [{ more: [] }] }]);
  });

  it("可缺的格里是数组套数组、回到自己 ⇒ 里一层不给那一格（容器兜底的 [] 不往里塞）", () => {
    const decls = declsFrom([`export type N = { name: string, kids?: Array<Array<N>>, };`]);
    expect(sampleOf(ref("N"), decls, new Map())).toEqual({ name: "name-x", kids: [[{ name: "name-x" }]] });
  });

  it("可空藏在别名里（M = N | null）也算可空的位置 ⇒ 里一层给 [null]，不让容器兜底的 [] 抢先", () => {
    const decls = declsFrom([`export type M = N | null;`, `export type N = { name: string, kids: Array<Array<N> | M>, };`]);
    expect(sampleOf(ref("N"), decls, new Map())).toEqual({ name: "name-x", kids: [[{ name: "name-x", kids: [null] }]] });
  });

  it("核对器数得出全可缺的自引用套了三层（第三层碰巧是 {} 也算一层）", () => {
    const decls = declsFrom([`export type P = { p?: P, };`]);
    expect(check({ p: { p: {} } }, ref("P"), decls).deepest).toBe(3);
  });

  it("经别名的数组：里一层那一格给 []", () => {
    const decls = declsFrom([`export type Kids = Array<N>;`, `export type N = { name: string, kids: Kids, };`]);
    expect(sampleOf(ref("N"), decls, new Map())).toEqual({ name: "name-x", kids: [{ name: "name-x", kids: [] }] });
  });

  it("别名自己就是数组、元素就是它自己（Top = Array<Top>）⇒ 套两层，第三层落成 []", () => {
    const decls = declsFrom([`export type Top = Array<Top>;`]);
    const v = sampleOf(ref("Top"), decls, new Map());
    expect(v).toEqual([[[]]]);
    expect(check(v, ref("Top"), decls)).toEqual({ misfits: [], deepest: 2 });
  });

  it("数组的元素可空、回到自己 ⇒ 里一层那一格给 [null]（末端落在原处，不丢成 []）", () => {
    const decls = declsFrom([`export type N = { name: string, kids: Array<N | null>, };`]);
    expect(sampleOf(ref("N"), decls, new Map())).toEqual({ name: "name-x", kids: [{ name: "name-x", kids: [null] }] });
  });

  it("不递归的泛型套泛型（W<W<W<string>>>）⇒ 不截、照实套三层", () => {
    const decls = declsFrom([`export type W<X> = { w: X, };`, `export type Top = { v: W<W<W<string>>>, };`]);
    const v = sampleOf(ref("Top"), decls, new Map());
    expect(v).toEqual({ v: { w: { w: { w: "w-x" } } } });
    expect(check(v, ref("Top"), decls).misfits).toEqual([]);
  });

  it("核对器不放过没读懂的声明（落成 raw 的那一格）", () => {
    const decls = declsFrom([`export type R = { x: [ };`, `export type Top = { r: R, };`]);
    expect(decls.get("R")?.ty.k).toBe("raw");
    expect(check(sampleOf(ref("Top"), decls, new Map()), ref("Top"), decls).misfits).toEqual([expect.stringContaining("R 的声明没读懂")]);
  });

  it("必有的格只能回到自己 ⇒ 报错、说是哪个类型，不出半截的样子", () => {
    const decls = declsFrom([`export type Bad = { next: Bad, };`]);
    expect(() => sampleOf(ref("Bad"), decls, new Map())).toThrow(/Bad/);
  });
});

describe("仓里真导出的每条手机回包：--samples 那一份解得开", () => {
  const { decls, replies: all }: { decls: Map<string, Decl>; replies: Map<string, { reply: Ty; sample: unknown }> } = replySamples();

  it("读到了手机那张表里的每条命令", () => {
    expect(all.size).toBeGreaterThan(10);
    expect([...all.keys()]).toContain("session-open-facts");
  });

  for (const [cmd, { reply, sample }] of all) {
    it(`${cmd} ⇒ 照声明核得上、同一个类型至多两层`, () => {
      const r = check(sample, reply, decls);
      expect(r.misfits).toEqual([]);
      expect(r.deepest).toBeLessThanOrEqual(2);
    });
  }
});
