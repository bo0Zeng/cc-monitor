/**
 * PN1b（`设计/97 §7.2`）：**按形状**渲染上游的 `Diagram` —— 一种形状一个渲染器，接口一致。
 *
 * ## 为什么按形状、不按图种
 * 图种（模块图 / 社区图 / 调用子图 / 类图 …）由上游注册表决定，本仓**一个名字都不写**。
 * 形状（`clusters` / `call_graph` / `type_graph`）才是渲染真正依赖的东西。
 * ⇒ 上游加一种**已有形状**的新图：本仓零改动就能画；加一种**新形状**：
 *   [`rendererFor`] 返回 `null`，界面如实说「这一版还画不出」并给「复制 Mermaid」。
 *
 * ## CP1：这里不算任何图分析
 * 节点、连接、每捆连接的**成分**、每条边的可信度全来自上游。这里只做两件事：
 * **摆位置**（确定性的简单布局：圆周 / 按跳数分列 / 网格）与**选线型**（成分 → 线型，
 * 与上游 Mermaid 渲染同一套语义）。**不解析 Mermaid**、不判边、不聚合、不滤。
 *
 * ## 不引布局库（理由见 `设计/97 §7.2`）
 * 一张图默认 ≤12 个节点，调用子图几十个；dagre / elk 那一档依赖换来的是用不上的规模。
 *
 * 买到：三种形状各有一个渲染器，线型按可信度区分，节点可点（下钻由调用方接）。
 * **买不到**：大图会挤（节点上限就是为这个留的）；没有拖拽 / 缩放；真机 WebView2 的性能没量。
 */
import type {
  CallEdge,
  CallGraphBody,
  CallNode,
  ClusterLink,
  ClusterNode,
  ClustersBody,
  Confidence,
  DiagramBody,
  TypeGraphBody,
  TypeNode,
} from "./types";

const SVG_NS = "http://www.w3.org/2000/svg";

/** 线型档（写进 `data-conf`，CSS 按它画）。 */
export type LineConf = "exact" | "dispatch" | "guess" | "mixed";

/** 节点被点时交给调用方的东西（下钻由调用方决定）。 */
export type NodePick =
  | { shape: "clusters"; node: ClusterNode }
  | { shape: "call_graph"; node: CallNode }
  | { shape: "type_graph"; node: TypeNode };

export interface RenderContext {
  onNode: (pick: NodePick) => void;
  /** 选中的文件：团/模块图上**包含它的节点**描环（数据来自上游 `member_files`）。 */
  focusFile?: string | null;
}

/** 渲染器接口：吃那个形状的图体，吐一张 SVG。 */
export type ShapeRenderer<B> = (body: B, ctx: RenderContext) => SVGSVGElement;

/**
 * **形状 → 渲染器**登记表。键就是上游 `DiagramShape` 的线上名；
 * 判据：键集合 == vendored 源码里 `DiagramBody` 的变体集合（两向）。
 */
export const RENDERERS: Record<string, ShapeRenderer<never>> = {
  clusters: renderClusters as ShapeRenderer<never>,
  call_graph: renderCallGraph as ShapeRenderer<never>,
  type_graph: renderTypeGraph as ShapeRenderer<never>,
};

/** 这一版能不能画这种形状。`null` = 画不出（调用方走「画不出 + 复制 Mermaid」那条路）。 */
export function rendererFor(shape: string): ShapeRenderer<DiagramBody> | null {
  return Object.prototype.hasOwnProperty.call(RENDERERS, shape)
    ? (RENDERERS[shape] as ShapeRenderer<DiagramBody>)
    : null;
}

// === 线型（与上游 Mermaid 渲染同一套语义）===

/** 一条符号级边的线型档：确定 / 动态派发 / 按名字凑。 */
export function edgeConf(c: Confidence): LineConf {
  switch (c) {
    case "Exact":
      return "exact";
    case "Dispatch":
      return "dispatch";
    case "Heuristic":
    case "DynamicGuess":
      return "guess";
  }
}

/** 一条符号级边的标签：「N 选 1」要印在脸上（上游要求）。 */
export function edgeLabel(e: CallEdge): string {
  const base = e.confidence === "Dispatch" ? "派发" : edgeConf(e.confidence) === "guess" ? "分不清?" : "";
  if (base && e.candidates != null) return `${base} ${e.candidates} 选 1`;
  return base;
}

/**
 * 一捆聚合连接的线型档。🔴 **混着的不许画成干净的粗实线** ——
 * 全确定才给 exact；一掺就是 mixed（细实线 ＋ 标签写出成分）。
 */
