/**
 * Batch15-P2：code-picture 全景后端命令的 invoke 封装（融合手册 §7.1）。
 *
 * 每个入口吃一个 [`RepoAt`]（**哪台机器上的哪个仓**，`设计/97 §6` ④）。返回类型是 core 直出的
 * snake_case 结构体（见 types.ts）—— 本机与远端**逐字同形**，渲染那一层只有一份。
 *
 * ## 〔RM1f · 本机对称〕一条路（用户 09-24 V108：选 B，「之后本机也走这条路、monitor 摘内嵌引擎」）
 *
 * 本机与远端同一条：`panorama_call(origin, op, …)` ⇒ 那台机器的后端 `panorama` 帧命令 ⇒ 后端经插件口起
 * 只装引擎的独立小程序，索引与图都在那台机器上算，线上只回结构化结果（不传源码、不传索引）。本机 = `<local>`。
 * 〔墓碑 —— RM1c 那一版这里是两条路：本机仍走 monitor 进程内那几条命令（per-repo Engine 池），远端才走 `panorama_call`。〕
 * - 〔RM1d · V110「引擎只算、文件管理来写」〕批注 / 文档关联的**写**本机远端同一条：
 *   `panorama_edit(origin, repo, op, args)` ⇒ 那台机器算出编辑计划（新内容），落盘经那台机器后端的
 *   文件管理（`files-put` 带 CAS / `files-delete`）。RM1c 那一拍的「远端仓只读、写入口当场拒」随之取消。
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

/** 〔RM1f〕一问被 `cancel` 撤掉时抛的那一个（界面按它认「是我撤的」，不当失败弹）。 */
export class PanoramaCancelled extends Error {
  constructor() {
    super("代码全景：这一问已取消");
    this.name = "PanoramaCancelled";
  }
}

/** 问那台机器的后端（本机 = `<local>`；`result` 的形状由 op 定）。给了 `cancel` ⇒ 带一张票，拨下就撤。 */
function remote<T>(
  at: { origin: Origin; path: string | null },
  op: string,
  args?: object,
  cancel?: AbortSignal,
): Promise<T> {
  const base = { origin: at.origin, op, repo: at.path, args: args ?? null };
  if (!cancel) return commands.panorama_call({ ...base, ticket: null }) as Promise<T>;
  if (cancel.aborted) return Promise.reject(new PanoramaCancelled());
  const ticket = `pano-${crypto.randomUUID()}`;
  const onAbort = (): void => {
    void commands.panorama_cancel({ ticket });
  };
  cancel.addEventListener("abort", onAbort, { once: true });
  return (commands.panorama_call({ ...base, ticket }) as Promise<T>).then(
    (v) => {
      cancel.removeEventListener("abort", onAbort);
      return v;
    },
    (e: unknown) => {
      cancel.removeEventListener("abort", onAbort);
      // 撤了之后那一问回的是「已取消」—— 换成类型，免得调用方去认那句话。
      throw cancel.aborted ? new PanoramaCancelled() : e;
    },
  );
}

/** 〔RM1d〕写：本机远端同一条（`op` 是 monitor `panorama_call.rs::EDITS` 第一列）。 */
function edit<T>(at: RepoAt, op: string, args: object): Promise<T> {
  return commands.panorama_edit({ origin: at.origin, repo: at.path, op, args }) as Promise<T>;
}

/** 两个 `RepoAt` 说的是不是同一个仓（同一台机器、同一个路径；两个都没有也算同一个）。 */
export const sameRepo = (a: RepoAt | null, b: RepoAt | null): boolean =>
  a === null || b === null ? a === b : a.origin === b.origin && a.path === b.path;

/** 界面上与「复制给 agent」里说这个仓的那一串：本机就是路径，远端带上机器名。 */
export const repoLabel = (at: RepoAt): string =>
  isLocalOrigin(at.origin) ? at.path : `${at.path}（远端 ${at.origin}）`;

/** 建索引（重活：tree-sitter 解析全仓 → SQLite）。开面板首次调 + loading。〔RM1f〕`cancel` 拨下 ⇒ 撤（本机远端都撤得掉）。 */
export const index = (at: RepoAt, cancel?: AbortSignal): Promise<IndexStats> =>
  remote(at, "index", undefined, cancel);

/** 重建索引（改代码后刷新；只写索引，非侵入）。刷新按钮调。〔RM1f〕`cancel` 同上。 */
export const reindex = (at: RepoAt, cancel?: AbortSignal): Promise<IndexStats> =>
  remote(at, "reindex", undefined, cancel);

/** 索引新鲜度 + 上次索引时间 + 符号总数。开面板时查一次决定是否需索引。 */
export const status = (at: RepoAt): Promise<PanoramaStatus> => remote(at, "status");

/** 项目全景（脊柱文件 + 子系统聚类 + 入口点 + 覆盖信号）。budget 控 token 预算裁剪。 */
export const overview = (at: RepoAt, budget?: number): Promise<Overview> =>
  remote(at, "overview", budget === undefined ? {} : { budget });

/**
 * F69（补 D20：代码分析默认关、每仓手动开启）：据 status 决定开面板时的动作。
 * `symbols===0` = 从未索引 = 未启用本仓分析 → `"enable-gate"`（显式手势才扫描，绝不自动扫）；
 * 否则（`symbols>0`，用户此前已启用）→ `"load"`（直接加载现有 overview，stale 靠手动「刷新」）。
 * 抽成纯函数是为了单测钉死「默认关」不被回归成开面板即自动扫描（D20 违规）。
 *
 * 〔RM1f〕`indexedAt === null` 也算「从未建完」：建索引被撤掉的那一趟会在索引里留下**一部分**符号
 * （真后端 × 真小程序现打：撤在 1.5 s 时 `symbols: 5622, indexedAt: null, stale: true`），
 * 只看 `symbols` 的话下次打开会被当成「已启用、陈旧」而**自动**重建 —— 用户刚撤掉的那一趟又自己跑起来了。
 */
