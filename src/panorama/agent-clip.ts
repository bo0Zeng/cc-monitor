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

export const CLIP_HEAD = "【代码全景 → agent】下面是某一次索引的快照，不是当前事实；用之前以代码为准。";

/** 索引读数那一行。 */
export function stampLine(stamp: IndexStamp | null): string {
  if (!stamp) return "索引读数：未取到（查索引状态失败）—— 这段内容的时效未知";
  if (stamp.indexedAt === null) return "索引读数：本仓没有索引记录 —— 这段内容的时效未知";
  const iso = new Date(stamp.indexedAt * 1000).toISOString();
  return (
    `索引读数：${iso}（unix ${stamp.indexedAt}）` +
    (stamp.stale ? " · ⚠ 索引已陈旧：源文件在这次索引之后改过" : "")
  );
}

/** CP4 前半：看不见多少。 */
export function unseenLine(cov: CoverageReading | null): string {
  if (!cov) return "看不见：未取到全仓覆盖读数（全景没加载完）—— 漏了多少未知";
  return (
    `看不见：全仓 ${cov.unresolved_calls} 处调用未解析 · ${cov.parse_errors} 个文件解析失败` +
    ` · 全景图只画了 ${cov.drawn_files}/${cov.total_files} 个文件`
  );
}

/** 按后端标的 confidence 计数（只数，不判）。 */
export function countConfidence(edges: Edge[]): Record<Confidence, number> {
  const c: Record<Confidence, number> = { Exact: 0, Heuristic: 0, DynamicGuess: 0 };
  for (const e of edges) c[e.confidence] = (c[e.confidence] ?? 0) + 1;
  return c;
}

/** CP4 后半：分不清多少（符号级）。 */
export function unsureLine(callers: Edge[], callees: Edge[]): string {
  const all = [...callers, ...callees];
  const c = countConfidence(all);
  const unsure = c.Heuristic + c.DynamicGuess;
  return (
    `分不清：本符号 ${all.length} 条直接边里 ${unsure} 条不是确定的` +
    `（启发 ${c.Heuristic} · 动态猜测 ${c.DynamicGuess}），确定 ${c.Exact} 条`
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
    `仓：${ctx.repo}`,
    `对象：符号 ${s.id}（${s.kind} · ${s.lang} · ${s.file}:${s.start_line}-${s.end_line}）`,
  ];
  if (s.signature) lines.push(`签名：${s.signature}`);
  lines.push(stampLine(ctx.stamp), unseenLine(ctx.coverage), unsureLine(callers, callees));
  lines.push(`调用了（callees，${callees.length}）：`);
  for (const e of callees) lines.push(edgeLine(e.to, e));
  lines.push(`被调用（callers，${callers.length}）：`);
  for (const e of callers) lines.push(edgeLine(e.from, e));
  if (annotations.length > 0) {
    lines.push(`生效批注（${annotations.length}）：`);
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
    `仓：${ctx.repo}`,
    `对象：文件 ${f.file}（子系统「${f.subsystem}」· ${f.symbols} 个符号${f.isEntry ? " · 含入口点" : ""}）`,
    stampLine(ctx.stamp),
    unseenLine(ctx.coverage),
    "分不清：文件级没有边 —— 边的确定度要点进符号那一级看",
    `符号（${symbols.length}）：`,
  ];
  for (const s of symbols) lines.push(`  - ${s.id}  ${s.kind}  L${s.start_line}`);
  return lines.join("\n");
}
