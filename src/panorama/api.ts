/**
 * Batch15-P2：code-picture 全景后端命令的 invoke 封装（融合手册 §7.1）。
 *
 * 每个入口吃一个 [`RepoAt`]（**哪台机器上的哪个仓**，`设计/97 §6` ④）。返回类型是 core 直出的
 * snake_case 结构体（见 types.ts）—— 本机与远端**逐字同形**，渲染那一层只有一份。
 *
 * ## 〔RM1c · 第四波〕两条路（用户 09-24 V108 选 B）
 *
 * - **本机**：仍是 monitor 进程内那几条命令（`src/bridge/src/panorama.rs`，per-repo Engine 池）。
 *   第二拍（本机也改走本机后端、monitor 摘内嵌引擎）才换 —— 那一拍先要答「本机的批注写走哪」。
 * - **远端**：`panorama_call(origin, op, …)` ⇒ 那台机器的后端 `panorama` 帧命令 ⇒ 后端经插件口起
 *   只装引擎的独立小程序，索引与图都在那台机器上算，线上只回结构化结果（不传源码、不传索引）。
 * - **远端第一拍只读**：批注 / 文档关联的写入口在远端**当场拒**并说清为什么
 *   （远端仓里的那些文件是那台机器上的用户文件，要等后端文件管理那一面接上才经它写）。
 */
import { commands } from "../ipc/commands";
import { isLocalOrigin, type Origin } from "../ipc/origin";
import type {
  Annotation,
  DiagramKindInfo,
  DiagramRequest,
  DocLink,
  DriftItem,
  Edge,
  ImpactSet,
  IndexStats,
  NodeView,
  Overview,
  PanoramaDiagram,
  PanoramaStatus,
  SubGraph,
  Symbol as PanoramaSymbol,
} from "./types";

/** 哪台机器上的哪个仓。`path` 是**那台机器上**的绝对路径。 */
export type RepoAt = { origin: Origin; path: string };

/** 远端仓上写入口的那句拒绝（`设计/97 §6` ③）。**只有这一处**说这句话。 */
export const REMOTE_WRITE_REFUSED =
  "远端仓的批注和文档关联暂时只能看、不能改：这一版还不支持改远端机器上的这些文件。";

/** 远端：问那台机器的后端（`result` 的形状由 op 定，与本机那条同形）。 */
function remote<T>(at: { origin: Origin; path: string | null }, op: string, args?: object): Promise<T> {
  return commands.panorama_call({ origin: at.origin, op, repo: at.path, args: args ?? null }) as Promise<T>;
}

/** 远端的写入口：当场拒（不发任何请求）。 */
function refuseRemoteWrite<T>(): Promise<T> {
  return Promise.reject(new Error(REMOTE_WRITE_REFUSED));
}

/** 这个仓能不能写批注 / 文档关联（界面据此决定要不要给写入口）。 */
export const canWriteAnnotations = (at: RepoAt): boolean => isLocalOrigin(at.origin);

/** 两个 `RepoAt` 说的是不是同一个仓（同一台机器、同一个路径；两个都没有也算同一个）。 */
export const sameRepo = (a: RepoAt | null, b: RepoAt | null): boolean =>
  a === null || b === null ? a === b : a.origin === b.origin && a.path === b.path;

/** 界面上与「复制给 agent」里说这个仓的那一串：本机就是路径，远端带上机器名。 */
export const repoLabel = (at: RepoAt): string =>
  isLocalOrigin(at.origin) ? at.path : `${at.path}（远端 ${at.origin}）`;

/** 建索引（重活：tree-sitter 解析全仓 → SQLite）。开面板首次调 + loading。 */
export const index = (at: RepoAt): Promise<IndexStats> =>
  isLocalOrigin(at.origin) ? commands.panorama_index({ repo: at.path }) : remote(at, "index");

/** 重建索引（改代码后刷新；只写索引，非侵入）。刷新按钮调。 */
export const reindex = (at: RepoAt): Promise<IndexStats> =>
  isLocalOrigin(at.origin) ? commands.panorama_reindex({ repo: at.path }) : remote(at, "reindex");

/** 索引新鲜度 + 上次索引时间 + 符号总数。开面板时查一次决定是否需索引。 */
export const status = (at: RepoAt): Promise<PanoramaStatus> =>
  isLocalOrigin(at.origin) ? commands.panorama_status({ repo: at.path }) : remote(at, "status");

/** 项目全景（脊柱文件 + 子系统聚类 + 入口点 + 覆盖信号）。budget 控 token 预算裁剪。 */
export const overview = (at: RepoAt, budget?: number): Promise<Overview> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_overview({ repo: at.path, budget })
    : remote(at, "overview", budget === undefined ? {} : { budget });

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
export const node = (at: RepoAt, symbol: string): Promise<NodeView | null> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_node({ repo: at.path, symbol })
    : remote(at, "node", { symbol });

/** 以某符号为心的双向邻域子图。 */
export const subgraph = (at: RepoAt, symbol: string, depth: number): Promise<SubGraph> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_subgraph({ repo: at.path, symbol, depth })
    : remote(at, "subgraph", { symbol, depth });

/** 反向调用边（谁调用了它，BFS 到 depth）。 */
export const callers = (at: RepoAt, symbol: string, depth: number): Promise<Edge[]> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_callers({ repo: at.path, symbol, depth })
    : remote(at, "callers", { symbol, depth });

