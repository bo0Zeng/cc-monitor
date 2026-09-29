/**
 * 〔C4d · 第四波 4B〕**历史浏览器的清单与注解，经通道问本机常驻后端**（历史跨机 join 的唯一的家）。
 *
 * # 它顶掉了什么
 *
 * 此前这一族是 monitor 的五条 Tauri 命令：本机项目清单（exec 一次性本机后端 `--list-projects`、monitor 并注解 ＋ `SessionMap` 判活）·
 * 远端项目清单（monitor 逐台 fan-out、按行拿回再并注解）· 展开一个项目（本机 monitor 自己扫目录、远端按行拿回 —— 两套口径）·
 * 改注解（monitor 读改写 `history-metadata.json`）· 上次账号表。主会话 09-25 裁（`调研/第四波记录/C4d.md`「主会话裁」第 2 条）：
 * 注解的**读写者**换成本机常驻后端（文件原地不动），它经 `remote_ask` 问远端那台、并上注解、**出成品**；前端经 `chan.call`。
 * ⇒ 五条命令与 monitor 那一份 join / 注解读写一起删了；这里经通道问 `<local>` 那一台，按形状收。
 *
 * # 本文件做的只有三件（调用方那一侧的事，不是通信层成员）
 *
 * 1. **问谁**：一律问本机常驻后端（`<local>`）；要远端那台的清单就在请求里带 `origin`（本机后端沿它持有的那条 SSH 去问）。
 * 2. **按形状收**：成品逐格 == 从前 monitor 线上的 `HistoryProject` / `HistorySessionEntry`；多一格 / 缺一格 / 类型不对 ⇒ 抛
 *    （跨语言金样 `tests/__fixtures__/history-products.golden.json`）。
 * 3. **fan-out 远端**：「哪几台」问的是 `list_remote_mcp_origins`（同 `views/history-search.ts` 那一处）；逐台失败不拖垮其余台、
 *    记进 `failedHosts`（`F76`：部分失败不冻结缓存）；全部失败 ⇒ 抛（与「没配远端」的空表分开）。
 *
 * # 期限（`X6`：调用点显式给）
 *
 * 项目清单 30 秒（与上一个住址 monitor `frame_query::LINES_BUDGET` 同值）· 会话清单 120 秒（本机那一支从前是流式无期限，
 * 一次交全之后给足余量）· 注解两问 10 秒（读写一份小文件）。
 */
import { commands } from "./ipc/commands";
import { chan } from "./comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { copyText } from "./copy-table";

// ─── 成品的形状（后端 `history_join.rs`；逐格 == 从前 monitor 那两个 ts-rs 生成物）───

/** 项目级那一行（不含会话内容）。 */
export interface HistoryProject {
  projectPath: string;
  projectName: string;
  /** 懒加载的键（记录树的项目目录名，或合成历史的 `<kind>:<cwd>`），原样交回 [`fetchSessions`]。 */
  projectDir: string;
  sessionCount: number;
  /** `K-R92`：`null` = 不知道（**不是 0**）。 */
  starredCount: number | null;
  hiddenCount: number | null;
  /** 毫秒。 */
  lastActivity: number;
  /** `null` = 这条路上答不了（远端 · Codex），**不是**「没有活会话」。 */
  hasLive: boolean | null;
  /** 远端那台的名字；本机那一行没有这一格。 */
  origin?: string;
}

/** 会话级那一行。 */
export interface HistorySessionEntry {
  sessionId: string;
  projectPath: string;
  projectName: string;
  aiTitle: string | null;
  firstUserExcerpt: string;
  startedAt: number;
  updatedAt: number;
  jsonlPath: string;
  /** `null` = 这条路上答不了活状态。 */
  isLive: boolean | null;
  messageCountApprox: number;
  isBg: boolean;
  starred: boolean;
  customTitle: string | null;
  hidden: boolean;
  forkedFromSessionId?: string;
  forkedFromMessageUuid?: string;
  origin?: string;
}

