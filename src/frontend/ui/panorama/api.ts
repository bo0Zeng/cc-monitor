/**
 * Batch15-P2：code-picture 全景后端命令的 invoke 封装（融合手册 §7.1）。
 *
 * 每个入口吃一个 [`RepoAt`]（**哪台机器上的哪个仓**，`设计/97 §6` ④）。返回类型是 core 直出的
 * snake_case 结构体（见 types.ts）—— 本机与远端**逐字同形**，渲染那一层只有一份。
 *
 * ## 〔RM1f · 本机对称〕一条路（用户 09-24 V108：选 B，「之后本机也走这条路、monitor 摘内嵌引擎」）
 *
 * 本机与远端同一条：〔MIG-3b 续〕`chan.call(origin, "panorama", …)` ⇒ 那台机器的后端 `panorama` 帧命令 ⇒ 后端经插件口起
 * 只装引擎的独立小程序，索引与图都在那台机器上算，线上只回结构化结果（不传源码、不传索引）。本机 = `<local>`。
 * 〔墓碑 —— RM1c 那一版这里是两条路：本机仍走 monitor 进程内那几条命令（per-repo Engine 池），远端才走 `panorama_call`。〕
 * - 〔RM1d · V110「引擎只算、文件管理来写」〕批注 / 文档关联的**写**本机远端同一条：
 *   那台后端的 `panorama-edit`（那台算出编辑计划，落盘经那台后端的文件管理：`files-put` 带 CAS / `files-delete`）。
 *
 * ## 〔MIG-3b 续 · 主会话 09-28 裁〕界面直问那台后端
 *
 * 读 `chan.call(origin, "panorama", {op, repo, args})`、写 `chan.call(origin, "panorama-edit", {repo, op, args})`，**不再经 monitor 那一跳转**
 * （原 Tauri 命令 `panorama_call` / `panorama_edit` / `panorama_cancel`〔散文墓碑〕删了）。
 * - **期限归发起方**：建索引那一档 960 s、其余 100 s（各比后端给小程序的期限多留一段回程，判据读后端源码对拍）。
 * - 〔PANO · V158「后端不带引擎知识」〕哪个 op 是哪一档、要的是哪一代小程序（`shape`），都取自与小程序同源的生成物
 *   `engine-contract.json`（小程序判据从它的 op 表与形状代号写出、对拍）；每问把 `shape` 带上，后端只拿它与 `--probe` 比。
 * - **撤单**：`cancel` 拨下 ⇒ 通道撤单过 webview 那一跳（`chan_cancel`）⇒ 后端撤掉处理器、小程序那一组子进程被杀。
 * - **没装 / 太旧**（对端回码 `not_installed` / `unsupported`）⇒ 请 monitor **放字节**（`panorama_place`：推到那台 / 放到本机），
 *   再问**一次**；仍这么说 ⇒ 如实说，不循环。
 * - 〔P7 · V158「长活要有进度」〕建索引那一问给了 `onProgress` ⇒ 先订那台的 `progress/<票>`、问的时候带上 `ticket`，
 *   那台的小程序每报一格（上游 `IndexProgress`）就交一格；答案照旧是那一问的应答。撤单照旧（撤了订阅一起撤）。
 */
import { commands } from "../ipc/commands";
import { chan, ChanError, type Item, type Sub } from "../../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, saidOf } from "../ipc/chan-caller";
import { isLocalOrigin, type Origin } from "../ipc/origin";
import type {
  Annotation,
  DiagramKindInfo,
  DiagramRequest,
  DocLink,
  DriftItem,
  Edge,
  ImpactSet,
  IndexPhase,
  IndexProgress,
  IndexStats,
  Neighborhood,
  NodeView,
  Overview,
  PanoramaDiagram,
  PanoramaStatus,
  Symbol as PanoramaSymbol,
  SymbolRef,
} from "./types";
import { copyText } from "../copy-table";
import CONTRACT from "./engine-contract.json";

/** 哪台机器上的哪个仓。`path` 是**那台机器上**的绝对路径。 */
export type RepoAt = { origin: Origin; path: string };

/** 〔RM1f〕一问被 `cancel` 撤掉时抛的那一个（界面按它认「是我撤的」，不当失败弹）。 */
export class PanoramaCancelled extends Error {
  constructor() {
    super("代码全景：这一问已取消");
    this.name = "PanoramaCancelled";
  }
}

