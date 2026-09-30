// PN1b 选图（`设计/97 §7.2`）的**异源**判据：本仓的渲染器登记 / 诚实信号表，与 **vendored 上游 Rust 源码**逐项两向比。
// 〔P7 · `99 §1` V158「线上契约由上游给、本仓不手抄」〕原 G3 / G4（手抄的 TS 镜像逐键对拍 vendored 源码）退役：
//   `types.ts` 成了生成物（上游 schema ＋ 小程序自己的应答，`tests/panorama-engine/cli_tests.rs::the_frontend_types_are_generated_from_upstream_and_this_program`
//   逐字节对拍），没有手抄的镜像可比了。留下的 G1 / G2 比的不是类型，是本仓自己的两张表（渲染器 · 诚实信号那一行的人话）。
//
// 异源在哪：左边是本仓 TS（`RENDERERS` 的键、`HONESTY_CELLS` 的键），右边是 `src/panorama-engine/vendor/code-picture-core/src/diagram/*.rs`
// 里的 `enum` 变体与 `struct` 字段（按固定文件名读，不遍历目录）。
// 上游加一种形状 / 一格诚实信号而本仓没跟 ⇒ 红；本仓多写一个上游没有的 ⇒ 也红。
//
// 🔴 被判对象零 mock：本文件不 mock 任何模块。
// **买不到**：线上 JSON 真长这样 —— 那一半在全景小程序的真引擎判据（`tests/panorama-engine/cli_tests.rs`
// 的 `every_op_runs_on_a_real_engine_over_a_synthetic_repo`）与上游自己的往返测试。
// 〔RM1f：原先点的是 monitor 那条（`the_diagram_commands_pass_the_upstream_through_untouched`〔散文墓碑〕），随内嵌引擎删了。〕
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { RENDERERS, legendFor, renderCallGraph, renderClusters, renderTypeGraph, layoutCallGraph } from "../../../../src/frontend/ui/panorama/diagram-render";
import { HONESTY_CELLS, honestyLine } from "../../../../src/frontend/ui/panorama/diagram-honesty";
import type { BodyOf } from "../../../../src/frontend/ui/panorama/diagram-render";
import * as fx from "./diagram-fixtures";

const VD = "src/panorama-engine/vendor/code-picture-core/src/diagram/";
const rust = (f: string): string => readFileSync(VD + f, "utf8");
const ALL_RUST = ["mod.rs", "registry.rs", "shape.rs", "uml.rs"].map(rust).join("\n");

/** `pub struct Name { ... }` / `pub enum Name { ... }` 的块体（到行首 `}` 为止）。读不到就抛 —— 不许退成空集。 */
function block(kind: "struct" | "enum", name: string): string {
  const m = new RegExp(`pub ${kind} ${name}\\b[^{]*\\{\\n([\\s\\S]*?)\\n\\}`, "m").exec(ALL_RUST);
  if (!m) throw new Error(`vendored 源码里找不到 pub ${kind} ${name} —— 上游改了名 / 搬了家，本判据要一起复核`);
  return m[1];
}

/** struct 的线上字段名（认 `#[serde(rename = "…")]`；`#[serde(skip)]` 的不上线、不算）。 */
function structFields(name: string): string[] {
  const out: string[] = [];
  let rename: string | null = null;
  let skip = false;
  for (const line of block("struct", name).split("\n")) {
    const r = /^\s*#\[serde\(rename = "(\w+)"\)\]/.exec(line);
    if (r) rename = r[1];
    if (/^\s*#\[serde\([^\]]*\b(skip|skip_serializing)\b/.test(line)) skip = true;
    const f = /^ {4}pub (\w+):/.exec(line);
    if (f) {
      if (!skip) out.push(rename ?? f[1]);
      rename = null;
      skip = false;
    }
  }
  return out.sort();
}

const snake = (s: string): string => s.replace(/[A-Z]/g, (c, i) => (i === 0 ? "" : "_") + c.toLowerCase());

/** enum 的变体名（`rename_all = "snake_case"` 之后的线上名）。 */
function enumVariants(name: string): string[] {
  return [...block("enum", name).matchAll(/^ {4}([A-Z]\w*)\b/gm)].map((m) => snake(m[1])).sort();
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
});

describe("PN1b 渲染器（纯函数，零 mock）", () => {
  it("R1 团/模块：每捆连接的线型按成分选，混着的不许画成干净的粗实线；标签写出成分", () => {
    const svg = renderClusters(fx.clustersDiagram.body as BodyOf<"clusters">, { onNode: () => {} });
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
    const svg = renderClusters(fx.clustersDiagram.body as BodyOf<"clusters">, {
      onNode: () => {},
      focusFile: "src/b/z.rs",
    });
    expect([...svg.querySelectorAll("[data-focus]")].map((n) => n.getAttribute("data-node"))).toEqual(["src_b"]);
  });

  it("R3 调用子图：中心第 0 列、它调的往右、调它的往左；按名字凑的边标「N 选 1」", () => {
    const body = fx.callDiagram.body as BodyOf<"call_graph">;
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
    const svg = renderTypeGraph(fx.typeDiagram.body as BodyOf<"type_graph">, {
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