/** 一条注解（`history-annotate` 回的那一条）。 */
export interface EntryMetadata {
  starred: boolean;
  customTitle: string | null;
  hidden: boolean;
  updatedAt: number;
  lastAccount: string | null;
}

/** 远端那一批：项目 ＋ 失败的那几台。 */
export interface RemoteProjectsResult {
  projects: HistoryProject[];
  failedHosts: string[];
}

const PROJECTS_BUDGET_MS = 30_000;
const SESSIONS_BUDGET_MS = 120_000;
const ANNOTATION_BUDGET_MS = 10_000;

// ─── 收货验形 ───

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const isStr = (v: unknown): v is string => typeof v === "string";
const isNum = (v: unknown): v is number =>
  typeof v === "number" && Number.isFinite(v);
const isBool = (v: unknown): v is boolean => typeof v === "boolean";
const orNull =
  <T>(p: (v: unknown) => v is T) =>
  (v: unknown): v is T | null =>
    v === null || p(v);

/** 形状不对。给人看的那句不带内部名；哪一格不对只进 `detail`（日志 —— 〔CP2b〕只进日志的细目用英文写，不进文案表）。 */
export class HistoryShapeError extends Error {
  readonly detail: string;
  constructor(what: string) {
    super(
      copyText("historyReads.shape.unreadable"),
    );
    this.name = "HistoryShapeError";
    this.detail = what;
  }
}

/** 键集合恰好是 `must` ＋（可选）`may` 里的。 */
function keysOk(
  o: Record<string, unknown>,
  must: readonly string[],
  may: readonly string[],
): boolean {
  const keys = Object.keys(o);
  return (
    must.every((k) => k in o) &&
    keys.every((k) => must.includes(k) || may.includes(k))
  );
}

const PROJECT_KEYS = [
  "projectPath",
  "projectName",
  "projectDir",
  "sessionCount",
  "starredCount",
  "hiddenCount",
  "lastActivity",
  "hasLive",
] as const;
const SESSION_KEYS = [
  "sessionId",
  "projectPath",
  "projectName",
  "aiTitle",
  "firstUserExcerpt",
  "startedAt",
  "updatedAt",
  "jsonlPath",
  "isLive",
  "messageCountApprox",
  "isBg",
  "starred",
  "customTitle",
  "hidden",
] as const;
const ENTRY_KEYS = [
  "starred",
  "customTitle",
  "hidden",
  "updatedAt",
  "lastAccount",
] as const;

/** 成品外壳 `{rows, notice}`。 */
function decodeShell(
  v: unknown,
  op: string,
): { rows: unknown[]; notice: string | null } {
  if (
    !isObj(v) ||
    !keysOk(v, ["rows", "notice"], []) ||
    !Array.isArray(v.rows) ||
    !orNull(isStr)(v.notice)
  ) {
    throw new HistoryShapeError(`${op} reply is not exactly {rows, notice}`);
  }
  return { rows: v.rows, notice: v.notice };
}

/** `history-projects` 的成品 ⇒ 项目行。 */
export function decodeProjects(v: unknown): {
  projects: HistoryProject[];
  notice: string | null;
} {
  const { rows, notice } = decodeShell(v, "history-projects");
  const projects = rows.map((r): HistoryProject => {
    const ok =
      isObj(r) &&
      keysOk(r, PROJECT_KEYS, ["origin"]) &&
      isStr(r.projectPath) &&
      isStr(r.projectName) &&
      isStr(r.projectDir) &&
      isNum(r.sessionCount) &&
      orNull(isNum)(r.starredCount) &&
      orNull(isNum)(r.hiddenCount) &&
      isNum(r.lastActivity) &&
      orNull(isBool)(r.hasLive) &&
      (r.origin === undefined || isStr(r.origin));
    if (!ok)
      throw new HistoryShapeError(
        `history-projects row has the wrong shape: ${JSON.stringify(r)}`,
      );
    return r as unknown as HistoryProject;
  });
  return { projects, notice };
}