/** 正向调用边（它调用了谁，BFS 到 depth）。 */
export const callees = (at: RepoAt, symbol: string, depth: number): Promise<Edge[]> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_callees({ repo: at.path, symbol, depth })
    : remote(at, "callees", { symbol, depth });

/** 改动某符号的 blast-radius（反向可达的全部传递调用者）。 */
export const impact = (at: RepoAt, symbol: string): Promise<ImpactSet> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_impact({ repo: at.path, symbol })
    : remote(at, "impact", { symbol });

/** 按名子串搜符号 → 拿全限定 id（裸名不解析，先搜再查 node）。 */
export const search = (at: RepoAt, query: string, limit?: number): Promise<PanoramaSymbol[]> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_search({ repo: at.path, query, limit })
    : remote(at, "search", limit === undefined ? { query } : { query, limit });

/** 覆盖某符号的 `.md` 文档链接。 */
export const docsFor = (at: RepoAt, symbol: string): Promise<DocLink[]> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_docs_for({ repo: at.path, symbol })
    : remote(at, "docs_for", { symbol });

/**
 * PN1b：图种注册表（原样）。本机那份编在 monitor 里；远端那份问那台机器上的小程序
 * （两台的版本可以不同 ⇒ 按机器取，不共用一份）。
 */
export const diagramKinds = (origin: Origin): Promise<DiagramKindInfo[]> =>
  isLocalOrigin(origin)
    ? commands.panorama_diagram_kinds()
    : remote({ origin, path: null }, "diagram_kinds");

/** PN1b：画一张图。`kind` 是注册表里的 id，本仓不写死。 */
export const diagram = (at: RepoAt, kind: string, request: DiagramRequest): Promise<PanoramaDiagram> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_diagram({ repo: at.path, kind, request })
    : remote(at, "diagram", { kind, request });

/** F71：列某文件的所有符号（点文件气泡 → 展开符号列表 → 点符号进详情）。 */
export const symbolsInFile = (at: RepoAt, file: string): Promise<PanoramaSymbol[]> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_symbols_in_file({ repo: at.path, file })
    : remote(at, "symbols_in_file", { file });

/** F71：文档漂移（仓里 `.md` 指向的目标文件/符号已失效）。反映上次索引快照，刷新后新鲜。 */
export const drift = (at: RepoAt): Promise<DriftItem[]> =>
  isLocalOrigin(at.origin) ? commands.panorama_drift({ repo: at.path }) : remote(at, "drift");

// === F72：批注 + 文档关联写（落被分析仓、人手势触发）。core 现成接口，不自造存储（SS-15）。 ===
// 〔RM1c〕远端仓上这几样一律当场拒（`REMOTE_WRITE_REFUSED`）。

/** F72：人写批注（直接 Active）。`symbol` = 符号段（如 `f`），null = 文件级。 */
export const addAnnotation = (
  at: RepoAt,
  file: string,
  symbol: string | null,
  body: string,
  author: string,
): Promise<string> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_add_annotation({ repo: at.path, file, symbol, body, author })
    : refuseRemoteWrite();

/** F72：agent 提议批注（Proposed，需人 approve 才 Active）。 */
export const proposeAnnotation = (
  at: RepoAt,
  file: string,
  symbol: string | null,
  body: string,
  author: string,
): Promise<string> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_propose_annotation({ repo: at.path, file, symbol, body, author })
    : refuseRemoteWrite();

/** F72：批准一条 Proposed 批注 → Active。 */
export const approveAnnotation = (at: RepoAt, id: string): Promise<boolean> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_approve_annotation({ repo: at.path, id })
    : refuseRemoteWrite();

/** F72：删批注。 */
export const removeAnnotation = (at: RepoAt, id: string): Promise<boolean> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_remove_annotation({ repo: at.path, id })
    : refuseRemoteWrite();

/** F72：列全部批注（含 Proposed，审批队列用）。**读**，远端也能看。 */
export const listAnnotations = (at: RepoAt): Promise<Annotation[]> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_list_annotations({ repo: at.path })
    : remote(at, "list_annotations");

/** F72：把某 `.md` 关联到某符号（写 doc 的 frontmatter covers:，进仓可提交）。 */
export const writeDocLink = (at: RepoAt, doc: string, target: string): Promise<void> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_write_doc_link({ repo: at.path, doc, target })
    : refuseRemoteWrite();

/** F72：删除某 `.md` 对某符号的关联。 */
export const removeDocLink = (at: RepoAt, doc: string, target: string): Promise<boolean> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_remove_doc_link({ repo: at.path, doc, target })
    : refuseRemoteWrite();

/**
 * ⭐ P3 护城河缝：一组文件/行 → 命中的符号 id。`ranges` 空 → 整文件所有符号。
 * cc-monitor 从 jsonl 的 Edit/Write 拿「agent 刚改了哪些文件行」→ 高亮 = 「agent 正在改这几个节点」。
 * 远端会话的文件路径本来就是那台机器上的路径 ⇒ 远端仓照样问那台。
 */
export const touching = (
  at: RepoAt,
  files: string[],
  ranges: [number, number][],
): Promise<string[]> =>
  isLocalOrigin(at.origin)
    ? commands.panorama_touching({ repo: at.path, files, ranges })
    : remote(at, "touching", { files, ranges });
