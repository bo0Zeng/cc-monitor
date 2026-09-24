// PN1b 选图（`设计/97 §7.2`）的**异源**判据：本仓的渲染器登记 / 诚实信号表 / TS 镜像类型，
// 与 **vendored 上游 Rust 源码**逐项两向比。
//
// 异源在哪：左边是本仓 TS（`RENDERERS` 的键、`HONESTY_CELLS` 的键、夹具的键 —— 夹具经 tsc
// 钉成与 `types.ts` 镜像逐键相同），右边是 `src/bridge/vendor/code-picture-core/src/diagram/*.rs`
// 里的 `enum` 变体与 `struct` 字段（按固定文件名读，不遍历目录）。
// 上游加一种形状 / 一格诚实信号 / 一个字段而本仓没跟 ⇒ 红；本仓多写一个上游没有的 ⇒ 也红。
//
// 🔴 被判对象零 mock：本文件不 mock 任何模块。
// **买不到**：线上 JSON 真长这样 —— 那一半在 `tests/bridge/panorama_tests.rs` 的真引擎判据
// （`the_diagram_commands_pass_the_upstream_through_untouched`）与上游自己的往返测试。
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { RENDERERS, legendFor, renderCallGraph, renderClusters, renderTypeGraph, layoutCallGraph } from "../../src/panorama/diagram-render";
import { HONESTY_CELLS, honestyLine } from "../../src/panorama/diagram-honesty";
import type { CallGraphBody, ClustersBody, TypeGraphBody } from "../../src/panorama/types";
import * as fx from "./diagram-fixtures";

const VD = "src/bridge/vendor/code-picture-core/src/diagram/";
const rust = (f: string): string => readFileSync(VD + f, "utf8");
const ALL_RUST = ["mod.rs", "registry.rs", "shape.rs", "uml.rs"].map(rust).join("\n");

/** `pub struct Name { ... }` / `pub enum Name { ... }` 的块体（到行首 `}` 为止）。读不到就抛 —— 不许退成空集。 */
function block(kind: "struct" | "enum", name: string): string {
  const m = new RegExp(`pub ${kind} ${name}\\b[^{]*\\{\\n([\\s\\S]*?)\\n\\}`, "m").exec(ALL_RUST);
  if (!m) throw new Error(`vendored 源码里找不到 pub ${kind} ${name} —— 上游改了名 / 搬了家，本判据要一起复核`);
  return m[1];
}

/** struct 的线上字段名（认 `#[serde(rename = "…")]`）。 */
function structFields(name: string): string[] {
  const out: string[] = [];
  let rename: string | null = null;
  for (const line of block("struct", name).split("\n")) {
    const r = /^\s*#\[serde\(rename = "(\w+)"\)\]/.exec(line);
    if (r) rename = r[1];
    const f = /^ {4}pub (\w+):/.exec(line);
    if (f) {
      out.push(rename ?? f[1]);
      rename = null;
    }
  }
  return out.sort();
}

const snake = (s: string): string => s.replace(/[A-Z]/g, (c, i) => (i === 0 ? "" : "_") + c.toLowerCase());

/** enum 的变体名（snake_case，即 `rename_all = "snake_case"` 之后的线上名）。 */
function enumVariants(name: string): string[] {
  return [...block("enum", name).matchAll(/^ {4}([A-Z]\w*)\b/gm)].map((m) => snake(m[1])).sort();
}

/** `DiagramBody` 某个结构体变体的字段。 */
function bodyVariantFields(variant: string): string[] {
  const m = new RegExp(`^ {4}${variant} \\{\\n([\\s\\S]*?)^ {4}\\},`, "m").exec(block("enum", "DiagramBody"));
  if (!m) throw new Error(`DiagramBody::${variant} 找不到`);
  return [...m[1].matchAll(/^ {8}(\w+):/gm)].map((x) => x[1]).sort();
}

const keys = (o: object): string[] => Object.keys(o).sort();

