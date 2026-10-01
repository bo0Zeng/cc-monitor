/**
 * PN1b 选图的测试夹具：每种形状一张**结构上完整**的图（纯结构，不含任何真会话 / 真仓内容）。
 *
 * 🔴 每个对象字面量都**标注成 `types.ts` 里那个类型** ⇒ tsc 的多余属性检查 ＋ 必填检查保证夹具与线上形状同形
 * （`types.ts` 是上游 schema 的生成物，不再是手抄镜像）。
 *
 * ⚠ 图种 id 故意用假的（`k-…`）：本仓不许写上游图种名，夹具也不借真名蒙混。
 */
import type {
  ArchLink,
  ArchNode,
  CallEdge,
  CallNode,
  Diagram,
  DiagramKindInfo,
  Honesty,
  Omitted,
  PanoramaDiagram,
  TypeNode,
  TypeRelation,
} from "../../../../src/frontend/ui/panorama/types";

export const omitted: Omitted = { nodes: 2, symbols: 9, links: 3 };

export const honestyFull: Honesty = {
  unresolved_calls: 12,
  ambiguous_calls: 5,
  filtered_guess_links: 4,
  excluded_test_symbols: 7,
  omitted,
  db_errors: [],
};

export const honestyTypes: Honesty = {
  unresolved_calls: null,
  ambiguous_calls: null,
  filtered_guess_links: null,
  excluded_test_symbols: null,
  omitted: { nodes: 1, symbols: 2, links: 0 },
  db_errors: ["all_impls: boom"],
};

export const clusterA: ArchNode = {
  id: "src_a",
  label: "src/a",
  size: 10,
  files: 2,
  anchors: ["f"],
  member_files: ["src/a/x.rs", "src/a/y.rs"],
};
export const clusterB: ArchNode = {
  id: "src_b",
  label: "src/b",
  size: 4,
  files: 1,
  anchors: [],
  member_files: ["src/b/z.rs"],
};
export const clusterC: ArchNode = {
  id: "src_c",
  label: "src/c",
  size: 3,
  files: 1,
  anchors: [],
  member_files: ["src/c/w.rs"],
};
export const linkExact: ArchLink = { from: "src_a", to: "src_b", exact: 3, dispatch: 0, guess: 0 };
export const linkMixed: ArchLink = { from: "src_a", to: "src_c", exact: 2, dispatch: 0, guess: 1 };
export const linkGuess: ArchLink = { from: "src_b", to: "src_c", exact: 0, dispatch: 0, guess: 2 };

export const clustersDiagram: Diagram = {
  kind: "k-clusters",
  honesty: honestyFull,
  body: { shape: "clusters", nodes: [clusterA, clusterB, clusterC], links: [linkExact, linkMixed, linkGuess] },
};

export const callCenter: CallNode = { id: "src/a/x.rs#f", name: "f", file: "src/a/x.rs", kind: "Function", start_line: 1 };
export const callOut: CallNode = { id: "src/b/z.rs#g", name: "g", file: "src/b/z.rs", kind: "Function", start_line: 3 };
export const callIn: CallNode = { id: "src/c/w.rs#h", name: "h", file: "src/c/w.rs", kind: "Function", start_line: 5 };
export const edgeExact: CallEdge = { from: "src/a/x.rs#f", to: "src/b/z.rs#g", confidence: "Exact", candidates: null, call_site_line: 2 };
export const edgeGuess: CallEdge = { from: "src/c/w.rs#h", to: "src/a/x.rs#f", confidence: "DynamicGuess", candidates: 3, call_site_line: 6 };

export const callDiagram: Diagram = {
  kind: "k-calls",
  honesty: { ...honestyFull, excluded_test_symbols: null },
  body: { shape: "call_graph", center: "src/a/x.rs#f", depth: 2, nodes: [callCenter, callOut, callIn], edges: [edgeExact, edgeGuess] },
};

export const typeA: TypeNode = {
  id: "A",
  name: "A",
  symbol: "src/a/x.rs#A",
  fields: [{ name: "b", ty: "B" }],
  methods: [{ name: "go", symbol: "src/a/x.rs#A::go" }],
};
export const typeB: TypeNode = { id: "B", name: "B", symbol: null, fields: [], methods: [] };
export const relCompose: TypeRelation = { from: "A", to: "B", kind: "composes", label: "b" };
export const relImpl: TypeRelation = { from: "B", to: "A", kind: "implements", label: null };

export const typeDiagram: Diagram = {
  kind: "k-types",
  honesty: honestyTypes,
  body: { shape: "type_graph", types: [typeA, typeB], relations: [relCompose, relImpl] },
};

/**
 * 一种**这一版不认识的形状**（上游将来加的那种）。`types.ts` 成了生成物，图体是这一版的闭集 ⇒ 线上那份「认不出的」
 * 只能强转着造（界面走「这一版还画不出」那条路，判据看的是运行时，不是类型）。
 */
export const unknownShapeDiagram: Diagram = {
  kind: "k-new",
  honesty: honestyFull,
  body: { shape: "hexagon" } as unknown as Diagram["body"],
};

export const kinds: DiagramKindInfo[] = [
  { id: "k-clusters", title: "甲图", summary: "团", params: ["max_nodes", "certain_only", "exclude_tests"], shape: "clusters" },
  { id: "k-calls", title: "乙图", summary: "调用", params: ["symbol", "depth", "certain_only"], shape: "call_graph" },
  { id: "k-types", title: "丙图", summary: "类型", params: ["max_nodes"], shape: "type_graph" },
  { id: "k-new", title: "丁图", summary: "新形状", params: [], shape: "hexagon" },
];

export const view = (d: Diagram): PanoramaDiagram => ({ diagram: d, mermaid: `flowchart LR\n  %% ${d.kind}\n` });