export function linkConf(l: ClusterLink): LineConf {
  const parts = [l.exact > 0, l.dispatch > 0, l.guess > 0].filter(Boolean).length;
  if (parts !== 1) return "mixed";
  if (l.exact > 0) return "exact";
  return l.dispatch > 0 ? "dispatch" : "guess";
}

/** 一捆聚合连接的标签：成分写出来。 */
export function linkLabel(l: ClusterLink): string {
  const total = l.exact + l.dispatch + l.guess;
  switch (linkConf(l)) {
    case "exact":
      return `${total}×`;
    case "dispatch":
      return `${total}×派发`;
    case "guess":
      return `${total}×分不清?`;
    case "mixed": {
      const parts: string[] = [];
      if (l.exact > 0) parts.push(`${l.exact} 确定`);
      if (l.dispatch > 0) parts.push(`${l.dispatch} 派发`);
      if (l.guess > 0) parts.push(`${l.guess} 分不清`);
      return `${total}×（${parts.join("+")}）`;
    }
  }
}

// === 布局（纯函数，确定性）===

export interface Box {
  id: string;
  x: number; // 中心
  y: number;
  w: number;
  h: number;
}

const PAD = 24;

/** 标签宽度的粗估（CJK 按两格）。只为摆位置，不追求像素准。 */
export function textWidth(s: string, px = 12): number {
  let units = 0;
  for (const ch of s) units += ch.charCodeAt(0) > 0x2e80 ? 2 : 1.1;
  return Math.ceil(units * px * 0.55);
}

/** 团/模块图：节点摆在一个圆上（按上游给的顺序，确定性）。 */
export function layoutClusters(nodes: ClusterNode[]): { boxes: Box[]; width: number; height: number } {
  const n = nodes.length;
  const sizes = nodes.map((nd) => ({
    w: Math.min(220, Math.max(96, textWidth(nd.label) + 28)),
    h: 44,
  }));
  const maxW = Math.max(96, ...sizes.map((s) => s.w));
  const r = n <= 1 ? 0 : Math.max(120, (n * (maxW * 0.62)) / Math.PI);
  const cx = r + maxW / 2 + PAD;
  const cy = r + 22 + PAD;
  const boxes = nodes.map((nd, i) => {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / Math.max(1, n);
    return { id: nd.id, x: cx + r * Math.cos(a), y: cy + r * Math.sin(a), ...sizes[i] };
  });
  return { boxes, width: 2 * cx, height: 2 * cy };
}

/**
 * 调用子图：以中心为第 0 列，顺调用方向往右（它调的）、逆调用方向往左（调它的）按跳数分列。
 * 这是**摆位置**（离中心几跳就放第几列），不是分析：边与节点一条不增不减。
 * 两个方向都够不着的节点（上游带进来的歧义候选端点）放在第 0 列中心下方。
 */
export function layoutCallGraph(body: Pick<CallGraphBody, "center" | "nodes" | "edges">): {
  boxes: Box[];
  width: number;
  height: number;
  column: Map<string, number>;
} {
  const out = new Map<string, string[]>();
  const inn = new Map<string, string[]>();
  for (const e of body.edges) {
    (out.get(e.from) ?? out.set(e.from, []).get(e.from)!).push(e.to);
    (inn.get(e.to) ?? inn.set(e.to, []).get(e.to)!).push(e.from);
  }
  const column = new Map<string, number>([[body.center, 0]]);
  const walk = (adj: Map<string, string[]>, sign: number): void => {
    let frontier = [body.center];
    let d = 0;
    const seen = new Set([body.center]);
    while (frontier.length > 0) {
      d += 1;
      const next: string[] = [];
      for (const id of frontier) {
        for (const t of adj.get(id) ?? []) {
          if (seen.has(t)) continue;
          seen.add(t);
          if (!column.has(t)) column.set(t, sign * d);
          next.push(t);
        }
      }
      frontier = next;
    }
  };
  walk(out, 1);
  walk(inn, -1);
  for (const n of body.nodes) if (!column.has(n.id)) column.set(n.id, 0);
  const cols = [...new Set(column.values())].sort((a, b) => a - b);
  const colW = 210;
  const rowH = 64;
  const byCol = new Map<number, CallNode[]>();
  for (const n of body.nodes) {
    const c = column.get(n.id) ?? 0;
    (byCol.get(c) ?? byCol.set(c, []).get(c)!).push(n);
  }
  // 中心放第 0 列最上面，其余按 id 排（确定性）
  for (const list of byCol.values()) {
    list.sort((a, b) =>
      a.id === body.center ? -1 : b.id === body.center ? 1 : a.id.localeCompare(b.id),
    );
  }
  const tallest = Math.max(1, ...[...byCol.values()].map((l) => l.length));
  const boxes: Box[] = [];
  for (const [c, list] of byCol) {
    const xi = cols.indexOf(c);
    list.forEach((n, i) => {
      boxes.push({
        id: n.id,
        x: PAD + colW / 2 + xi * colW,
        y: PAD + rowH / 2 + i * rowH,
        w: Math.min(190, Math.max(90, textWidth(n.name) + 28)),
        h: 40,
      });
    });
  }
  return { boxes, width: PAD * 2 + cols.length * colW, height: PAD * 2 + tallest * rowH, column };
}

