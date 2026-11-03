#!/usr/bin/env node
// 手机那一侧的线上类型（Kotlin）由这里从 ts-rs 导出的声明生成，不手抄。
//
//   读：src/frontend/ui/generated/*.ts（ts-rs 从 Rust 类型导出的那一份）· TypedCommands.ts（命令 ⇒ 入参 · 回包）·
//       Rust 源里写成 f32 / f64 的格（TS 只有一种 `number`，整数与小数写回时字节不同 ⇒ 照 Rust 定）·
//       judgment-rules.ts 里本机那台的机器键 `LOCAL`（手机不手写 `"<local>"`）。
//   写：src/mobile/core-wire/src/main/kotlin/com/ccmonitor/mobile/core/wire/gen/Generated.kt（类型 ＋ 各自的编解码 ＋ 常量 `LOCAL`）·
//       同目录 Commands.kt（命令表：命令名 ⇒ 回包怎么解）。
//   用法：node tests/scripts/gen-kotlin-wire.mjs            重生成上面两份
//         node tests/scripts/gen-kotlin-wire.mjs --samples  印每条命令回包的结构夹具（`tests/__fixtures__/wire-replies.golden.json` 的 `sample` 格从它来）
//
// 映射：对象 ⇒ data class（格按线上顺序，键名原样）· 字符串并集 ⇒ enum · 带同一个字面量判别格的对象并集 ⇒ sealed 一族 ·
// `T | null` ⇒ 可空（写回照样写出 null）· `k?:` ⇒ 可缺（写回时缺）· Array ⇒ List · 键集合 ⇒ Map · 认不出的形 ⇒ Raw（原样那一段 JSON）。
// 编解码是生成的（Moshi 的 JsonReader / JsonWriter，不经反射、不经 codegen）：写回与 Rust 逐字节同。
//
// 撞名：一族的各形是 `sealed interface` 里的嵌套类，形名照线上那个词（`list` ⇒ `List`）或照它装的那个类型名（`string | InlNode` ⇒ `InlNode`），
// 不为生成器改名。嵌套名在一族体内遮住同名的顶层名 ⇒ 一族体内的类型引用撞上某个形名的，写全名：Kotlin 标准名写 `kotlin.…` /
// `kotlin.collections.…`，生成类型与编解码那几个 val 写 `com.ccmonitor.mobile.core.wire.gen.…`；不撞的照写短名。
// 形名撞上一族骨架自己要写的名（判别格的 `String` · `Ordered` · `Variant` … 见 [FAMILY_FRAME]）或跟一族同名 ⇒ 生成时报错，不悄悄出编不过的 Kotlin。

import { readFileSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../..");
const GEN = join(ROOT, "src/frontend/ui/generated");
const OUT_DIR = join(ROOT, "src/mobile/core-wire/src/main/kotlin/com/ccmonitor/mobile/core/wire/gen");
const PKG = "com.ccmonitor.mobile.core.wire.gen";

// ───────────────────────────── 读 TS 声明 ─────────────────────────────

function stripComments(s) {
  return s.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
}

function tokenize(s) {
  const out = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (/\s/.test(c)) { i++; continue; }
    if (c === '"') {
      let j = i + 1;
      while (s[j] !== '"') j += s[j] === "\\" ? 2 : 1;
      out.push({ t: "str", v: JSON.parse(s.slice(i, j + 1)) });
      i = j + 1;
      continue;
    }
    if (/[A-Za-z_$]/.test(c)) {
      let j = i;
      while (j < s.length && /[A-Za-z0-9_$]/.test(s[j])) j++;
      out.push({ t: "id", v: s.slice(i, j) });
      i = j;
      continue;
    }
    if (/[0-9-]/.test(c)) {
      let j = i + 1;
      while (j < s.length && /[0-9.eE+-]/.test(s[j])) j++;
      out.push({ t: "num", v: s.slice(i, j) });
      i = j;
      continue;
    }
    out.push({ t: "p", v: c });
    i++;
  }
  return out;
}

class Parser {
  constructor(toks) { this.k = toks; this.i = 0; }
  peek(o = 0) { return this.k[this.i + o]; }
  is(v) { const t = this.peek(); return t && t.t === "p" && t.v === v; }
  eat(v) { if (!this.is(v)) throw new Error(`want ${v} at ${JSON.stringify(this.peek())}`); this.i++; }
  type() {
    const alts = [this.inter()];
    while (this.is("|")) { this.i++; alts.push(this.inter()); }
    return alts.length === 1 ? alts[0] : { k: "union", alts };
  }
  inter() {
    const parts = [this.primary()];
    while (this.is("&")) { this.i++; parts.push(this.primary()); }
    return parts.length === 1 ? parts[0] : { k: "inter", parts };
  }
  primary() {
    const t = this.peek();
    if (t.t === "str") { this.i++; return { k: "lit", v: t.v }; }
    if (t.t === "num") { this.i++; return { k: "numlit", v: t.v }; }
    if (t.t === "p" && t.v === "(") { this.i++; const x = this.type(); this.eat(")"); return x; }
    if (t.t === "p" && t.v === "[") {
      this.i++;
      const items = [];
      while (!this.is("]")) { items.push(this.type()); if (this.is(",")) this.i++; }
      this.eat("]");
      return { k: "tuple", items };
    }
    if (t.t === "p" && t.v === "{") return this.object();
    if (t.t === "id") {
      this.i++;
      const name = t.v;
      let args = [];
      if (this.is("<")) {
        this.i++;
        while (!this.is(">")) { args.push(this.type()); if (this.is(",")) this.i++; }
        this.eat(">");
      }
      if (name === "Array") return { k: "array", of: args[0] };
      if (name === "true" || name === "false") return { k: "boollit", v: name === "true" };
      if (name === "Record") return args[0].k === "ref" && args[0].name === "symbol" ? { k: "obj", fields: [] } : { k: "map", of: args[1] };
      if (["string", "number", "boolean", "bigint", "null", "unknown", "any"].includes(name)) return { k: "prim", v: name };
      return { k: "ref", name, args };
    }
    throw new Error(`unexpected ${JSON.stringify(t)}`);
  }
  object() {
    this.eat("{");
    if (this.is("[")) {
      // { [key in K]?: V }
      this.i++;
      this.i++; // key
      this.i++; // in
      this.type();
      this.eat("]");
      if (this.is("?")) this.i++;
      this.eat(":");
      const v = this.type();
      if (this.is(",") || this.is(";")) this.i++;
      this.eat("}");
      return { k: "map", of: v };
    }
    const fields = [];
    while (!this.is("}")) {
      const kt = this.peek();
      this.i++;
      let opt = false;
      if (this.is("?")) { this.i++; opt = true; }
      this.eat(":");
      const ty = this.type();
      fields.push({ key: kt.v, opt, ty });
      if (this.is(",") || this.is(";")) this.i++;
    }
    this.eat("}");
    return { k: "obj", fields };
  }
}

export function loadDecls() {
  return declsFrom(readdirSync(GEN).filter((f) => f.endsWith(".ts")).map((f) => readFileSync(join(GEN, f), "utf8")));
}

/** 几份 TS 声明的原文（每份一个 `export type`）⇒ 名 ⇒ 声明。 */
export function declsFrom(texts) {
  const decls = new Map();
  for (const raw of texts) {
    const text = stripComments(raw);
    const m = text.match(/export type (\w+)(?:<([^>]*)>)?\s*=\s*([\s\S]*?);\s*$/);
    if (!m) continue;
    const [, name, params, body] = m;
    try {
      const p = new Parser(tokenize(body));
      const ty = p.type();
      decls.set(name, { name, params: params ? params.split(",").map((s) => s.split("=")[0].trim()) : [], defaults: params ? params.split(",").map((s) => s.split("=")[1]?.trim()) : [], ty });
    } catch (e) {
      decls.set(name, { name, params: [], ty: { k: "raw" }, bad: String(e) });
    }
  }
  return decls;
}