/** 小程序自报的档（生成物；`long` = 建索引那一档，后端给它 900 s）。 */
const TIERS: Readonly<Record<string, string>> = CONTRACT.ops;
/** 要的那一代小程序（生成物里的形状代号；每问带上）。 */
export const SHAPE: string = CONTRACT.shape;
/** 建索引那一档：后端给小程序 900 s，再留 60 s 给回程（值归发起方，DL1）。 */
export const BUILD_BUDGET_MS = 960_000;
/** 其余查询：后端给小程序 60 s（另有 10 s 探测），再留 30 s。 */
export const QUERY_BUDGET_MS = 100_000;
/** 〔RM1e〕对端回这几个码 ⇒ 那台缺小程序 / 装的那份太旧 ⇒ 放字节再问一次（== 后端适配层映射出来的码，判据读后端源码两向）。 */
export const PUSH_ON: readonly string[] = ["not_installed", "unsupported"];

/** 这个 op 等多久（档取自生成物，不手写哪几个是长活）。 */
export const budgetFor = (op: string): number => (TIERS[op] === "long" ? BUILD_BUDGET_MS : QUERY_BUDGET_MS);

/** 那台后端比这条命令老（不认它）时那句话。 */
const OLD_BACKEND = copyText("panoramaApi.backend.tooOld");

/** 这一次失败是不是「那台缺小程序 / 太旧」（对端说了「不行」、码在 {@link PUSH_ON} 里）。 */
export function wantsBytes(e: unknown): boolean {
  if (!(e instanceof ChanError) || e.error.layer !== "peer" || e.error.why !== "refused") return false;
  const r = refusalOf(e.error.body);
  return r !== null && PUSH_ON.includes(r.code);
}

/**
 * 问一次；撞上「缺 / 旧」⇒ 放一次字节、再问一次。**放最多一次，问最多两次**（原 monitor 那一跳的「问 · 放 · 再问」，搬到发起方）。
 * `ask` / `place` 由调用方给（生产 = 通道 ＋ `panorama_place`；判据用替身数次数）。撤了（`cancel` 拨下）⇒ {@link PanoramaCancelled}。
 */
export async function askOrPlace(
  ask: () => Promise<Uint8Array>,
  place: () => Promise<unknown>,
  cancel?: AbortSignal,
): Promise<unknown> {
  const said = (e: unknown): Error =>
    cancel?.aborted === true ? new PanoramaCancelled() : new Error(saidOf(e, OLD_BACKEND));
  let first: unknown;
  try {
    return resultOf(await ask());
  } catch (e) {
    if (!wantsBytes(e)) throw said(e);
    first = e;
  }
  try {
    await place();
  } catch (e) {
    throw new Error(copyText("panoramaApi.place.failed", { said: saidOf(first, OLD_BACKEND), e: String(e) }));
  }
  try {
    return resultOf(await ask());
  } catch (e) {
    if (wantsBytes(e)) throw new Error(copyText("panoramaApi.place.stillSays", { said: saidOf(e, OLD_BACKEND) }));
    throw said(e);
  }
}

/** 〔P7〕建索引的进度流（与 Rust `event_replay.rs::PROGRESS_KIND` 同一个串；后面跟 `/<ticket>`）。 */
export const PROGRESS_KIND = "progress";
/** 进度流一开始给的 credit（每收一格还一格：一趟建索引至多四百来格，窗口不必开到那么大）。 */
const PROGRESS_WINDOW = 64;

/** 这一版认得的阶段（`Record<IndexPhase, …>` ⇒ 上游生成物多 / 少一个阶段，tsc 当场红）。 */
const PHASES: Readonly<Record<IndexPhase, true>> = { Parse: true, Link: true, Relink: true, Docs: true };

/** 〔P7〕严格收一格进度（上游 `IndexProgress`）：恰好三个键、阶段认得、`0 ≤ done ≤ total` 的整数；否则 `null`（那一格不画）。 */
export function decodeProgress(v: unknown): IndexProgress | null {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return null;
  const o = v as Record<string, unknown>;
  const keys = Object.keys(o).sort();
  const nat = (x: unknown): x is number => typeof x === "number" && Number.isInteger(x) && x >= 0;
  if (keys.join(",") !== "done,phase,total" || typeof o.phase !== "string" || !Object.prototype.hasOwnProperty.call(PHASES, o.phase)) return null;
  if (!nat(o.done) || !nat(o.total) || o.done > o.total) return null;
  return { phase: o.phase as IndexPhase, done: o.done, total: o.total };
}