describe("PN1b 选图：本仓与上游两向相等（异源）", () => {
  it("G1 形状 → 渲染器登记 == 上游 DiagramBody 的变体 == 上游 DiagramShape；每种形状都有图例", () => {
    const body = enumVariants("DiagramBody");
    expect(body).toEqual(["call_graph", "clusters", "type_graph"]); // 正控：解析真的解出了东西
    expect(keys(RENDERERS)).toEqual(body);
    expect(enumVariants("DiagramShape")).toEqual(body);
    expect(keys(RENDERERS).filter((s) => legendFor(s).length === 0)).toEqual([]);
  });

  it("G2 诚实信号那一行的格子 == 上游 Honesty 的字段", () => {
    expect(keys(HONESTY_CELLS)).toEqual(structFields("Honesty"));
    expect(structFields("Honesty").length).toBe(6); // 正控
  });

  it("G3 TS 镜像类型（经夹具 + tsc）== 上游结构体字段", () => {
    const pairs: [string, object][] = [
      ["Diagram", fx.clustersDiagram],
      ["Honesty", fx.honestyFull],
      ["Omitted", fx.omitted],
      ["ArchNode", fx.clusterA],
      ["ArchLink", fx.linkExact],
      ["CallNode", fx.callCenter],
      ["CallEdge", fx.edgeExact],
      ["TypeNode", fx.typeA],
      ["TypeField", fx.typeA.fields[0]],
      ["TypeMethod", fx.typeA.methods[0]],
      ["TypeRelation", fx.relCompose],
      ["DiagramKindInfo", fx.kinds[0]],
    ];
    const diff = pairs
      .map(([name, sample]) => [name, keys(sample), structFields(name)] as const)
      .filter(([, ts, rs]) => JSON.stringify(ts) !== JSON.stringify(rs));
    expect(diff).toEqual([]);
    // 图体三个变体：TS 镜像的键 == 上游变体字段 ＋ 形状标签
    const variants: [string, object][] = [
      ["Clusters", fx.clustersDiagram.body],
      ["CallGraph", fx.callDiagram.body],
      ["TypeGraph", fx.typeDiagram.body],
    ];
    for (const [v, sample] of variants) {
      expect(keys(sample), v).toEqual([...bodyVariantFields(v), "shape"].sort());
    }
  });
});

describe("PN1b 渲染器（纯函数，零 mock）", () => {
  it("R1 团/模块：每捆连接的线型按成分选，混着的不许画成干净的粗实线；标签写出成分", () => {
    const svg = renderClusters(fx.clustersDiagram.body as ClustersBody, { onNode: () => {} });
    const edges = [...svg.querySelectorAll("[data-edge]")];
    expect(edges.map((e) => e.getAttribute("data-conf"))).toEqual(["exact", "mixed", "guess"]);
    expect(edges.map((e) => e.querySelector("text")?.textContent)).toEqual([
      "3×",
      "3×（2 确定+1 分不清）",
      "2×分不清?",
    ]);
    expect([...svg.querySelectorAll("[data-node]")].map((n) => n.getAttribute("data-node"))).toEqual([
      "src_a",
      "src_b",
      "src_c",
    ]);
  });

  it("R2 团/模块：选中的文件 ⇒ 含它的那个节点（且只那一个）描环；数据来自上游 member_files", () => {
    const svg = renderClusters(fx.clustersDiagram.body as ClustersBody, {
      onNode: () => {},
      focusFile: "src/b/z.rs",
    });
    expect([...svg.querySelectorAll("[data-focus]")].map((n) => n.getAttribute("data-node"))).toEqual(["src_b"]);
  });

  it("R3 调用子图：中心第 0 列、它调的往右、调它的往左；按名字凑的边标「N 选 1」", () => {
    const body = fx.callDiagram.body as CallGraphBody;
    const { column } = layoutCallGraph(body);
    expect([...column.entries()].sort()).toEqual([
      ["src/a/x.rs#f", 0],
      ["src/b/z.rs#g", 1],
      ["src/c/w.rs#h", -1],
    ]);
    const svg = renderCallGraph(body, { onNode: () => {} });
    const edges = [...svg.querySelectorAll("[data-edge]")];
    expect(edges.map((e) => e.getAttribute("data-conf"))).toEqual(["exact", "guess"]);
    expect(edges.map((e) => e.querySelector("text")?.textContent ?? "")).toEqual(["", "分不清? 3 选 1"]);
    // 中心描环
    expect([...svg.querySelectorAll("[data-focus]")].map((n) => n.getAttribute("data-node"))).toEqual([
      "src/a/x.rs#f",
    ]);
  });

  it("R4 类图：关系按种类（实现 / 组合）画；点节点把类型交给调用方", () => {
    const picked: string[] = [];
    const svg = renderTypeGraph(fx.typeDiagram.body as TypeGraphBody, {
      onNode: (p) => picked.push(`${p.shape}:${p.shape === "type_graph" ? p.node.name : ""}`),
    });
    expect([...svg.querySelectorAll("[data-edge]")].map((e) => e.getAttribute("data-conf"))).toEqual([
      "composes",
      "implements",
    ]);
    (svg.querySelector('[data-node="A"]') as SVGGElement).dispatchEvent(new MouseEvent("click"));
    expect(picked).toEqual(["type_graph:A"]);
  });

  it("R5 诚实信号那一行：六格全写，null 写「不适用」不写 0", () => {
    expect(honestyLine(fx.honestyFull)).toBe(
      "看不见 12 处调用 · 分不清 5 处调用 · 滤掉 4 条全靠名字凑的连接 · 排除 7 个测试符号 · " +
        "省略 2 个节点（9 个符号）、3 条连接 · 读索引没出错",
    );
    expect(honestyLine(fx.honestyTypes)).toBe(
      "看不见：不适用 · 分不清：不适用 · 滤掉：不适用 · 排除：不适用 · " +
        "省略 1 个节点（2 个符号）、0 条连接 · 读索引出错 1 处，这张图不完整",
    );
  });
});