/** 类图：网格（按上游给的顺序）。 */
export function layoutTypeGraph(types: TypeNode[]): { boxes: Box[]; width: number; height: number } {
  const n = types.length;
  const cols = Math.max(1, Math.ceil(Math.sqrt(n)));
  const cellW = 230;
  const cellH = 150;
  const boxes = types.map((t, i) => ({
    id: t.id,
    x: PAD + cellW / 2 + (i % cols) * cellW,
    y: PAD + cellH / 2 + Math.floor(i / cols) * cellH,
    w: 190,
    h: 28 + 16 * Math.min(TYPE_MEMBER_LINES, t.fields.length + t.methods.length),
  }));
  const rows = Math.max(1, Math.ceil(n / cols));
  return { boxes, width: PAD * 2 + cols * cellW, height: PAD * 2 + rows * cellH };
}

const TYPE_MEMBER_LINES = 5;

/** 从 a 的中心到 b 的中心，两端各截到矩形边上。 */
export function clipSegment(a: Box, b: Box): { x1: number; y1: number; x2: number; y2: number } {
  const cut = (from: Box, to: Box): { x: number; y: number } => {
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    if (dx === 0 && dy === 0) return { x: from.x, y: from.y };
    const t = Math.min(
      dx === 0 ? Infinity : from.w / 2 / Math.abs(dx),
      dy === 0 ? Infinity : from.h / 2 / Math.abs(dy),
    );
    return { x: from.x + dx * t, y: from.y + dy * t };
  };
  const s = cut(a, b);
  const e = cut(b, a);
  return { x1: s.x, y1: s.y, x2: e.x, y2: e.y };
}

// === SVG ===

function svgEl<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Record<string, string | number> = {},
  text?: string,
): SVGElementTagNameMap[K] {
  const e = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  if (text !== undefined) e.textContent = text;
  return e;
}

function frame(width: number, height: number): SVGSVGElement {
  const svg = svgEl("svg", {
    width: Math.ceil(width),
    height: Math.ceil(height),
    viewBox: `0 0 ${Math.ceil(width)} ${Math.ceil(height)}`,
  });
  const defs = svgEl("defs");
  const marker = svgEl("marker", {
    id: "pano-arrow",
    viewBox: "0 0 10 10",
    refX: 9,
    refY: 5,
    markerWidth: 7,
    markerHeight: 7,
    orient: "auto-start-reverse",
  });
  marker.appendChild(svgEl("path", { d: "M 0 0 L 10 5 L 0 10 z", "data-arrow": "1" }));
  defs.appendChild(marker);
  svg.appendChild(defs);
  return svg;
}

function edge(
  a: Box,
  b: Box,
  conf: LineConf | "implements" | "composes",
  label: string,
  from: string,
  to: string,
): SVGGElement {
  const g = svgEl("g", { "data-edge": `${from}→${to}`, "data-conf": conf });
  const { x1, y1, x2, y2 } = clipSegment(a, b);
  g.appendChild(svgEl("line", { x1, y1, x2, y2, "marker-end": "url(#pano-arrow)" }));
  if (label) {
    g.appendChild(svgEl("text", { x: (x1 + x2) / 2, y: (y1 + y2) / 2 - 4, "data-edge-label": "1" }, label));
  }
  return g;
}

