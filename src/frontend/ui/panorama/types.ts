/**
 * Batch15-P2：code-picture 全景前端类型。
 *
 * ⚠ 命名约定（融合手册 §7.3/§7.4）：`Overview`/`NodeView`/`Symbol`/`Edge`… 是 core
 * `code-picture-core` 直出的 Serialize 结构体，**没有** `#[serde(rename_all="camelCase")]`，
 * 所以 wire 上是 **snake_case**（`total_symbols`/`spine_files`/`start_line`/`call_site_line`
 * /`unresolved_calls`…）。这些类型**照抄 core 的 snake_case**，本模块局部不与项目 camelCase
 * 惯例统一（手册推荐做法，省一层 DTO 样板）。
 *
 * 唯一例外：`PanoramaStatus` 是 cc-monitor 侧新建的 DTO（今天住全景小程序 `src/panorama-engine/main.rs`
 * 的 `status` 那一臂，逐字沿用当初 monitor 那份 camelCase；〔RM1f〕monitor 那份 Rust 源随内嵌引擎删了、
 * 生成物 `src/frontend/ui/generated/PanoramaStatus.ts` 跟着删了 ⇒ 这里是它唯一的 TS 住址，
 * 小程序判据 `the_status_shape_matches_the_monitor_dto` 读本文件对拍）→ 这里用 **camelCase**（`indexedAt`）。
 *
 * 〔PANO · `99 §1` V158「线上契约由上游给、本仓不手抄」〕手抄不许悄悄漂：上游镜像逐键两向对 vendored 源码
 * （`tests/frontend/ui/panorama/diagram-guards.vitest.ts` G3 图那一族 · G4 其余结构体与枚举变体）；本仓自己的 DTO
 * （`PanoramaStatus` · `Neighborhood`）由小程序判据读本文件对拍。长期解（上游导出 schema、本仓据它生成）记在 `PANO.md`「上游需求」。
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
  /** 〔PANO 补镜像〕返回类型名（上游 `Symbol.return_type`，没有时缺省）。本仓不读。 */
  return_type?: string | null;
  /** 〔PANO 补镜像〕哪几个形参位流到返回值（上游 `Symbol.return_flow`）。本仓不读。 */
  return_flow?: string | null;
  /** 〔PANO 补镜像〕函数摘要：形参写进哪个形参所指对象的字段（上游 `Symbol.param_flow`）。本仓不读。 */
  param_flow?: string | null;
}

/** 〔P7〕符号 id 的结构化拆分（上游 `SymbolRef`；拆法只住上游 `SymbolRef::of`，前端读字段、不拆 id）。 */
export interface SymbolRef {
  id: string;
  file: string;
  /** `#` 之后、去掉 `@行号` 的那一段；文件级 id ⇒ null。 */
  symbol: string | null;
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
  /** 〔PANO 补镜像〕参数流 `"0>1,2>0"`（上游 `Edge.arg_flow`，没有时缺省）。本仓不读。 */
  arg_flow?: string | null;
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
  // 〔PANO 补镜像〕以下四格上游恒发；标可选只为不逼现有夹具补齐（契约判据比键、不比可选性）。本仓不读。
  /** 成员集合的指纹（「变没变」，不是身份）。 */
  member_hash?: string;
  /** 代表符号（配对锚）。 */
  anchors?: string[];
  /** 团内部调用边数。 */
  internal_edges?: number;
  /** 恰有一端在本团的调用边数。 */
  external_edges?: number;
}

/**
 * 项目全景（文件级）。**无文件间边** → P2 渲聚类气泡地图（非力导边图）；函数级调用子图
 * （有边、力导）留 P4。
 */