/** 订那台的 `progress/<票>`：每收一格进度交 `on`、还一格 credit；丢了（gap）/ 看不见那台（unseen）都不管 —— 每格是整份快照。 */
async function followProgress(origin: Origin, ticket: string, on: (p: IndexProgress) => void): Promise<Sub> {
  let sub: Sub | null = null;
  const onItems = (items: Item[]): void => {
    let frames = 0;
    for (const it of items) {
      if (it.t !== "frame") continue;
      frames += 1;
      let v: unknown = null;
      try {
        v = JSON.parse(it.body);
      } catch {
        v = null;
      }
      const p = decodeProgress(v);
      if (p !== null) on(p);
    }
    if (frames > 0) sub?.want(frames);
  };
  sub = await chan.subscribe(origin, `${PROGRESS_KIND}/${ticket}`, null, PROGRESS_WINDOW, onItems);
  return sub;
}

/** 应答体 ⇒ 它的 JSON（`panorama` 是 `{result}`，`panorama-edit` 整份就是那个值）。 */
function resultOf(body: Uint8Array): unknown {
  return readJson(body);
}

/**
 * 问那台机器的后端（本机 = `<local>`；`result` 的形状由 op 定）。给了 `cancel` ⇒ 拨下就撤（撤单过通道那一跳）。
 * 〔P7〕给了 `onProgress` ⇒ 先订那台的进度流、载荷带上票（后端据它推 `progress` 帧），这一问结束（成 / 败 / 撤）就撤订。
 */
async function remote<T>(
  at: { origin: Origin; path: string | null },
  op: string,
  args?: object,
  cancel?: AbortSignal,
  onProgress?: (p: IndexProgress) => void,
): Promise<T> {
  if (cancel?.aborted === true) throw new PanoramaCancelled();
  const ticket = onProgress ? crypto.randomUUID() : null;
  // 进度只是给人看的：订不上（这一页的 webview 听不了）就不画进度，那一问照问 —— 不许因为它让建索引失败。
  const sub = onProgress && ticket ? await followProgress(at.origin, ticket, onProgress).catch(() => null) : null;
  try {
    const payload = { op, repo: at.path, args: args ?? null, shape: SHAPE };
    const body = jsonBody(ticket === null ? payload : { ...payload, ticket });
    // 每一问现造期限（放字节之后那一问重新起算，同原 monitor 那一跳）。
    const budget = (): ReturnType<typeof budgetWithin> => budgetWithin(budgetFor(op), cancel);
    const got = await askOrPlace(
      () => chan.call(at.origin, "panorama", body, budget()),
      () => commands.panorama_place({ origin: at.origin }),
      cancel,
    );
    const r = got !== null && typeof got === "object" ? (got as { result?: unknown }).result : undefined;
    if (r === undefined) throw new Error(copyText("panoramaApi.reply.noResult"));
    return r as T;
  } finally {
    sub?.stop();
  }
}

/** 小程序自报的写表（生成物）：「算」op → 写成之后要跑的 op（没有 = `null`）。 */
const PLANS: Readonly<Record<string, string | null>> = CONTRACT.plans;
/** 一次写最多问几趟「算」（`stale` 重算；== 后端 `assets::door::EDIT_ATTEMPTS`，判据读后端源码）。 */
const EDIT_ASKS = 3;

/**
 * 一次写的期限：至多三趟「算」（按那个「算」op 的档）＋ 写表里说了写成之后还要跑的那一个（按它的档）。
 * 〔PANO〕「哪几种写要刷」只住小程序的写表（生成物），这里按它逐个给，不再一律按最坏一形。
 */
export const editBudgetFor = (op: string): number => {
  const then = PLANS[op];
  return EDIT_ASKS * budgetFor(op) + (then ? budgetFor(then) : 0);
};

/** 〔RM1d〕写：本机远端同一条（〔PANO〕`op` 是小程序写表里的「算」op；那台后端照它自报的写表走）。 */
function edit<T>(at: RepoAt, op: string, args: object): Promise<T> {
  const body = jsonBody({ repo: at.path, op, args, shape: SHAPE });
  // 每一问现造期限（同 `remote`）。
  const budget = (): ReturnType<typeof budgetWithin> => budgetWithin(editBudgetFor(op));
  return askOrPlace(
    () => chan.call(at.origin, "panorama-edit", body, budget()),
    () => commands.panorama_place({ origin: at.origin }),
  ) as Promise<T>;
}

