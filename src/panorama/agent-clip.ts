/**
 * 「复制给 agent」的文本（`设计/97 §2.4` 判据 CP7）。纯函数，界面只负责把它塞进剪贴板。
 *
 * ## 这段文本必须带的三样（CP7）
 * 1. **住址**：哪个仓 · 哪个符号／文件（连子系统）。
 * 2. **哪一次索引的读数**：`indexedAt`（UTC ISO ＋ unix 秒），陈旧就明说。
 *    没有这一行，agent 会把十分钟前那次索引当成当前事实用 —— 那一族错误静默且很贵。
 * 3. **CP4 那一行**：「看不见多少 / 分不清多少」。一段滤过的图描述不带它，就会被读成「这就是全部」。
 *
 * ## CP1：这里不算任何图分析
 * 只**搬运**后端给的数（未解析调用数、解析失败数、每条边的 confidence）并**计数**；
 * 不判边、不定置信度、不聚类。「分不清」= 后端标成 `Heuristic` / `DynamicGuess` 的边的条数。
 *
 * 买到：贴进对话的每一段都能被追溯到「哪个仓、哪次索引、漏了多少」。
 * **买不到**：它不保证那次索引**现在**还对 —— 只保证把「是哪一次」说出来；
 * 也不含图上的视觉选区（今天的图是文件气泡图，没有框选；选中单位就是一个文件或一个符号）。
 */
import type { Annotation, Confidence, Edge, Symbol } from "./types";
import { copyText } from "../copy-table";

/** 哪一次索引（`panorama_status` 的读数）。`null` = 没取到，文本里会如实说「未取到」。 */
export interface IndexStamp {
  indexedAt: number | null;
  stale: boolean;
}

/** CP4「看不见多少」的原料 —— 全部来自后端 overview 与本页画了几个气泡。 */
export interface CoverageReading {
  unresolved_calls: number;
  parse_errors: number;
  total_files: number;
  /** 全景图上实际画出的文件气泡数（只画脊柱文件）。 */
  drawn_files: number;
}

export interface ClipContext {
  repo: string;
  stamp: IndexStamp | null;
  /** `null` = 全景没加载完（没有 overview）—— 文本里如实说，而不是省掉那一行。 */
  coverage: CoverageReading | null;
}

export const CLIP_HEAD = copyText("agentClip.head.banner");

/** 索引读数那一行。 */
export function stampLine(stamp: IndexStamp | null): string {
  if (!stamp) return copyText("agentClip.stamp.failed");
  if (stamp.indexedAt === null) return copyText("agentClip.stamp.none");
  const iso = new Date(stamp.indexedAt * 1000).toISOString();
  return (
    copyText("agentClip.stamp.line", { iso, indexedAt: stamp.indexedAt, stale: (stamp.stale ? copyText("agentClip.stamp.stale") : "") })
  );
}

/** CP4 前半：看不见多少。 */
export function unseenLine(cov: CoverageReading | null): string {
  if (!cov) return copyText("agentClip.unseen.unknown");
  return (
    copyText("agentClip.unseen.counts", { unresolvedCalls: cov.unresolved_calls, parseErrors: cov.parse_errors, drawnFiles: cov.drawn_files, totalFiles: cov.total_files })
  );
}

/** 按后端标的 confidence 计数（只数，不判）。 */
export function countConfidence(edges: Edge[]): Record<Confidence, number> {
  const c: Record<Confidence, number> = { Exact: 0, Dispatch: 0, Heuristic: 0, DynamicGuess: 0 };
  for (const e of edges) c[e.confidence] = (c[e.confidence] ?? 0) + 1;
  return c;
}

/**
 * CP4 后半：分不清多少（符号级）。「分不清」= 按名字凑的（`Heuristic` + `DynamicGuess`）；
 * 动态派发（`Dispatch`）候选集完整、只是运行时才定 —— 上游说它与按名字凑是两种认知状态，
 * 这里单列，不并进「分不清」。
 */