/** `history-sessions` 的成品 ⇒ 会话行。 */
export function decodeSessions(v: unknown): {
  sessions: HistorySessionEntry[];
  notice: string | null;
} {
  const { rows, notice } = decodeShell(v, "history-sessions");
  const sessions = rows.map((r): HistorySessionEntry => {
    const ok =
      isObj(r) &&
      keysOk(r, SESSION_KEYS, [
        "forkedFromSessionId",
        "forkedFromMessageUuid",
        "origin",
      ]) &&
      isStr(r.sessionId) &&
      isStr(r.projectPath) &&
      isStr(r.projectName) &&
      orNull(isStr)(r.aiTitle) &&
      isStr(r.firstUserExcerpt) &&
      isNum(r.startedAt) &&
      isNum(r.updatedAt) &&
      isStr(r.jsonlPath) &&
      orNull(isBool)(r.isLive) &&
      isNum(r.messageCountApprox) &&
      isBool(r.isBg) &&
      isBool(r.starred) &&
      orNull(isStr)(r.customTitle) &&
      isBool(r.hidden) &&
      (r.forkedFromSessionId === undefined) ===
        (r.forkedFromMessageUuid === undefined) &&
      (r.forkedFromSessionId === undefined || isStr(r.forkedFromSessionId)) &&
      (r.forkedFromMessageUuid === undefined ||
        isStr(r.forkedFromMessageUuid)) &&
      (r.origin === undefined || isStr(r.origin));
    if (!ok)
      throw new HistoryShapeError(
        `history-sessions row has the wrong shape: ${JSON.stringify(r)}`,
      );
    return r as unknown as HistorySessionEntry;
  });
  return { sessions, notice };
}

/** `history-annotate` 的成品 ⇒ 那一条注解。 */
export function decodeEntry(v: unknown): EntryMetadata {
  const e = isObj(v) && keysOk(v, ["entry"], []) ? v.entry : undefined;
  const ok =
    isObj(e) &&
    keysOk(e, ENTRY_KEYS, []) &&
    isBool(e.starred) &&
    orNull(isStr)(e.customTitle) &&
    isBool(e.hidden) &&
    isNum(e.updatedAt) &&
    orNull(isStr)(e.lastAccount);
  if (!ok)
    throw new HistoryShapeError(
      `history-annotate reply has the wrong shape: ${JSON.stringify(v)}`,
    );
  return e as unknown as EntryMetadata;
}

/** `history-last-accounts` 的成品 ⇒ sid → 账号名。 */
export function decodeLastAccounts(v: unknown): Record<string, string> {
  const m = isObj(v) && keysOk(v, ["accounts"], []) ? v.accounts : undefined;
  if (!isObj(m) || !Object.values(m).every(isStr)) {
    throw new HistoryShapeError(
      `history-last-accounts reply has the wrong shape: ${JSON.stringify(v)}`,
    );
  }
  return m as Record<string, string>;
}

/** 一次失败 ⇒ 给人看的那句话（按层说的那一份住 `chan-caller.ts::saidOf`；形状不对时细目进日志）。 */
export function historyReasonOf(e: unknown): string {
  if (e instanceof HistoryShapeError)
    console.warn(`[history-reads] ${e.detail}`);
  return saidOf(e, copyText("historyReads.reason.fallback"));
}

// ─── 问 ───

/** 本机的项目清单（记录树 ＋ Codex 合成）。失败 ⇒ 抛（给人看的那句见 [`historyReasonOf`]）。 */
export async function fetchLocalProjects(): Promise<{
  projects: HistoryProject[];
  notice: string | null;
}> {
  const body = jsonBody({});
  const budget = budgetWithin(PROJECTS_BUDGET_MS);
  const reply = await chan.call(LOCAL_ORIGIN, "history-projects", body, budget);
  return decodeProjects(readJson(reply));
}