/** 两个 `RepoAt` 说的是不是同一个仓（同一台机器、同一个路径；两个都没有也算同一个）。 */
export const sameRepo = (a: RepoAt | null, b: RepoAt | null): boolean =>
  a === null || b === null ? a === b : a.origin === b.origin && a.path === b.path;

/** 界面上与「复制给 agent」里说这个仓的那一串：本机就是路径，远端带上机器名。 */
export const repoLabel = (at: RepoAt): string =>
  isLocalOrigin(at.origin) ? at.path : copyText("api.repoLabel.remote", { path: at.path, machine: at.origin });

/**
 * 建索引（重活：tree-sitter 解析全仓 → SQLite）。开面板首次调 + loading。〔RM1f〕`cancel` 拨下 ⇒ 撤（本机远端都撤得掉）。
 * 〔P7〕`onProgress`：那台小程序报的每一格进度（阶段 · 这一阶段做完几份 / 共几份）。
 */
export const index = (
  at: RepoAt,
  cancel?: AbortSignal,
  onProgress?: (p: IndexProgress) => void,
): Promise<IndexStats> => remote(at, "index", undefined, cancel, onProgress);

/** 重建索引（改代码后刷新；只写索引，非侵入）。刷新按钮调。〔RM1f〕`cancel` · 〔P7〕`onProgress` 同上。 */
export const reindex = (
  at: RepoAt,
  cancel?: AbortSignal,
  onProgress?: (p: IndexProgress) => void,
): Promise<IndexStats> => remote(at, "reindex", undefined, cancel, onProgress);

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

/** 以某符号为心的双向邻域，每个符号带距根几跳（〔PANO〕跳数由那台的小程序给，前端不算）。 */
export const neighborhood = (at: RepoAt, symbol: string, depth: number): Promise<Neighborhood> =>
  remote(at, "neighborhood", { symbol, depth });

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

/**
 * F72：人写批注（直接 Active）。`target` = **整个**符号 id（文件级批注 = 文件路径）；
 * 〔P7〕截 `@行号`、取文件段归上游（`SymbolRef::of`），前端不拆。回批注 id。
 */
export const addAnnotation = (at: RepoAt, target: string, body: string, author: string): Promise<string> =>
  edit(at, "plan_add_annotation", { target, body, author });

/** F72：agent 提议批注（Proposed，需人 approve 才 Active）。`target` 同 {@link addAnnotation}。回批注 id。 */
export const proposeAnnotation = (at: RepoAt, target: string, body: string, author: string): Promise<string> =>
  edit(at, "plan_propose_annotation", { target, body, author });

/** F72：批准一条 Proposed 批注 → Active。回它在不在。 */
export const approveAnnotation = (at: RepoAt, id: string): Promise<boolean> =>
  edit(at, "plan_approve_annotation", { id });

/** F72：删批注。回它原本在不在。 */
export const removeAnnotation = (at: RepoAt, id: string): Promise<boolean> =>
  edit(at, "plan_remove_annotation", { id });

/** F72：列全部批注（含 Proposed，审批队列用）。**读**。 */
export const listAnnotations = (at: RepoAt): Promise<Annotation[]> => remote(at, "list_annotations");

/** F72：把某 `.md` 关联到某符号（写 doc 的 frontmatter covers:，进仓可提交）。 */
export const writeDocLink = (at: RepoAt, doc: string, target: string): Promise<void> =>
  edit<unknown>(at, "plan_write_doc_link", { doc, target }).then(() => undefined);

/** F72：删除某 `.md` 对某符号的关联。回它原本在不在。 */
export const removeDocLink = (at: RepoAt, doc: string, target: string): Promise<boolean> =>
  edit(at, "plan_remove_doc_link", { doc, target });

/**
 * ⭐ P3 护城河缝：一组文件/行 → 命中的符号（〔P7〕带 `file`，上游给）。`ranges` 空 → 整文件所有符号。
 * cc-monitor 从 jsonl 的 Edit/Write 拿「agent 刚改了哪些文件行」→ 高亮 = 「agent 正在改这几个节点」。
 * 远端会话的文件路径本来就是那台机器上的路径 ⇒ 远端仓照样问那台。
 */
export const touching = (
  at: RepoAt,
  files: string[],
  ranges: [number, number][],
): Promise<SymbolRef[]> => remote(at, "touching", { files, ranges });