export function unsureLine(callers: Edge[], callees: Edge[]): string {
  const all = [...callers, ...callees];
  const c = countConfidence(all);
  const unsure = c.Heuristic + c.DynamicGuess;
  return (
    copyText("agentClip.unsure.symbol", { allCount: all.length, unsure, heuristic: c.Heuristic, dynamicGuess: c.DynamicGuess, dispatch: c.Dispatch, exact: c.Exact })
  );
}

const edgeLine = (otherId: string, e: Edge): string =>
  `  - ${otherId}  [${e.confidence}]` + (e.call_site_line != null ? `  L${e.call_site_line}` : "");

/** 符号那一级的「复制给 agent」。 */
export function clipForSymbol(
  ctx: ClipContext,
  s: Symbol,
  callers: Edge[],
  callees: Edge[],
  annotations: Annotation[],
): string {
  const lines = [
    CLIP_HEAD,
    copyText("agentClip.clip.repo", { repo: ctx.repo }),
    copyText("agentClip.clip.symbolObject", { id: s.id, kind: s.kind, lang: s.lang, file: s.file, startLine: s.start_line, endLine: s.end_line }),
  ];
  if (s.signature) lines.push(copyText("agentClip.clip.signature", { signature: s.signature }));
  lines.push(stampLine(ctx.stamp), unseenLine(ctx.coverage), unsureLine(callers, callees));
  lines.push(copyText("agentClip.clip.callees", { n: callees.length }));
  for (const e of callees) lines.push(edgeLine(e.to, e));
  lines.push(copyText("agentClip.clip.callers", { n: callers.length }));
  for (const e of callers) lines.push(edgeLine(e.from, e));
  if (annotations.length > 0) {
    lines.push(copyText("agentClip.clip.annotations", { n: annotations.length }));
    for (const a of annotations) lines.push(`  - ${a.author}：${a.body}`);
  }
  return lines.join("\n");
}

/** 文件那一级（点气泡）的「复制给 agent」。 */
export function clipForFile(
  ctx: ClipContext,
  f: { file: string; subsystem: string; symbols: number; isEntry: boolean },
  symbols: Symbol[],
): string {
  const lines = [
    CLIP_HEAD,
    copyText("agentClip.clip.repo", { repo: ctx.repo }),
    copyText("agentClip.clip.fileObject", { file: f.file, subsystem: f.subsystem, symbols: f.symbols, entry: f.isEntry ? copyText("agentClip.clip.fileEntry") : "" }),
    stampLine(ctx.stamp),
    unseenLine(ctx.coverage),
    copyText("agentClip.unsure.file"),
    copyText("agentClip.clip.symbols", { n: symbols.length }),
  ];
  for (const s of symbols) lines.push(`  - ${s.id}  ${s.kind}  L${s.start_line}`);
  return lines.join("\n");
}

/**
 * PN1b：一张图的「复制给 agent」—— Mermaid ＋ 诚实信号那一行 ＋ 索引读数（CP7 三样都在）。
 * `honesty` 是调用方用 `diagram-honesty.ts` 拼好的那一行（数据只来自上游 `Diagram.honesty`）；
 * `mermaid` 是上游 `to_mermaid` 的原文（它自己的图注里也印着诚实读数）。
 */
export function clipForDiagram(
  ctx: Pick<ClipContext, "repo" | "stamp">,
  kind: { id: string; title: string },
  honesty: string,
  mermaid: string,
  center: string | null,
): string {
  return [
    CLIP_HEAD,
    copyText("agentClip.clip.repo", { repo: ctx.repo }),
    copyText("agentClip.clip.diagramObject", { title: kind.title, id: kind.id, center: (center ? copyText("agentClip.clip.diagramCenter", { symbol: center }) : "") }),
    stampLine(ctx.stamp),
    copyText("agentClip.clip.honesty", { honesty }),
    "Mermaid：",
    mermaid.trimEnd(),
  ].join("\n");
}