function nodeGroup(box: Box, lines: string[], onClick: () => void, focus = false): SVGGElement {
  const g = svgEl("g", { "data-node": box.id, tabindex: 0, role: "button" });
  if (focus) g.setAttribute("data-focus", "1");
  g.appendChild(
    svgEl("rect", { x: box.x - box.w / 2, y: box.y - box.h / 2, width: box.w, height: box.h, rx: 6 }),
  );
  lines.forEach((t, i) => {
    g.appendChild(
      svgEl(
        "text",
        {
          x: box.x,
          y: box.y - box.h / 2 + 17 + i * 15,
          "data-line": i === 0 ? "title" : "sub",
        },
        t,
      ),
    );
  });
  g.addEventListener("click", onClick);
  g.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onClick();
    }
  });
  return g;
}

/** 渲染器：团 / 模块（节点 ＋ 带成分的聚合连接）。 */
export function renderClusters(body: ClustersBody, ctx: RenderContext): SVGSVGElement {
  const { boxes, width, height } = layoutClusters(body.nodes);
  const byId = new Map(boxes.map((b) => [b.id, b]));
  const svg = frame(width, height);
  for (const l of body.links) {
    const a = byId.get(l.from);
    const b = byId.get(l.to);
    if (a && b) svg.appendChild(edge(a, b, linkConf(l), linkLabel(l), l.from, l.to));
  }
  body.nodes.forEach((n, i) => {
    const focus = !!ctx.focusFile && n.member_files.includes(ctx.focusFile);
    svg.appendChild(
      nodeGroup(boxes[i], [n.label, `${n.size} 符号 / ${n.files} 文件`], () => ctx.onNode({ shape: "clusters", node: n }), focus),
    );
  });
  return svg;
}

/** 渲染器：符号级调用子图。 */
export function renderCallGraph(body: CallGraphBody, ctx: RenderContext): SVGSVGElement {
  const { boxes, width, height } = layoutCallGraph(body);
  const byId = new Map(boxes.map((b) => [b.id, b]));
  const svg = frame(width, height);
  for (const e of body.edges) {
    const a = byId.get(e.from);
    const b = byId.get(e.to);
    if (a && b) svg.appendChild(edge(a, b, edgeConf(e.confidence), edgeLabel(e), e.from, e.to));
  }
  for (const n of body.nodes) {
    const box = byId.get(n.id);
    if (!box) continue;
    svg.appendChild(
      nodeGroup(box, [n.name, basename(n.file)], () => ctx.onNode({ shape: "call_graph", node: n }), n.id === body.center),
    );
  }
  return svg;
}

/** 渲染器：类型 ＋ 实现/组合关系。 */
export function renderTypeGraph(body: TypeGraphBody, ctx: RenderContext): SVGSVGElement {
  const { boxes, width, height } = layoutTypeGraph(body.types);
  const byId = new Map(boxes.map((b) => [b.id, b]));
  const svg = frame(width, height);
  for (const r of body.relations) {
    const a = byId.get(r.from);
    const b = byId.get(r.to);
    if (a && b) svg.appendChild(edge(a, b, r.kind, r.label ?? (r.kind === "implements" ? "实现" : ""), r.from, r.to));
  }
  body.types.forEach((t, i) => {
    const members = [
      ...t.fields.map((f) => `${f.name}: ${f.ty}`),
      ...t.methods.map((m) => `${m.name}()`),
    ];
    const shown = members.slice(0, TYPE_MEMBER_LINES - 1);
    if (members.length > shown.length) shown.push(`…还有 ${members.length - shown.length} 项`);
    svg.appendChild(nodeGroup(boxes[i], [t.name, ...shown], () => ctx.onNode({ shape: "type_graph", node: t })));
  });
  return svg;
}

function basename(p: string): string {
  const i = p.lastIndexOf("/");
  return i >= 0 ? p.slice(i + 1) : p;
}

/** 图例（常驻，CP3）：每种形状各自的线型说明。不认识的形状 ⇒ 空（那时本来也画不出）。 */
export function legendFor(shape: string): { conf: string; text: string }[] {
  switch (shape) {
    case "clusters":
      return [
        { conf: "exact", text: "粗实线：全部确定" },
        { conf: "dispatch", text: "虚线：动态派发" },
        { conf: "guess", text: "点线加 ?：按名字凑的候选" },
        { conf: "mixed", text: "细实线：混着的一捆，标签写出成分" },
      ];
    case "call_graph":
      return [
        { conf: "exact", text: "粗实线：确定" },
        { conf: "dispatch", text: "虚线：动态派发" },
        { conf: "guess", text: "点线加 ?：按名字凑（N 选 1）" },
      ];
    case "type_graph":
      return [
        { conf: "implements", text: "虚线：实现 / 继承" },
        { conf: "composes", text: "实线：组合（字段类型）" },
      ];
    default:
      return [];
  }
}