/** TypedCommands.ts：命令 ⇒ { req, reply }（类型表达式）。 */
function loadCommands() {
  const text = stripComments(readFileSync(join(GEN, "TypedCommands.ts"), "utf8"));
  const out = new Map();
  const re = /^\s*"([a-z0-9-]+)":\s*\{\s*req:\s*([\s\S]*?);\s*reply:\s*([\s\S]*?);\s*code:/gm;
  let m;
  while ((m = re.exec(text))) {
    out.set(m[1], { req: new Parser(tokenize(m[2])).type(), reply: new Parser(tokenize(m[3])).type() });
  }
  return out;
}

/** 手机按类型读回包的那几条命令（`cmd-wire/src/call.rs` 那张 `calls!` 表，一行 `入参 => "名字", 回包;`；两层入参同名的只算一条）。 */
function phoneCommands() {
  const text = readFileSync(join(ROOT, "src/common/cmd-wire/src/call.rs"), "utf8");
  const start = text.indexOf("\ncalls! {");
  if (start < 0) throw new Error("call.rs 里找不到 `calls! { … }` 那张表");
  const body = text.slice(start, text.indexOf("\n}", start));
  const names = [...body.matchAll(/^\s*[\w<>, ]+=>\s*"([a-z0-9-]+)"\s*,/gm)].map((m) => m[1]);
  if (names.length < 10) throw new Error(`\`calls!\` 表只读到 ${names.length} 行 —— 读法坏了`);
  return [...new Set(names)];
}

// ───────────────────── Rust 字段的格宽 ─────────────────────

/** 仓里 `src/common` · `src/backend` 底下每一份 Rust 源的格宽（见 [rustFieldWidths]）。 */
export function rustFloatFields() {
  const texts = [];
  const walk = (d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) { if (!["target", "vendor", "node_modules"].includes(e.name)) walk(p); continue; }
      if (e.name.endsWith(".rs")) texts.push(readFileSync(p, "utf8"));
    }
  };
  walk(join(ROOT, "src/common"));
  walk(join(ROOT, "src/backend"));
  return rustFieldWidths(texts);
}

/**
 * 几段 Rust 源里派生了 `TS` 的 struct / enum ⇒ 导出名 ⇒ 格宽记号的集合（小数记格名 · 无符号 64 位记 `u:格名` · 32 位以内整数记 `i:格名`；
 * snake 与 camel 都记；flatten 进来的类型的格一并带上）。格宽照 Rust 的字段类型定，类型与字段写成哪种可见性都一样读。
 */
