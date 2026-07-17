/**
 * Batch15-P2：code-picture 全景后端命令的 invoke 封装（融合手册 §7.1）。
 *
 * 每个命令一个 fn，参数名 **camelCase**（Tauri 自动映射到 Rust snake_case 参数，
 * 与项目惯例一致）。所有命令都带 `repo`（活跃 tab 的 cwd）——后端 `panorama.rs` 按仓
 * 惰性建/缓存 Engine（per-repo 池）。返回类型是 core 直出的 snake_case 结构体（见 types.ts）。
 *
 * 后端命令族（`src-tauri/src/panorama.rs`）：index/reindex/status/overview/node/subgraph/
 * callers/callees/impact/search/docs_for/touching。
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  Overview,
  NodeView,
  SubGraph,
  Symbol,
  Edge,
  ImpactSet,
  DocLink,
  IndexStats,
  PanoramaStatus,
  DriftItem,
  Annotation,
} from "./types";

/** 建索引（重活：tree-sitter 解析全仓 → SQLite）。开面板首次调 + loading。 */
export const index = (repo: string) =>
  invoke<IndexStats>("panorama_index", { repo });

/** 重建索引（改代码后刷新；只写 `.codepicture/index.db`，非侵入）。刷新按钮调。 */
export const reindex = (repo: string) =>
  invoke<IndexStats>("panorama_reindex", { repo });

/** 索引新鲜度 + 上次索引时间 + 符号总数。开面板时查一次决定是否需索引。 */
export const status = (repo: string) =>
  invoke<PanoramaStatus>("panorama_status", { repo });

/** 项目全景（脊柱文件 + 子系统聚类 + 入口点 + 覆盖信号）。budget 控 token 预算裁剪。 */
export const overview = (repo: string, budget?: number) =>
  invoke<Overview>("panorama_overview", { repo, budget });

/**
 * F69（补 D20：代码分析默认关、每仓手动开启）：据 status 决定开面板时的动作。
 * `symbols===0` = 从未索引 = 未启用本仓分析 → `"enable-gate"`（显式手势才扫描，绝不自动扫）；
 * 否则（`symbols>0`，用户此前已启用）→ `"load"`（直接加载现有 overview，stale 靠手动「刷新」）。
 * 抽成纯函数是为了单测钉死「默认关」不被回归成开面板即自动扫描（D20 违规）。
 */
export function panoramaLoadDecision(
  st: Pick<PanoramaStatus, "symbols">,
): "enable-gate" | "load" {
  return st.symbols === 0 ? "enable-gate" : "load";
}

/** 单符号详情（符号 + 直接 callers/callees + 关联文档）。symbol 用全限定 id。 */
export const node = (repo: string, symbol: string) =>
  invoke<NodeView | null>("panorama_node", { repo, symbol });

/** 以某符号为心的双向邻域子图（P2 未用；留 P4 力导图）。 */
export const subgraph = (repo: string, symbol: string, depth: number) =>
  invoke<SubGraph>("panorama_subgraph", { repo, symbol, depth });

/** 反向调用边（谁调用了它，BFS 到 depth）。 */
export const callers = (repo: string, symbol: string, depth: number) =>
  invoke<Edge[]>("panorama_callers", { repo, symbol, depth });

/** 正向调用边（它调用了谁，BFS 到 depth）。 */
export const callees = (repo: string, symbol: string, depth: number) =>
  invoke<Edge[]>("panorama_callees", { repo, symbol, depth });

/** 改动某符号的 blast-radius（反向可达的全部传递调用者）。P2 未用（留 P4）。 */
export const impact = (repo: string, symbol: string) =>
  invoke<ImpactSet>("panorama_impact", { repo, symbol });

/** 按名子串搜符号 → 拿全限定 id（裸名不解析，先搜再查 node）。 */
export const search = (repo: string, query: string, limit?: number) =>
  invoke<Symbol[]>("panorama_search", { repo, query, limit });

/** 覆盖某符号的 `.md` 文档链接。 */
export const docsFor = (repo: string, symbol: string) =>
  invoke<DocLink[]>("panorama_docs_for", { repo, symbol });

/** F71：列某文件的所有符号（点文件气泡 → 展开符号列表 → 点符号进详情）。 */
export const symbolsInFile = (repo: string, file: string) =>
  invoke<Symbol[]>("panorama_symbols_in_file", { repo, file });

/** F71：文档漂移（仓里 `.md` 指向的目标文件/符号已失效）。反映上次索引快照，刷新后新鲜。 */
export const drift = (repo: string) =>
  invoke<DriftItem[]>("panorama_drift", { repo });

// === F72：批注 + 文档关联写（落被分析仓、人手势触发）。core 现成接口，不自造存储（SS-15）。 ===

/** F72：人写批注（直接 Active）。`symbol` = 符号段（如 `f`），null = 文件级。 */
export const addAnnotation = (
  repo: string,
  file: string,
  symbol: string | null,
  body: string,
  author: string,
) => invoke<string>("panorama_add_annotation", { repo, file, symbol, body, author });

/** F72：agent 提议批注（Proposed，需人 approve 才 Active）。 */
export const proposeAnnotation = (
  repo: string,
  file: string,
  symbol: string | null,
  body: string,
  author: string,
) => invoke<string>("panorama_propose_annotation", { repo, file, symbol, body, author });

/** F72：批准一条 Proposed 批注 → Active。 */
export const approveAnnotation = (repo: string, id: string) =>
  invoke<boolean>("panorama_approve_annotation", { repo, id });

/** F72：删批注。 */
export const removeAnnotation = (repo: string, id: string) =>
  invoke<boolean>("panorama_remove_annotation", { repo, id });

/** F72：列全部批注（含 Proposed，审批队列用）。 */
export const listAnnotations = (repo: string) =>
  invoke<Annotation[]>("panorama_list_annotations", { repo });

/** F72：把某 `.md` 关联到某符号（写 doc 的 frontmatter covers:，进仓可提交）。 */
export const writeDocLink = (repo: string, doc: string, target: string) =>
  invoke<void>("panorama_write_doc_link", { repo, doc, target });

/** F72：删除某 `.md` 对某符号的关联。 */
export const removeDocLink = (repo: string, doc: string, target: string) =>
  invoke<boolean>("panorama_remove_doc_link", { repo, doc, target });

/**
 * ⭐ P3 护城河缝（P2 未接）：一组文件/行 → 命中的符号 id。`ranges` 空 → 整文件所有符号。
 * cc-monitor 从 jsonl 的 Edit/Write 拿「agent 刚改了哪些文件行」→ 高亮 = 「agent 正在改这几个节点」。
 */
export const touching = (
  repo: string,
  files: string[],
  ranges: [number, number][],
) => invoke<string[]>("panorama_touching", { repo, files, ranges });