export interface Overview {
  spine_files: RankedFile[];
  subsystems: Subsystem[];
  entry_points: SymbolRef[]; // 〔P7〕带 file（上游给），前端不拆 id
  total_symbols: number;
  total_files: number;
  /** ⭐ 覆盖信号：识别为调用但连不上仓内符号（外部/stdlib/漏抓）的调用点数。 */
  unresolved_calls: number;
  /** ⭐ 覆盖信号：解析失败未产出符号的文件数。 */
  parse_errors: number;
  /** 〔PN1b〕看得见但分不清的调用点数（仓内有同名候选、钉不死）。 */
  ambiguous_calls?: number;
  /** 〔PANO 补镜像〕识别为 import 但目标不在仓内的条数（上游恒发；标可选同 `Subsystem` 那四格）。本仓不读。 */
  unresolved_imports?: number;
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

/**
 * 〔PANO · CP1〕以某符号为心的邻域，每个符号带**距根几跳**（`neighborhood` op；小程序 `src/panorama-engine/main.rs`
 * 自己的 DTO，跳数由它按上游 `subgraph` 的 depth 口径给，前端只分组不算）。根不进 `reached`。
 */
export interface Neighborhood {
  root: string;
  reached: { id: string; depth: number }[];
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
  /** 〔PANO 补镜像〕看得见但分不清的调用点数。 */
  ambiguous_calls: number;
  /** 〔PANO 补镜像〕连不上仓内文件的 import 条数。 */
  unresolved_imports: number;
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

// === PN1b：选图（`设计/97 §7`）—— 上游 `diagram::registry` / `diagram::shape` 的手写镜像 ===
//
// ⚠ 与上面那批一样是**手写**（上游类型在 vendored 副本里，不许给它加 `ts_rs` 派生）。
// 🔴 本仓**不写任何图种的名字**：`kind` / `id` 一律是 `string`，选项来自注册表现读。
// 🔴 形状是**开集**：上游将来加一种新形状，线上会出现这里不认识的 `shape` ——
//    类型上它落进 `UnknownDiagramBody`，界面走「这一版还画不出」那条路，不静默。

/** 注册表的一行（`DiagramKindInfo`）。 */
export interface DiagramKindInfo {
  /** 稳定 id —— 画图请求里原样回传。 */
  id: string;
  /** 人读名。 */
  title: string;
  summary: string;
  /** 这张图认哪些输入（`symbol`/`depth`/`max_nodes`/`certain_only`/`exclude_tests` 的子集；开集）。 */
  params: string[];
  /** 画出来的形状（开集）。 */
  shape: string;
}

/** 画图请求（`DiagramRequest`）。上游 `deny_unknown_fields`：拼错字段名会被拒。 */
export interface DiagramRequest {
  symbol?: string | null;
  depth?: number | null;
  max_nodes?: number | null;
  certain_only?: boolean | null;
  exclude_tests?: boolean | null;
}

/** 因节点上限而没画的量。 */
export interface DiagramOmitted {
  nodes: number;
  symbols: number;
  links: number;
}

/**
 * 公共诚实信号（`Honesty`）。🔴 `null` ≠ 0：`null` = 这张图不量这一格（类图不画调用），
 * 0 = 量了、没有。界面把 `null` 写成「不适用」，不写成 0。
 */
export interface DiagramHonesty {
  unresolved_calls: number | null;
  ambiguous_calls: number | null;
  filtered_guess_links: number | null;
  excluded_test_symbols: number | null;
  omitted: DiagramOmitted | null;
  db_errors: string[];
}

/** 团/模块节点（`ArchNode`）。 */
export interface ClusterNode {
  id: string;
  label: string;
  size: number;
  files: number;
  anchors: string[];
  /** 成员文件（下钻用）。 */
  member_files: string[];
}

/** 一捆聚合连接（`ArchLink`）：成分分开记，不许合成一个「最可信的档」。 */
export interface ClusterLink {
  from: string;
  to: string;
  exact: number;
  dispatch: number;
  guess: number;
}

export interface ClustersBody {
  shape: "clusters";
  nodes: ClusterNode[];
  links: ClusterLink[];
}

export interface CallNode {
  id: string;
  name: string;
  file: string;
  kind: SymKind;
  start_line: number;
}

export interface CallEdge {
  from: string;
  to: string;
  confidence: Confidence;
  candidates: number | null;
  call_site_line: number | null;
}

export interface CallGraphBody {
  shape: "call_graph";
  center: string;
  depth: number;
  nodes: CallNode[];
  edges: CallEdge[];
}

export interface TypeNode {
  id: string;
  name: string;
  symbol: string | null;
  fields: { name: string; ty: string }[];
  methods: { name: string; symbol: string }[];
}

export interface TypeRelation {
  from: string;
  to: string;
  kind: "implements" | "composes";
  label: string | null;
}

export interface TypeGraphBody {
  shape: "type_graph";
  types: TypeNode[];
  relations: TypeRelation[];
}

/** 这一版不认识的形状 —— 只保证有 `shape` 这个标签。 */
export interface UnknownDiagramBody {
  shape: string;
}

export type DiagramBody = ClustersBody | CallGraphBody | TypeGraphBody | UnknownDiagramBody;

export interface Diagram {
  kind: string;
  honesty: DiagramHonesty;
  body: DiagramBody;
}

/** `panorama_diagram` 的返回：图 ＋ 上游 Mermaid 渲染（复制与「画不出」兜底用，不许解析它）。 */
export interface PanoramaDiagram {
  diagram: Diagram;
  mermaid: string;
}