export function panoramaLoadDecision(
  st: Pick<PanoramaStatus, "symbols" | "indexedAt">,
): "enable-gate" | "load" {
  return st.symbols === 0 || st.indexedAt === null ? "enable-gate" : "load";
}

/** 单符号详情（符号 + 直接 callers/callees + 关联文档）。symbol 用全限定 id。 */
export const node = (at: RepoAt, symbol: string): Promise<NodeView | null> =>
  remote(at, "node", { symbol });

/** 以某符号为心的双向邻域子图。 */
export const subgraph = (at: RepoAt, symbol: string, depth: number): Promise<SubGraph> =>
  remote(at, "subgraph", { symbol, depth });

/** 反向调用边（谁调用了它，BFS 到 depth）。 */
export const callers = (at: RepoAt, symbol: string, depth: number): Promise<Edge[]> =>
  remote(at, "callers", { symbol, depth });

/** 正向调用边（它调用了谁，BFS 到 depth）。 */
export const callees = (at: RepoAt, symbol: string, depth: number): Promise<Edge[]> =>
  remote(at, "callees", { symbol, depth });

/** 改动某符号的 blast-radius（反向可达的全部传递调用者）。 */
export const impact = (at: RepoAt, symbol: string): Promise<ImpactSet> =>
  remote(at, "impact", { symbol });

/** 按名子串搜符号 → 拿全限定 id（裸名不解析，先搜再查 node）。 */
export const search = (at: RepoAt, query: string, limit?: number): Promise<PanoramaSymbol[]> =>
  remote(at, "search", limit === undefined ? { query } : { query, limit });

/** 覆盖某符号的 `.md` 文档链接。 */
export const docsFor = (at: RepoAt, symbol: string): Promise<DocLink[]> =>
  remote(at, "docs_for", { symbol });

/**
 * PN1b：图种注册表（原样）。问那台机器上的小程序（两台的版本可以不同 ⇒ 按机器取，不共用一份）。
 * 〔RM1f〕本机也问本机的那一份（改前本机那份编在 monitor 里）。
 */
export const diagramKinds = (origin: Origin): Promise<DiagramKindInfo[]> =>
  remote({ origin, path: null }, "diagram_kinds");

/** PN1b：画一张图。`kind` 是注册表里的 id，本仓不写死。 */
export const diagram = (at: RepoAt, kind: string, request: DiagramRequest): Promise<PanoramaDiagram> =>
  remote(at, "diagram", { kind, request });

/** F71：列某文件的所有符号（点文件气泡 → 展开符号列表 → 点符号进详情）。 */
export const symbolsInFile = (at: RepoAt, file: string): Promise<PanoramaSymbol[]> =>
  remote(at, "symbols_in_file", { file });

/** F71：文档漂移（仓里 `.md` 指向的目标文件/符号已失效）。反映上次索引快照，刷新后新鲜。 */
export const drift = (at: RepoAt): Promise<DriftItem[]> => remote(at, "drift");

// === F72：批注 + 文档关联写（落被分析仓、人手势触发）。存储格式只在上游定义（SS-15）。 ===
// 〔RM1d〕本机远端同一条 `panorama_edit`：那台机器算、那台机器后端的文件管理写。

/** F72：人写批注（直接 Active）。`symbol` = 符号段（如 `f`），null = 文件级。回批注 id。 */
export const addAnnotation = (
  at: RepoAt,
  file: string,
  symbol: string | null,
  body: string,
  author: string,
): Promise<string> => edit(at, "add_annotation", { file, symbol, body, author });

/** F72：agent 提议批注（Proposed，需人 approve 才 Active）。回批注 id。 */
export const proposeAnnotation = (
  at: RepoAt,
  file: string,
  symbol: string | null,
  body: string,
  author: string,
): Promise<string> => edit(at, "propose_annotation", { file, symbol, body, author });

/** F72：批准一条 Proposed 批注 → Active。回它在不在。 */
export const approveAnnotation = (at: RepoAt, id: string): Promise<boolean> =>
  edit(at, "approve_annotation", { id });

/** F72：删批注。回它原本在不在。 */
export const removeAnnotation = (at: RepoAt, id: string): Promise<boolean> =>
  edit(at, "remove_annotation", { id });

/** F72：列全部批注（含 Proposed，审批队列用）。**读**。 */
export const listAnnotations = (at: RepoAt): Promise<Annotation[]> => remote(at, "list_annotations");

/** F72：把某 `.md` 关联到某符号（写 doc 的 frontmatter covers:，进仓可提交）。 */
export const writeDocLink = (at: RepoAt, doc: string, target: string): Promise<void> =>
  edit<unknown>(at, "write_doc_link", { doc, target }).then(() => undefined);

/** F72：删除某 `.md` 对某符号的关联。回它原本在不在。 */
export const removeDocLink = (at: RepoAt, doc: string, target: string): Promise<boolean> =>
  edit(at, "remove_doc_link", { doc, target });

/**
 * ⭐ P3 护城河缝：一组文件/行 → 命中的符号 id。`ranges` 空 → 整文件所有符号。
 * cc-monitor 从 jsonl 的 Edit/Write 拿「agent 刚改了哪些文件行」→ 高亮 = 「agent 正在改这几个节点」。
 * 远端会话的文件路径本来就是那台机器上的路径 ⇒ 远端仓照样问那台。
 */
export const touching = (
  at: RepoAt,
  files: string[],
  ranges: [number, number][],
): Promise<string[]> => remote(at, "touching", { files, ranges });
