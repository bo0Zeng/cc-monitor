/**
 * Batch15-P2：code-picture 全景前端类型。
 *
 * ⚠ 命名约定（融合手册 §7.3/§7.4）：`Overview`/`NodeView`/`Symbol`/`Edge`… 是 core
 * `code-picture-core` 直出的 Serialize 结构体，**没有** `#[serde(rename_all="camelCase")]`，
 * 所以 wire 上是 **snake_case**（`total_symbols`/`spine_files`/`start_line`/`call_site_line`
 * /`unresolved_calls`…）。这些类型**照抄 core 的 snake_case**，本模块局部不与项目 camelCase
 * 惯例统一（手册推荐做法，省一层 DTO 样板）。
 *
 * 唯一例外：`PanoramaStatus` 是 cc-monitor 侧新建的 DTO（`src/bridge/src/panorama.rs`），
 * 带 `#[serde(rename_all="camelCase")]` → 这里用 **camelCase**（`indexedAt`）。
 */

// 枚举都序列化为字符串（externally-tagged 单元变体）
export type Lang =
  | "Rust"
  | "Python"
  | "JavaScript"
  | "TypeScript"
  | "Java"
  | "Kotlin"
  | "C"
  | "Cpp"
  | "CSharp";
export type SymKind = "Function" | "Method" | "Class" | "Module";
/**
 * 〔PN1b re-vendor〕上游加了 `Dispatch`（动态派发：接收者是仓内 trait/接口，候选集**完整**，
 * 只是运行时才知道是哪个）—— 与 `Heuristic`/`DynamicGuess`（按名字凑）是两种认知状态，别混。
 */
export type Confidence = "Exact" | "Dispatch" | "Heuristic" | "DynamicGuess";
/** 〔PN1b〕`AmbiguousCall` = 「N 选 1 里的一个」，`to` 是妥协不是真相；`candidates` 记 N。 */
export type EdgeKind = "Calls" | "Imports" | "AmbiguousCall";

/**
 * 一个符号。`id` 是全限定：`"src/a.rs#foo"`（自由函数）或 `"src/a.rs#Type::method"`（方法）。
 * 注：core 的 `body_hash` 是内部指纹，已 `#[serde(skip)]`，前端拿不到（无需要）。
 */
export interface Symbol {
  id: string;
  name: string; // 裸名
  file: string; // 仓库相对，正斜杠
  kind: SymKind;
  lang: Lang;
  start_line: number; // 1-based
  end_line: number;
  /** F68：签名文本（如 `fn foo(a:u32)->String`）。后端 `Symbol.signature`，拿不到时缺省。 */
  signature?: string | null;
}

/** 一条调用/导入边。`confidence` 是**尽力**标注（非 sound），如实呈现别当完整真相。 */
export interface Edge {
  from: string; // 符号 id
  to: string; // 符号 id
  kind: EdgeKind;
  call_site_line: number | null;
  confidence: Confidence;
  /** 〔PN1b〕`AmbiguousCall` 专用：仓内同名候选数（缺省 = 不适用）。 */
  candidates?: number;
}

/** 脊柱文件（按重要性排名）。`score` 越大越重要，`symbols` = 文件内符号数。 */
export interface RankedFile {
  file: string;
  score: number;
  symbols: number;
}

/** 一个子系统（文件聚类）。`label` = 聚类名，`files` = 成员文件（仓库相对），`size` = 规模。 */
export interface Subsystem {
  label: string;
  files: string[];
  size: number;
}

/**
 * 项目全景（文件级）。**无文件间边** → P2 渲聚类气泡地图（非力导边图）；函数级调用子图
 * （有边、力导）留 P4。
 */
export interface Overview {
  spine_files: RankedFile[];
  subsystems: Subsystem[];
  entry_points: string[]; // 符号 id
  total_symbols: number;
  total_files: number;
  /** ⭐ 覆盖信号：识别为调用但连不上仓内符号（外部/stdlib/漏抓）的调用点数。 */
  unresolved_calls: number;
  /** ⭐ 覆盖信号：解析失败未产出符号的文件数。 */
  parse_errors: number;
  /** 〔PN1b〕看得见但分不清的调用点数（仓内有同名候选、钉不死）。 */
  ambiguous_calls?: number;
  /** 〔PN1b〕读库出错记录；非空 = 这份全景不完整（空时上游省略这个字段）。 */
  db_errors?: string[];
}

/** 覆盖某符号的 `.md` 文档链接。 */
export interface DocLink {
  doc_path: string;
  target_file: string;
  target_symbol: string | null;
  source: "Colocation" | "Frontmatter" | "Inline";
}

/**
 * 〔PN1b · CP6〕批注来源：人写 / agent 提议（批准后仍是 agent）/ 旧文件没记（不猜）。
 * 与 `status`（审没审）是两件事。
 */
export type AnnotationOrigin = "Human" | "Agent" | "Unrecorded";

/** 人写/agent 提议的批注。 */
export interface Annotation {
  id: string;
  file: string;
  symbol: string | null;
  body: string;
  author: string;
  status: "Active" | "Proposed";
  origin: AnnotationOrigin;
}

/** 单符号详情（符号 + 直接 callers/callees + 关联文档 + 批注）。 */
export interface NodeView {
  symbol: Symbol;
  callers: Edge[];
  callees: Edge[];
  docs: DocLink[];
  annotations: Annotation[];
}

/** 以某符号为心的双向邻域子图（节点集 + 边集）。P2 未用（留 P4）。 */
export interface SubGraph {
  symbols: Symbol[];
  edges: Edge[];
}

/** 受影响的符号 + 反向距离（1 = 直接调用者）。 */
export interface AffectedSymbol {
  id: string;
  depth: number;
}

/** 改动某符号的 blast-radius（反向可达的全部传递调用者）。P2 未用（留 P4）。 */
export interface ImpactSet {
  root: string;
  affected: AffectedSymbol[];
}

/** 索引统计（index/reindex 返回）。 */
export interface IndexStats {
  files: number;
  symbols: number;
  unresolved_calls: number;
  parse_errors: number;
}

/**
 * 索引状态。**cc-monitor 侧新建 DTO** → camelCase（`indexedAt`）。
 * `stale` = 源文件有改动、索引已陈旧；`indexedAt` = 上次索引 unix 秒（null=从未索引）；
 * `symbols` = 已索引符号总数（0 通常意味着尚未索引）。
 */
export interface PanoramaStatus {
  stale: boolean;
  indexedAt: number | null;
  symbols: number;
}

/**
 * F71：文档漂移项——仓里 `.md` 指向的目标文件/符号已失效（悬空链接）。core 直出 snake_case
 * （见 §7.3 passthrough）。`reason` ∈ 文件不存在 / 目录不存在 / 符号已不存在 / 符号有多个同名候选。
 */
export interface DriftItem {
  doc_path: string;
  target_file: string;
  target_symbol: string | null;
  reason: string;
}