export function rustFieldWidths(texts) {
  const own = new Map(); // Rust 类型名 ⇒ { ts: 导出名, floats: Set, flat: [被 flatten 进来的类型名] }
  const both = (x, tag, name) => {
    x.floats.add(tag + name);
    x.floats.add(tag + name.replace(/_([a-z])/g, (_, c) => c.toUpperCase()));
  };
  for (const text of texts) {
    // 可见性（`pub` · `pub(crate)` · `pub(super)` · `pub(in …)` · 不写）不进判断：格宽是类型的事。
    for (const m of text.matchAll(/(?<![\w])(?:struct|enum) (\w+)[^{;]*\{([\s\S]*?)\n\}/g)) {
      // 紧挨着它的那几行属性里的 `ts(rename = "…")`（导出名与 Rust 名不同）。
      const before = text.slice(Math.max(0, m.index - 800), m.index);
      const attrs = before.slice(Math.max(before.lastIndexOf("}\n"), before.lastIndexOf(";\n")) + 1);
      // 只认导出到线上的类型（派生了 `TS`）：别处同名、不上线的类型（如落盘声明的 `Doc<T>`）不许盖掉线上那一份的格宽。
      if (!/\bTS\b/.test(attrs)) continue;
      const rn = attrs.match(/ts\([^)]*rename\s*=\s*"(\w+)"/);
      const x = { ts: rn ? rn[1] : m[1], floats: new Set(), flat: [] };
      for (const g of m[2].matchAll(/(\w+): (?:Option<)?(?:Vec<)?f(?:32|64)\b/g)) both(x, "", g[1]);
      // 无符号 64 位（摘要一类，可以大过 Long）⇒ Kotlin 的 Long 照位存、写回照无符号；记成 `u:格名`。
      for (const g of m[2].matchAll(/(\w+): (?:Option<)?(?:Vec<)?u64\b/g)) both(x, "u:", g[1]);
      // 32 位以内的整数（Rust 那头装不下更大的）⇒ Kotlin 的 Int；记成 `i:格名`。
      for (const g of m[2].matchAll(/(\w+): (?:Option<)?(?:Vec<)?[ui](?:8|16|32)\b/g)) both(x, "i:", g[1]);
      for (const g of m[2].matchAll(/#\[serde\(flatten\)\][\s\S]*?(?<![\w])\w+: (?:[\w:]+::)?(\w+)/g)) x.flat.push(g[1]);
      own.set(m[1], x);
    }
  }
  const all = (name, seen = new Set()) => {
    const x = own.get(name);
    if (!x || seen.has(name)) return new Set();
    seen.add(name);
    const out = new Set(x.floats);
    for (const c of x.flat) for (const v of all(c, seen)) out.add(v);
    return out;
  };
  const out = new Map(); // 导出名 ⇒ Set(格名，snake 与 camel 都记)
  for (const [name, x] of own) {
    const fl = all(name);
    if (fl.size) out.set(x.ts, new Set([...(out.get(x.ts) ?? []), ...fl]));
  }
  return out;
}

// ───────────────────────────── 出 Kotlin ─────────────────────────────

/** Kotlin 标准名住哪个包（一族体内被同名的形遮住时写全名用）；别的（生成类型 · 编解码那几个 val）都住 [PKG]。 */
const FULL = { String: "kotlin", Boolean: "kotlin", Long: "kotlin", Int: "kotlin", Double: "kotlin", List: "kotlin.collections", Map: "kotlin.collections" };
/** 一族体内（`ctx.shadow` 是那一族各形的名）撞上形名的那个名 ⇒ 全名；不撞照写短名。 */
const qual = (ctx, short) => (ctx.shadow?.has(short) ? `${FULL[short] ?? PKG}.${short}` : short);
/** 一族体内骨架自己要写的名（判别格的 `String` · `Ordered` · 编解码那几个构件 …）：形名撞上它们就生成不了。 */
const FAMILY_FRAME = new Set(["String", "Any", "Int", "Boolean", "IntArray", "Ordered", "JsonAdapter", "JsonReader", "Variant", "Ext", "Tok", "Field", "Class", "Suppress"]);

/** 一族各形的名 ⇒ 体内要写全名的那一组；撞了骨架名或跟一族同名 ⇒ 报出哪一族哪一形。 */
function familyShadow(name, vnames) {
  for (const v of vnames) {
    if (FAMILY_FRAME.has(v) || v === name) throw new Error(`${name}.${v}：形名撞了一族体内骨架要写的名，生成不了 —— 改生成器给这一形另起 Kotlin 名的规则`);
  }
  return new Set(vnames);
}

const pascal = (s) => s.replace(/(^|[_\-\s.])([a-zA-Z0-9])/g, (_, __, c) => c.toUpperCase()).replace(/[^A-Za-z0-9]/g, "");
/** 枚举常量名：线上那个词照大写下划线（`not_yours` · `hostKey` ⇒ `NOT_YOURS` · `HOST_KEY`）。 */
const upper = (s) => (s.replace(/([a-z0-9])([A-Z])/g, "$1_$2").replace(/[^A-Za-z0-9]+/g, "_").replace(/^_|_$/g, "").toUpperCase() || "EMPTY").replace(/^([0-9])/, "N$1");
const camel = (s) => { const p = pascal(s); return p.charAt(0).toLowerCase() + p.slice(1); };
const KW = new Set(["class", "object", "fun", "val", "var", "in", "is", "as", "when", "if", "else", "for", "while", "do", "return", "null", "true", "false", "this", "super", "typealias", "interface", "package", "import", "try", "catch", "throw", "break", "continue", "type"]);
const prop = (key) => { const c = camel(key) || "x"; const n = /^[0-9]/.test(c) ? "n" + c : c; return KW.has(n) ? "`" + n + "`" : n; };

class Gen {
  constructor(decls, floats) {
    this.decls = decls;
    this.floats = floats;
    this.out = new Map(); // Kotlin 类型名 ⇒ 代码
    this.done = new Set();
    this.raw = [];
    this.rawNames = new Set();
  }

  /** 类型表达式 ⇒ { kt: Kotlin 型, ad: 编解码表达式, nullable }。`ctx.shadow`：此刻在一族体内，那一族各形的名（撞上的写全名，见头注）。 */
  ty(t, ctx) {
    const q = (short) => qual(ctx, short);
    switch (t.k) {
      case "prim":
        if (t.v === "string") return { kt: q("String"), ad: q("Str") };
        if (t.v === "boolean") return { kt: q("Boolean"), ad: q("Bool") };
        if (t.v === "number" || t.v === "bigint") return ctx.float ? { kt: q("Double"), ad: q("Dbl") } : ctx.int ? { kt: q("Int"), ad: q("Int32") } : ctx.u64 ? { kt: q("Long"), ad: q("U64") } : { kt: q("Long"), ad: q("Lng") };
        return { kt: q("Raw"), ad: q("RawJson") };
      case "array": {
        const e = this.ty(t.of, ctx);
        return { kt: `${q("List")}<${e.kt}>`, ad: `arr(${e.ad})` };
      }
      case "map": {
        const e = this.ty(t.of, ctx);
        return { kt: `${q("Map")}<${q("String")}, ${e.kt}>`, ad: `dict(${e.ad})` };
      }
      case "union": {
        const alts = t.alts.filter((a) => !(a.k === "prim" && a.v === "null"));
        const nullable = alts.length < t.alts.length;
        let inner;
        if (alts.length === 1) inner = this.ty(alts[0], ctx);
        else inner = this.anon({ k: "union", alts }, ctx);
        return nullable ? { kt: inner.kt + "?", ad: `opt(${inner.ad})` } : inner;
      }
      case "lit": return this.anon({ k: "union", alts: [t] }, ctx);
      case "boollit": return { kt: "Boolean", ad: "Bool" };
      case "obj": case "inter": case "tuple": return this.anon(t, ctx);
      case "ref": {
        if (ctx.params && ctx.params.includes(t.name)) return { kt: t.name, ad: `ad${t.name}` };
        const d = this.decls.get(t.name);
        if (!d) { this.raw.push(`${ctx.owner}: 没有 ${t.name} 的声明`); return { kt: q("Raw"), ad: q("RawJson") }; }
        // 别名（`type X = Y<…>` · 数组 · 键集合 · 原始型）不另出一个类，照它指的那一个。
        if (!d.params.length && ["ref", "array", "map", "prim"].includes(d.ty.k)) return this.ty(d.ty, { ...ctx, owner: d.name });
        this.emit(d);
        if (this.rawNames.has(d.name)) return { kt: q("Raw"), ad: q("RawJson") };
        const args = t.args.map((a) => this.ty(a, ctx));
        const n = q(t.name);
        if (!args.length) return { kt: n, ad: `${n}.ADAPTER` };
        return { kt: `${n}<${args.map((a) => a.kt).join(", ")}>`, ad: `${n}.adapter(${args.map((a) => a.ad).join(", ")})` };
      }
      default:
        return { kt: "Raw", ad: "RawJson" };
    }
  }

  /** 一组字面量 ⇒ 有名字的那一个枚举（导出里恰好一个类型就是这一组词，Rust 那头 `ts(type = …)` 内联出来的同一组）。 */
  namedWords(alts) {
    if (!this.wordSets) {
      this.wordSets = new Map();
      for (const d of this.decls.values()) {
        const as = d.ty.k === "union" ? d.ty.alts : [d.ty];
        if (d.params.length || !as.every((a) => a.k === "lit")) continue;
        const key = as.map((a) => a.v).sort().join("\u0000");
        this.wordSets.set(key, this.wordSets.has(key) ? null : d.name);
      }
    }
    if (!alts.every((a) => a.k === "lit")) return null;
    return this.wordSets.get(alts.map((a) => a.v).sort().join("\u0000")) ?? null;
  }

  /** 收窄过的那几个词（Rust 那一格是有名的枚举、TS 照 `ts(type)` 只写了它的一部分）⇒ 含这几个词、词最少的那个有名枚举；没有 ⇒ `null`。 */
  namedSuperset(alts) {
    if (!alts.every((a) => a.k === "lit")) return null;
    const want = alts.map((a) => a.v);
    let best = null;
    for (const d of this.decls.values()) {
      const as = d.ty.k === "union" ? d.ty.alts : [d.ty];
      if (d.params.length || !as.every((a) => a.k === "lit")) continue;
      const words = new Set(as.map((a) => a.v));
      if (want.every((w) => words.has(w)) && (!best || words.size < best.size)) best = { name: d.name, size: words.size };
    }
    return best?.name ?? null;
  }

  /** 每一组内联的字面量（不是自己导出的那一种）在全部声明里出现几回。 */
  inlineCounts() {
    if (this.counts) return this.counts;
    this.counts = new Map();
    const walk = (t, top) => {
      if (!t || typeof t !== "object") return;
      if (t.k === "union" && !top && t.alts.every((a) => a.k === "lit")) {
        const key = t.alts.map((a) => a.v).sort().join("\u0000");
        this.counts.set(key, (this.counts.get(key) ?? 0) + 1);
      }
      if (t.k === "lit" && !top) {
        this.counts.set(t.v, (this.counts.get(t.v) ?? 0) + 1);
      }
      for (const v of Object.values(t)) {
        if (Array.isArray(v)) v.forEach((x) => walk(x, false));
        else if (v && typeof v === "object") walk(v, false);
      }
    };
    for (const d of this.decls.values()) walk(d.ty, true);
    return this.counts;
  }

  anon(t, ctx) {
    const named = t.k === "union" ? this.namedWords(t.alts) : null;
    if (named) return this.ty({ k: "ref", name: named, args: [] }, ctx);
    // 内联的同一组词出现在不止一格：Rust 那头是有名的枚举（收窄过）⇒ 用那个名；没有名字 ⇒ 生成不了，去 `cmd-wire` 给这个概念起一个名（不拼词当名）。
    const sorted = t.k === "union" && t.alts.every((a) => a.k === "lit") ? t.alts.map((a) => a.v).sort() : null;
    const shared = sorted && sorted.length > 1 && this.inlineCounts().get(sorted.join("\u0000")) > 1;
    if (shared) {
      const sup = this.namedSuperset(t.alts);
      if (sup) return this.ty({ k: "ref", name: sup, args: [] }, ctx);
      throw new Error(`${ctx.owner}.${ctx.field}：几格共用的一组词 ${sorted.join(" | ")} 没有名字 —— 在 cmd-wire 里给它起一个`);
    }
    const name = ctx.owner + pascal(ctx.field ?? "Item");
    // 内联的形另出一个顶层类：它的体不在这一族里 ⇒ 不带 shadow。
    this.emitDecl(name, [], t, { ...ctx, owner: name, shadow: undefined });
    if (this.rawNames.has(name)) return { kt: qual(ctx, "Raw"), ad: qual(ctx, "RawJson") };
    return { kt: qual(ctx, name), ad: `${qual(ctx, name)}.ADAPTER` };
  }

  emit(d) {
    if (this.done.has(d.name)) return;
    this.emitDecl(d.name, d.params, d.ty, { owner: d.name, params: d.params });
  }

  fieldsOf(t, ctx) {
    if (t.k === "obj") return t.fields;
    if (t.k === "inter") {
      const parts = t.parts.map((p) => this.fieldsOf(p, ctx));
      return parts.every(Boolean) ? parts.flat() : null;
    }
    if (t.k === "ref") {
      const d = this.decls.get(t.name);
      if (d && !d.params.length) return this.fieldsOf(d.ty, ctx);
    }
    return null;
  }

  /** 交叉里夹着一个并集（`{共有的格} & ({…} | {…})`）⇒ 摊成并集：每一形都带上共有的那几格。 */
  distribute(t) {
    if (t.k !== "inter") return t;
    const u = t.parts.find((p) => p.k === "union");
    if (!u) return t;
    const rest = t.parts.filter((p) => p !== u);
    return { k: "union", alts: u.alts.map((a) => ({ k: "inter", parts: [...rest, a] })) };
  }

  /** 并集各形在 JSON 里是哪一种记号（分得开就能按记号认）。 */
  tokenOf(t) {
    if (t.k === "prim") return { string: "STRING", number: "NUMBER", bigint: "NUMBER", boolean: "BOOLEAN" }[t.v];
    if (t.k === "boollit") return "BOOLEAN";
    if (t.k === "array" || t.k === "tuple") return "BEGIN_ARRAY";
    if (t.k === "obj" || t.k === "inter" || t.k === "map") return "BEGIN_OBJECT";
    if (t.k === "ref") {
      const d = this.decls.get(t.name);
      if (!d) return null;
      if (d.ty.k === "union") return d.ty.alts.every((a) => a.k === "lit") ? "STRING" : "BEGIN_OBJECT";
      return this.tokenOf(d.ty);
    }
    if (t.k === "lit") return "STRING";
    return null;
  }

  emitDecl(name, params, t0, ctx) {
    if (this.done.has(name)) return;
    this.done.add(name);
    this.out.set(name, ""); // 先占位（递归引用）
    const floats = this.floats.get(name) ?? new Set();
    const t = this.distribute(t0);
    const alts = t.k === "union" ? t.alts.filter((a) => !(a.k === "prim" && a.v === "null")) : [t];
    const c = { ...ctx, floats };
    // 字符串并集 ⇒ 枚举
    if (alts.every((a) => a.k === "lit")) {
      this.out.set(name, this.enumCode(name, alts.map((a) => a.v)));
      return;
    }
    // 对象（含交叉）
    if (t.k !== "union") {
      const fields = this.fieldsOf(t, ctx);
      if (fields) {
        this.out.set(name, this.objCode(name, params, fields, c));
        return;
      }
    }
    if (t.k === "union") {
      const objs = alts.map((a) => this.fieldsOf(a, ctx));
      if (objs.every(Boolean)) {
        // 带同一个字面量判别格（serde internally tagged）
        const disc = objs[0].map((f) => f.key).find((k) =>
          objs.every((fs) => fs.some((f) => f.key === k && f.ty.k === "lit")));
        if (disc) {
          this.out.set(name, this.sealedCode(name, params, disc, objs, c));
          return;
        }
        // 每一形一个键（serde externally tagged）
        const keys = objs.map((fs) => (fs.length === 1 ? fs[0].key : null));
        if (keys.every(Boolean) && new Set(keys).size === keys.length) {
          this.out.set(name, this.externalCode(name, params, objs.map((fs) => fs[0]), c));
          return;
        }
      }
      // 各形的记号分得开（untagged：串 · 数 · 真假 · 对象 · 数组各至多一形）
      const toks = alts.map((a) => this.tokenOf(a));
      if (toks.every(Boolean) && new Set(toks).size === toks.length) {
        this.out.set(name, this.tokenCode(name, params, alts, toks, c));
        return;
      }
    }
    // 别名（`type Words = string` 一类）：Kotlin 里照名字可写，编解码照它指的那一个（[ty] 不经这里）。
    if (["prim", "ref", "array", "map"].includes(t.k) && !params.length) {
      this.out.set(name, `typealias ${name} = ${this.ty(t, ctx).kt}\n`);
      return;
    }
    if (t.k === "tuple") {
      this.out.set(name, this.tupleCode(name, t.items, c));
      return;
    }
    this.raw.push(`${name}: 认不出的形（${t.k}${t.k === "union" ? "：" + t.alts.map((a) => a.k).join("|") : ""}）⇒ 原样`);
    this.rawNames.add(name);
    this.out.delete(name);
  }

  enumCode(name, vals) {
    const consts = vals.map((v) => `    ${upper(v)}("${esc(v)}"),`).join("\n");
    return `enum class ${name}(val wire: String) {\n${consts}\n    ;\n\n    companion object {\n        val ADAPTER: JsonAdapter<${name}> = enumOf(entries) { it.wire }\n    }\n}\n`;
  }

  fieldSpecs(fields, ctx) {
    return fields.map((f) => {
      const x = this.ty(f.ty, { ...ctx, field: f.key, float: ctx.floats?.has(f.key), int: ctx.floats?.has("i:" + f.key), u64: ctx.floats?.has("u:" + f.key) });
      const nullable = x.kt.endsWith("?") || f.opt;
      const kt = f.opt && !x.kt.endsWith("?") ? x.kt + "?" : x.kt;
      const ad = f.opt && !x.kt.endsWith("?") ? `opt(${x.ad})` : x.ad;
      return { key: f.key, prop: prop(f.key), kt, ad, opt: f.opt, nullable };
    });
  }

  /** `specs` 里可以夹判别格（`tag`：不进类，读时核那个词、写时写那个词，位置照声明）。 */
  objBody(cls, specs, typeArgs) {
    const props = specs.filter((s) => s.tag === undefined);
    const ctor = props.map((s) => `    val ${s.prop}: ${s.kt}${s.opt ? " = null" : ""},`).join("\n");
    const fieldList = specs.map((s) => s.tag !== undefined
      ? `            Field("${esc(s.key)}", { tag("${esc(s.tag)}") }, optional = false, nullable = false),`
      : `            Field("${esc(s.key)}", { ${s.ad} }, optional = ${s.opt}, nullable = ${s.nullable}),`).join("\n");
    const build = specs.map((s, i) => (s.tag !== undefined ? null : `a[${i}] as ${s.kt}`)).filter(Boolean).join(", ");
    const get = specs.map((s) => (s.tag !== undefined ? `"${esc(s.tag)}"` : `v.${s.prop}`)).join(", ");
    if (!props.length) {
      const one = typeArgs ? `${cls}${typeArgs}()` : cls;
      return { ctor, adapter: `obj(\n            listOf(\n${fieldList}\n            ),\n            { ${one} },\n            { arrayOf(${get}) },\n        )` };
    }
    return { ctor, adapter: `obj(\n            listOf(\n${fieldList}\n            ),\n            { a -> ${cls}${typeArgs}(${build}) },\n            { v -> arrayOf(${get}) },\n        )` };
  }

  objCode(name, params, fields, ctx) {
    if (!fields.length && !params.length) {
      return `data object ${name} {\n    val ADAPTER: JsonAdapter<${name}> = obj(emptyList(), { ${name} }, { emptyArray() })\n}\n`;
    }
    const specs = this.fieldSpecs(fields, ctx);
    const tp = params.length ? `<${params.join(", ")}>` : "";
    const { ctor, adapter } = this.objBody(name, specs, tp);
    const head = `@Suppress("UNCHECKED_CAST")\ndata class ${name}${tp}(\n${ctor}\n) : Ordered`;
    if (!params.length) return `${head} {\n    override var wireOrder: IntArray? = null\n\n    companion object {\n        val ADAPTER: JsonAdapter<${name}> = ${adapter}\n    }\n}\n`;
    const ps = params.map((p) => `ad${p}: JsonAdapter<${p}>`).join(", ");
    return `${head} {\n    override var wireOrder: IntArray? = null\n\n    companion object {\n        fun ${tp} adapter(${ps}): JsonAdapter<${name}${tp}> = ${adapter}\n    }\n}\n`;
  }

  /** 一族的头：`sealed interface X<…>` 与它的编解码怎么开口（有类型参数 ⇒ 函数）。 */
  familyHead(name, params) {
    const tp = params.length ? `<${params.join(", ")}>` : "";
    const ps = params.map((p) => `ad${p}: JsonAdapter<${p}>`).join(", ");
    const open = params.length
      ? `fun ${tp} adapter(${ps}): JsonAdapter<${name}${tp}>`
      : `val ADAPTER: JsonAdapter<${name}>`;
    return { tp, open };
  }

  variantClass(name, tp, vname, specs, common = new Set(), tag = null) {
    // 判别格那个词（`tag` = [判别格的属性名, 词]）：读得到，不进构造。
    const tagLine = tag ? `        override val ${tag[0]}: String get() = "${esc(tag[1])}"\n` : "";
    if (!specs.length) {
      return tp
        ? `    class ${vname}${tp} : ${name}${tp} {\n${tagLine}\n        override fun equals(other: Any?): Boolean = other is ${name}.${vname}<*>\n\n        override fun hashCode(): Int = ${JSON.stringify(vname)}.hashCode()\n    }`
        : tag ? `    data object ${vname} : ${name} {\n${tagLine}    }` : `    data object ${vname} : ${name}`;
    }
    const ctor = specs.map((s) => `        ${common.has(s.prop) ? "override " : ""}val ${s.prop}: ${s.kt}${s.opt ? " = null" : ""},`).join("\n");
    return `    @Suppress("UNCHECKED_CAST")\n    data class ${vname}${tp}(\n${ctor}\n    ) : ${name}${tp}, Ordered {\n${tagLine}        override var wireOrder: IntArray? = null\n    }`;
  }

  sealedCode(name, params, disc, objs, ctx) {
    const { tp, open } = this.familyHead(name, params);
    const vn = (fs) => pascal(fs.find((f) => f.key === disc).ty.v) || "Empty";
    const shadow = familyShadow(name, objs.map(vn));
    const variants = objs.map((fs) => {
      const tag = fs.find((f) => f.key === disc).ty.v;
      const vname = vn(fs);
      const specs = this.fieldSpecs(fs.filter((f) => f.key !== disc), { ...ctx, owner: name + vname, shadow });
      const at = fs.findIndex((f) => f.key === disc);
      const all = [...specs.slice(0, at), { key: disc, tag }, ...specs.slice(at)];
      return { tag, vname, specs, all };
    });
    // 每一形都有、型也一样（可空与否可以不同）的格（通用记录的 `id` · `agent` …）：一族上直接读得到；有一形可空 ⇒ 一族上可空。
    const base = (kt) => kt.replace(/\?$/, "");
    const shared = variants.every((v) => v.specs.length)
      ? variants[0].specs
          .filter((s) => variants.every((v) => v.specs.some((x) => x.prop === s.prop && base(x.kt) === base(s.kt))))
          .map((s) => {
            const any = variants.some((v) => v.specs.find((x) => x.prop === s.prop).kt.endsWith("?"));
            return { ...s, kt: any ? base(s.kt) + "?" : s.kt };
          })
      : [];
    const common = new Set(shared.map((s) => s.prop));
    const dp = prop(disc);
    const code = [`sealed interface ${name}${tp} {`, `    /** 判别格（\`${esc(disc)}\`）那个词。 */\n    val ${dp}: String\n`, ...shared.map((s) => `    val ${s.prop}: ${s.kt}\n`)];
    const rows = [];
    for (const v of variants) {
      code.push(this.variantClass(name, tp, v.vname, v.specs, common, [dp, v.tag]));
      const ad = this.objBody(`${name}.${v.vname}`, v.all, tp).adapter;
      rows.push(`                Variant("${esc(v.tag)}", ${cast(`${name}.${v.vname}::class.java`, name, tp)}, ${ad.replace(/\n/g, "\n            ")}),`);
    }
    code.push(`\n    companion object {\n        ${open} =\n            tagged<${name}${tp}>(\n                "${esc(disc)}",\n                listOf(\n${rows.join("\n")}\n                ),\n            )\n    }\n}\n`);
    return code.join("\n");
  }

  externalCode(name, params, fields, ctx) {
    const { tp, open } = this.familyHead(name, params);
    const code = [`sealed interface ${name}${tp} {`];
    const rows = [];
    const shadow = familyShadow(name, fields.map((f) => pascal(f.key) || "Empty"));
    for (const f of fields) {
      const vname = pascal(f.key) || "Empty";
      const x = this.ty(f.ty, { ...ctx, owner: name + vname, shadow, field: "value", float: ctx.floats?.has(f.key), int: ctx.floats?.has("i:" + f.key), u64: ctx.floats?.has("u:" + f.key) });
      code.push(`    data class ${vname}${tp}(\n        val value: ${x.kt},\n    ) : ${name}${tp}`);
      rows.push(`                Ext("${esc(f.key)}", ${cast(`${name}.${vname}::class.java`, name, tp)}, { ${x.ad} }, { ${name}.${vname}${tp}(it as ${x.kt}) }, { (it as ${name}.${vname}${tp}).value }),`);
    }
    code.push(`\n    companion object {\n        ${open} =\n            external<${name}${tp}>(\n                listOf(\n${rows.join("\n")}\n                ),\n            )\n    }\n}\n`);
    return code.join("\n");
  }

  tokenCode(name, params, alts, toks, ctx) {
    const { tp, open } = this.familyHead(name, params);
    const VN = { STRING: "Text", NUMBER: "Num", BOOLEAN: "Flag", BEGIN_OBJECT: "Obj", BEGIN_ARRAY: "Items" };
    const code = [`sealed interface ${name}${tp} {`];
    const rows = [];
    const vn = (a, i) => (a.k === "ref" ? a.name : VN[toks[i]]);
    const shadow = familyShadow(name, alts.map(vn));
    alts.forEach((a, i) => {
      const vname = vn(a, i);
      const x = this.ty(a.k === "boollit" ? { k: "prim", v: "boolean" } : a, { ...ctx, owner: name + vname, shadow, field: "value" });
      code.push(`    data class ${vname}${tp}(\n        val value: ${x.kt},\n    ) : ${name}${tp}`);
      rows.push(`                Tok(JsonReader.Token.${toks[i]}, ${cast(`${name}.${vname}::class.java`, name, tp)}, { ${x.ad} }, { ${name}.${vname}${tp}(it as ${x.kt}) }, { (it as ${name}.${vname}${tp}).value }),`);
    });
    code.push(`\n    companion object {\n        ${open} =\n            byToken<${name}${tp}>(\n                listOf(\n${rows.join("\n")}\n                ),\n            )\n    }\n}\n`);
    return code.join("\n");
  }

  tupleCode(name, items, ctx) {
    const specs = items.map((t, i) => {
      const x = this.ty(t, { ...ctx, field: `p${i}` });
      return { prop: `p${i}`, kt: x.kt, ad: x.ad };
    });
    const ctor = specs.map((s) => `    val ${s.prop}: ${s.kt},`).join("\n");
    return `@Suppress("UNCHECKED_CAST")\ndata class ${name}(\n${ctor}\n) {\n    companion object {\n        val ADAPTER: JsonAdapter<${name}> = tuple(\n            listOf(${specs.map((s) => `{ ${s.ad} }`).join(", ")}),\n            { a -> ${name}(${specs.map((s, i) => `a[${i}] as ${s.kt}`).join(", ")}) },\n            { v -> arrayOf(${specs.map((s) => `v.${s.prop}`).join(", ")}) },\n        )\n    }\n}\n`;
  }
}

/** 带类型参数的一族：各形的类按擦除后的类认，转成这一族的那一个类型。 */
const cast = (cls, name, tp) => (tp ? `${cls} as Class<out ${name}${tp}>` : cls);

const esc = (s) => s.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\$/g, "\\$");

// ───────────────────────────── 夹具 ─────────────────────────────

/** 线上带格式约束、TS 里只是 string 的格 ⇒ 夹具里填一个合格的值（Rust 那头解的时候核）。 */
const FORMATS = { b16: "6869", sha256: "0".repeat(64) };

/** 一份结构夹具：每格都填（可空的填非空、可缺的也给），并集取第一形，数组 · 键集合各一项。 */
function sample(t, decls, floats, owner, field, depth = 0, env = {}) {
  if (depth > 40) return null;
  switch (t.k) {
    case "prim":
      // 线上带格式约束、TS 里只是 string 的那几格（[FORMATS]）。
      if (t.v === "string") return FORMATS[field] ?? (field ? `${field}-x` : "x");
      if (t.v === "boolean") return true;
      if (t.v === "number" || t.v === "bigint") return floats.get(owner)?.has(field) ? 1.5 : 7;
      if (t.v === "null") return null;
      return {};
    case "lit": return t.v;
    case "boollit": return t.v;
    case "numlit": return Number(t.v);
    case "array": return [sample(t.of, decls, floats, owner, field, depth + 1, env)];
    case "map": return { k: sample(t.of, decls, floats, owner, field, depth + 1, env) };
    case "tuple": return t.items.map((x, i) => sample(x, decls, floats, owner, `p${i}`, depth + 1, env));
    case "union": {
      const alts = t.alts.filter((a) => !(a.k === "prim" && a.v === "null"));
      return sample(alts[0], decls, floats, owner, field, depth + 1, env);
    }
    case "obj": {
      const o = {};
      for (const f of t.fields) o[f.key] = sample(f.ty, decls, floats, owner, f.key, depth + 1, env);
      return o;
    }
    case "inter": return Object.assign({}, ...t.parts.map((p) => sample(p, decls, floats, owner, field, depth + 1, env)));
    case "ref": {
      if (env[t.name]) return sample(env[t.name], decls, floats, owner, field, depth + 1, {});
      const d = decls.get(t.name);
      if (!d) return {};
      const e = {};
      d.params.forEach((p, i) => { const a = t.args[i]; e[p] = a && a.k === "ref" && env[a.name] ? env[a.name] : a; });
      return sample(d.ty, decls, floats, t.name, field, depth + 1, e);
    }
    default: return null;
  }
}

// ───────────────────────────── 主 ─────────────────────────────

// 生成的编解码共用的那几块（随生成物一起出，不另住一份手写的）。
const PREAMBLE = String.raw`import com.squareup.moshi.JsonDataException
import com.squareup.moshi.JsonReader
import com.squareup.moshi.JsonWriter

/** 线上原样的一段 JSON（声明里认不出的那几形）：读进来什么、写回去就是什么。 */
@JvmInline
value class Raw(val json: String) {
    /** 那一段读成 JSON 值（串 · 数 · 真假 · null · 列表 · 键值）。 */
    val value: Any? get() = JsonReader.of(okio.Buffer().writeUtf8(json)).readJsonValue()
}

/**
 * 一条命令（[Commands] 里每条一个，照 TypedCommands 生成）：线上名 · 入参类怎么写 · 回包类怎么解。
 * 手机发命令只经它取名字、出入参、解回包，不手写名字与键。
 */
abstract class Command<Q, R>(val name: String, req: () -> JsonAdapter<Q>, reply: () -> JsonAdapter<R>) {
    private val reqAd: JsonAdapter<Q> by lazy(req)
    private val replyAd: JsonAdapter<R> by lazy(reply)

    /** 入参出成交 CoreLink.call 的那一份：可缺的格（Rust 那头 skip_serializing_if）缺着就不交，可空的照写 null。 */
    @Suppress("UNCHECKED_CAST")
    fun args(req: Q): Map<String, Any?> = reqAd.serializeNulls().toJsonValue(req) as Map<String, Any?>

    /** 回包（核心交来的 data）照回包类解；缺格 · 型不对 · 认不出的词 ⇒ null（两端契约对不上，不补默认值）。 */
    fun reply(data: Any?): R? =
        try {
            replyAd.fromJsonValue(data)
        } catch (_: JsonDataException) {
            null
        }
}

/**
 * 对象的一格：线上键 · 它的编解码（头一回用到才取：各类的编解码互相引用，不按初始化先后）·
 * 可缺（写回时值为 null 就不写，Rust 那头 skip_serializing_if）· 可空（缺席照 null 读）。
 */
class Field(val key: String, ad: () -> JsonAdapter<*>, val optional: Boolean, val nullable: Boolean) {
    val ad: JsonAdapter<*> by lazy(ad)
}

/** 两端契约对不上（缺格 · 型不对 · 认不出的词）：说出是哪一格。 */
fun mismatch(what: String): JsonDataException = JsonDataException("wire contract mismatch: " + what)

private fun JsonReader.notNull(what: String) {
    if (peek() == JsonReader.Token.NULL) throw mismatch(what + " must not be null (at " + path + ")")
}

private inline fun <T> prim(
    crossinline read: (JsonReader) -> T,
    crossinline write: (JsonWriter, T) -> Unit,
    what: String,
): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            reader.notNull(what)
            return read(reader)
        }

        override fun toJson(writer: JsonWriter, value: T?) {
            write(writer, value ?: throw mismatch(what + " must not be null"))
        }
    }

val Str: JsonAdapter<String> = prim({ it.nextString() }, { w, v -> w.value(v) }, "string")
val Bool: JsonAdapter<Boolean> = prim({ it.nextBoolean() }, { w, v -> w.value(v) }, "boolean")
val Lng: JsonAdapter<Long> = prim({ it.nextLong() }, { w, v -> w.value(v) }, "integer")
val Int32: JsonAdapter<Int> = prim({ it.nextInt() }, { w, v -> w.value(v) }, "integer")
/** 无符号 64 位：照位存进 Long；读的时候不管它过没过 JSON 数（大于 Long 的照无符号读），写回照无符号的字。 */
val U64: JsonAdapter<Long> =
    prim({ it.nextString().toBigDecimal().toBigInteger().toLong() }, { w, v -> if (v >= 0) w.value(v) else w.valueSink().use { s -> s.writeUtf8(java.lang.Long.toUnsignedString(v)) } }, "integer")
val Dbl: JsonAdapter<Double> = prim({ it.nextDouble() }, { w, v -> w.value(v) }, "number")
/** 那一段原样（null 也是一段）。 */
val RawJson: JsonAdapter<Raw> =
    object : JsonAdapter<Raw>() {
        override fun fromJson(reader: JsonReader): Raw = reader.nextSource().use { Raw(it.readUtf8()) }

        override fun toJson(writer: JsonWriter, value: Raw?) {
            writer.valueSink().use { it.writeUtf8((value ?: throw mismatch("raw json must not be null")).json) }
        }
    }

/** 照原样留着、还没装进线上类型的那一段（带键集合手上那一份逐项存盘 · 用时才各按自己的类型解）。 */
val JsonValue: JsonAdapter<Any> =
    prim({ r -> r.readJsonValue() ?: throw mismatch("value must not be null (at " + r.path + ")") }, { w, v -> w.jsonValue(v) }, "value")

fun <T> opt(ad: JsonAdapter<T>): JsonAdapter<T?> =
    object : JsonAdapter<T?>() {
        override fun fromJson(reader: JsonReader): T? = if (reader.peek() == JsonReader.Token.NULL) reader.nextNull() else ad.fromJson(reader)

        override fun toJson(writer: JsonWriter, value: T?) {
            if (value == null) {
                val was = writer.serializeNulls
                writer.serializeNulls = true
                writer.nullValue()
                writer.serializeNulls = was
            } else {
                ad.toJson(writer, value)
            }
        }
    }

@Suppress("UNCHECKED_CAST")
fun <T> arr(ad: JsonAdapter<T>): JsonAdapter<List<T>> =
    prim({ r ->
        val out = ArrayList<T>()
        r.beginArray()
        while (r.hasNext()) out.add(ad.fromJson(r) as T)
        r.endArray()
        out
    }, { w, v ->
        w.beginArray()
        v.forEach { ad.toJson(w, it) }
        w.endArray()
    }, "array")

@Suppress("UNCHECKED_CAST")
fun <T> dict(ad: JsonAdapter<T>): JsonAdapter<Map<String, T>> =
    prim({ r ->
        val out = LinkedHashMap<String, T>()
        r.beginObject()
        while (r.hasNext()) out[r.nextName()] = ad.fromJson(r) as T
        r.endObject()
        out
    }, { w, v ->
        w.beginObject()
        v.forEach { (k, x) ->
            w.name(k)
            ad.toJson(w, x)
        }
        w.endObject()
    }, "map")

fun <E : Enum<E>> enumOf(all: List<E>, wire: (E) -> String): JsonAdapter<E> =
    prim({ r ->
        val s = r.nextString()
        all.firstOrNull { wire(it) == s } ?: throw mismatch("unknown word \"" + s + "\" (at " + r.path + ")")
    }, { w, v -> w.value(wire(v)) }, "enum word")

/**
 * 读进来的那一份记着各格到的先后，写回照它（ts-rs 导出时把 serde flatten 进来的格挪到了最后，声明里的先后不是 Rust 写的先后）；
 * 自己造的照声明的先后写（读的那一头不看先后）。不进 equals。
 */
interface Ordered {
    var wireOrder: IntArray?
}

/** 一个对象：读时认不得的格跳过，缺了非可空的格是契约对不上。 */
class ObjAd<T>(
    private val fields: List<Field>,
    private val build: (Array<Any?>) -> T,
    private val get: (T) -> Array<Any?>,
) : JsonAdapter<T>() {
    private val index = fields.withIndex().associate { (i, f) -> f.key to i }

    override fun fromJson(reader: JsonReader): T {
        reader.notNull("object")
        val a = arrayOfNulls<Any?>(fields.size)
        val seen = BooleanArray(fields.size)
        val order = ArrayList<Int>(fields.size)
        reader.beginObject()
        while (reader.hasNext()) {
            val i = index[reader.nextName()]
            if (i == null) {
                reader.skipValue()
            } else {
                a[i] = fields[i].ad.fromJson(reader)
                if (!seen[i]) order.add(i)
                seen[i] = true
            }
        }
        reader.endObject()
        fields.forEachIndexed { i, f ->
            if (!seen[i] && !f.nullable) throw mismatch("missing field \"" + f.key + "\" (at " + reader.path + ")")
        }
        val built = build(a)
        (built as? Ordered)?.wireOrder = order.toIntArray()
        return built
    }

    override fun toJson(writer: JsonWriter, value: T?) {
        writer.beginObject()
        writeFields(writer, value ?: throw mismatch("object must not be null"))
        writer.endObject()
    }

    @Suppress("UNCHECKED_CAST")
    fun writeFields(writer: JsonWriter, value: T) {
        val v = get(value)
        val read = (value as? Ordered)?.wireOrder
        val order = if (read == null) fields.indices.toList() else read.toList() + fields.indices.filter { it !in read }
        for (i in order) {
            val f = fields[i]
            if (v[i] == null && (f.optional || read != null && i !in read)) continue
            writer.name(f.key)
            (f.ad as JsonAdapter<Any?>).toJson(writer, v[i])
        }
    }
}

fun <T> obj(fields: List<Field>, build: (Array<Any?>) -> T, get: (T) -> Array<Any?>): ObjAd<T> = ObjAd(fields, build, get)

/** 判别格那一格：读时只认那一个词，写时写它。 */
fun tag(word: String): JsonAdapter<String> =
    prim({ r ->
        val s = r.nextString()
        if (s != word) throw mismatch("tag should be \"" + word + "\", got \"" + s + "\" (at " + r.path + ")")
        s
    }, { w, _ -> w.value(word) }, "tag")

/** 带判别格的并集的一形：判别格的那个词 · 它的类 · 其余各格。 */
class Variant<T : Any>(val tag: String, val cls: Class<out T>, val ad: ObjAd<out T>)

/** 带判别格的并集（serde 的 internally tagged）：判别格是各形里的一格（位置照读进来的 · 照声明）。 */
fun <T : Any> tagged(disc: String, variants: List<Variant<T>>): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            reader.notNull("union")
            val tag =
                reader.peekJson().use { p ->
                    p.beginObject()
                    var t: String? = null
                    while (p.hasNext()) {
                        if (p.nextName() == disc) {
                            t = p.nextString()
                            break
                        }
                        p.skipValue()
                    }
                    t
                } ?: throw mismatch("missing tag \"" + disc + "\" (at " + reader.path + ")")
            val v = variants.firstOrNull { it.tag == tag } ?: throw mismatch("unknown variant \"" + tag + "\" (at " + reader.path + ")")
            return v.ad.fromJson(reader) as T
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("union must not be null")
            val v = variants.first { it.cls.isInstance(x) }
            (v.ad as ObjAd<T>).toJson(writer, x)
        }
    }

/** 每一形一个键的并集（serde 的 externally tagged）：{"那个键": 值}。 */
class Ext<T : Any>(val key: String, val cls: Class<out T>, ad: () -> JsonAdapter<*>, val wrap: (Any?) -> T, val unwrap: (T) -> Any?) {
    val ad: JsonAdapter<*> by lazy(ad)
}

fun <T : Any> external(variants: List<Ext<T>>): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            reader.notNull("union")
            reader.beginObject()
            val key = reader.nextName()
            val v = variants.firstOrNull { it.key == key } ?: throw mismatch("unknown variant \"" + key + "\" (at " + reader.path + ")")
            val x = v.wrap(v.ad.fromJson(reader))
            reader.endObject()
            return x
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("union must not be null")
            val v = variants.first { it.cls.isInstance(x) }
            writer.beginObject()
            writer.name(v.key)
            (v.ad as JsonAdapter<Any?>).toJson(writer, v.unwrap(x))
            writer.endObject()
        }
    }

/** 各形按 JSON 记号分得开的并集（serde 的 untagged：串 · 数 · 真假 · 对象 · 数组各至多一形）。 */
class Tok<T : Any>(val token: JsonReader.Token, val cls: Class<out T>, ad: () -> JsonAdapter<*>, val wrap: (Any?) -> T, val unwrap: (T) -> Any?) {
    val ad: JsonAdapter<*> by lazy(ad)
}

fun <T : Any> byToken(variants: List<Tok<T>>): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            val tok = reader.peek()
            val v = variants.firstOrNull { it.token == tok } ?: throw mismatch("unexpected " + tok + " (at " + reader.path + ")")
            return v.wrap(v.ad.fromJson(reader))
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("union must not be null")
            val v = variants.first { it.cls.isInstance(x) }
            (v.ad as JsonAdapter<Any?>).toJson(writer, v.unwrap(x))
        }
    }

/** 定长数组（Rust 的元组）。 */
fun <T> tuple(lazyAds: List<() -> JsonAdapter<*>>, build: (Array<Any?>) -> T, get: (T) -> Array<Any?>): JsonAdapter<T> =
    prim({ r ->
        val ads = lazyAds.map { it() }
        val a = arrayOfNulls<Any?>(ads.size)
        r.beginArray()
        ads.forEachIndexed { i, ad -> a[i] = ad.fromJson(r) }
        r.endArray()
        build(a)
    }, { w, v ->
        val a = get(v)
        val ads = lazyAds.map { it() }
        w.beginArray()
        @Suppress("UNCHECKED_CAST")
        ads.forEachIndexed { i, ad -> (ad as JsonAdapter<Any?>).toJson(w, a[i]) }
        w.endArray()
    }, "tuple")
`;


/** 每个导出的线上类型都出一份（回包 · 帧 · 入参 · 手机核心交的事件，读哪一个都照同一份）；值表那两份不是类型。 */
function emitAll(decls, floats) {
  const g = new Gen(decls, floats);
  for (const d of [...decls.values()].sort((a, b) => a.name.localeCompare(b.name))) if (d.name !== "TypedCommands") g.emit(d);
  return g;
}

const typesOf = (g) => [...g.out.keys()].sort().map((k) => g.out.get(k)).join("\n");

/** 全部类型的 Kotlin（Generated.kt 里前言之后那一段）。 */
export function kotlinTypes(decls, floats) {
  return typesOf(emitAll(decls, floats));
}

/** `judgment-rules.ts` 原文 ⇒ 本机那台机器键的 Kotlin 常量（契约常量住 `cmd_wire::machine::LOCAL`）；原文里没有 ⇒ 报错。 */
export function localConst(text) {
  const m = stripComments(text).match(/^export const LOCAL\s*=\s*("(?:[^"\\]|\\.)*")\s*;/m);
  if (!m) throw new Error("judgment-rules.ts 里没有 `export const LOCAL = \"…\";`（cmd_wire::machine::LOCAL 没导出）");
  return `/** 本机那台的机器键（\`cmd_wire::machine::LOCAL\`）。 */\nconst val LOCAL: String = "${esc(JSON.parse(m[1]))}"\n`;
}

function main() {
  const decls = loadDecls();
  const cmds = loadCommands();
  const floats = rustFloatFields();
  const phone = phoneCommands();
  if (process.argv.includes("--samples")) {
    const out = {};
    for (const c of phone) out[c] = sample(cmds.get(c).reply, decls, floats, "", "");
    process.stdout.write(JSON.stringify(out, null, 2) + "\n");
    return;
  }
  const g = emitAll(decls, floats);
  const table = [];
  for (const c of phone) {
    const x = g.ty(cmds.get(c).reply, { owner: pascal(c) + "Reply" });
    table.push(`        "${c}" to ${x.ad},`);
  }
  // 每条命令一个对象（桌面 TypedCommands 那一份，全数）。对象名与类型名可能同名（`history-read` ⇒ 命令 HistoryRead · 回包 HistoryRead），
  // Commands 里的类型（对象体 · REPLIES · BY_CLASS）一律写全名，不被同名的命令对象遮住。
  const KT_BUILTIN = new Set(["String", "Boolean", "Long", "Int", "Double", "Float", "List", "Map", "Any", "Unit", "ADAPTER"]);
  const fq = (s) => s.replace(/(?<![\w.])([A-Z]\w*)/g, (m) => (KT_BUILTIN.has(m) ? m : `${PKG}.${m}`));
  const commands = [];
  for (const [c, { req, reply }] of [...cmds.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    const q = g.ty(req, { owner: pascal(c) + "Req" });
    const r = g.ty(reply, { owner: pascal(c) + "Reply" });
    commands.push(
      `    /** \`${c}\`。 */\n    object ${pascal(c)} : Command<${fq(q.kt)}, ${fq(r.kt)}>("${c}", { ${fq(q.ad)} }, { ${fq(r.ad)} }) {\n        const val NAME: String = "${c}"\n    }\n`,
    );
  }
  const byClass = [...g.out.keys()].filter((k) => /\n {4}(?:val ADAPTER|companion object \{\n {8}val ADAPTER)/.test(g.out.get(k)) || /^data object /m.test(g.out.get(k))).sort();
  const header = `// 生成的文件（tests/scripts/gen-kotlin-wire.mjs 读 ts-rs 导出的声明出的）。不要手改：改 Rust 那一份类型，再跑 \`npm run gen:types\`。\n@file:Suppress("ktlint", "detekt:all")\n\npackage ${PKG}\n\nimport com.squareup.moshi.JsonAdapter\n`;
  const types = typesOf(g);
  const local = localConst(readFileSync(join(GEN, "judgment-rules.ts"), "utf8"));
  writeFileSync(join(OUT_DIR, "Generated.kt"), `${header}\n${PREAMBLE}\n${local}\n${types}`);
  writeFileSync(
    join(OUT_DIR, "Commands.kt"),
    `${header}\n/** 每条命令（桌面 TypedCommands 那一份）：名字 · 入参类 · 回包类，按命令名取的 [ALL]；按类取编解码的 [BY_CLASS]。 */\nobject Commands {\n    /** 金样逐字节对过的那几条（\`cmd-wire/src/call.rs\` 那张 \`calls!\` 表）⇒ 回包怎么解。 */\n    val REPLIES: Map<String, JsonAdapter<*>> = mapOf(\n${table.map(fq).join("\n")}\n    )\n\n${commands.join("\n")}\n    /** 命令名 ⇒ 那一条。 */\n    val ALL: Map<String, Command<*, *>> by lazy {\n        listOf<Command<*, *>>(\n${[...cmds.keys()].sort().map((c) => `            ${pascal(c)},`).join("\n")}\n        ).associateBy { it.name }\n    }\n\n    /** 每个不带类型参数的类型 ⇒ 它的编解码（按类取，不经反射）。 */\n    val BY_CLASS: Map<Class<*>, JsonAdapter<*>> by lazy {\n        mapOf(\n${byClass.map((k) => `            ${fq(k)}::class.java to ${fq(k)}.ADAPTER,`).join("\n")}\n        )\n    }\n}\n`,
  );
  if (g.raw.length) process.stderr.write("原样（认不出的形）：\n  " + g.raw.join("\n  ") + "\n");
}

// 直接跑才生成（判据 import 它只取上面那几个函数）；经软链跑也认得出。
if (process.argv[1] && realpathSync(fileURLToPath(import.meta.url)) === realpathSync(process.argv[1])) main();