/**
 * 远端各台的项目清单（本机后端逐台去问）。没配远端 ⇒ 空；逐台失败 ⇒ 进 `failedHosts`、其余照收；
 * **全部**失败 ⇒ 抛（与「没配远端」区分开）。
 */
export async function fetchRemoteProjects(): Promise<RemoteProjectsResult> {
  const origins = await commands.list_remote_mcp_origins();
  if (origins.length === 0) return { projects: [], failedHosts: [] };
  const per = await Promise.all(
    origins.map(async (origin) => {
      try {
        const body = jsonBody({ origin });
        const budget = budgetWithin(PROJECTS_BUDGET_MS);
        const reply = await chan.call(
          LOCAL_ORIGIN,
          "history-projects",
          body,
          budget,
        );
        const { projects } = decodeProjects(readJson(reply));
        return { origin, projects, error: null as string | null };
      } catch (e) {
        console.warn(`远端 [${origin}] 历史项目清单没拿到（跳过这台）:`, e);
        return {
          origin,
          projects: [] as HistoryProject[],
          error: historyReasonOf(e),
        };
      }
    }),
  );
  const failed = per.filter((p) => p.error !== null);
  if (failed.length === per.length) {
    throw new Error(
      copyText("historyReads.remote.allFailed", { count: per.length, error: String(failed[failed.length - 1].error) }),
    );
  }
  return {
    projects: per.flatMap((p) => p.projects),
    failedHosts: failed.map((p) => p.origin),
  };
}

/** 一个项目下的会话（`origin` 缺席 = 本机那一行）。 */
export async function fetchSessions(proj: {
  projectDir: string;
  origin?: string;
}): Promise<{ sessions: HistorySessionEntry[]; notice: string | null }> {
  const args: Record<string, unknown> = { project_dir: proj.projectDir };
  if (proj.origin) args.origin = proj.origin;
  const body = jsonBody(args);
  const budget = budgetWithin(SESSIONS_BUDGET_MS);
  const reply = await chan.call(LOCAL_ORIGIN, "history-sessions", body, budget);
  return decodeSessions(readJson(reply));
}

/** 改一条注解（星标 / 改名 / 隐藏 / 上次账号）。缺格或 `null` = 不改；标题 / 账号名给空白串 = 清空。 */
export async function annotate(
  sessionId: string,
  patch: {
    starred?: boolean;
    customTitle?: string | null;
    hidden?: boolean;
    lastAccount?: string | null;
  },
): Promise<EntryMetadata> {
  const body = jsonBody({ sid: sessionId, patch });
  const budget = budgetWithin(ANNOTATION_BUDGET_MS);
  const reply = await chan.call(LOCAL_ORIGIN, "history-annotate", body, budget);
  return decodeEntry(readJson(reply));
}

/** 删会话之后连带删那一条注解（删不掉只记一行：孤儿注解不害人）。 */
export async function forgetAnnotation(sessionId: string): Promise<void> {
  try {
    const body = jsonBody({ sid: sessionId });
    const budget = budgetWithin(ANNOTATION_BUDGET_MS);
    await chan.call(LOCAL_ORIGIN, "history-forget", body, budget);
  } catch (e) {
    console.warn(`会话 ${sessionId} 删了，它那条注解没删掉（留着不害人）:`, e);
  }
}

/** sid → 上次用哪个号起（只含真记过的那几条）。 */
export async function lastAccounts(): Promise<Record<string, string>> {
  const body = jsonBody({});
  const budget = budgetWithin(ANNOTATION_BUDGET_MS);
  const reply = await chan.call(
    LOCAL_ORIGIN,
    "history-last-accounts",
    body,
    budget,
  );
  return decodeLastAccounts(readJson(reply));
}
