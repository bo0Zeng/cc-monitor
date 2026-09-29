/**
 * 〔C4a〕判据里假扮 monitor 那一跳（包装层 `chan_call`）的几样小工具。
 *
 * 生产上 `chan.call` ⇒ `commands.chan_call` ⇒ `invoke("chan_call", { origin, op, payload, leftMs })`，
 * monitor 回原样字节（`ArrayBuffer`）或 `{ err, body }`（`wire::err_to_wire` 的线上形状）。
 * 各判据 mock 的是 `@tauri-apps/api/core` 的 `invoke` —— 本文件只帮它们造「那一跳会回什么」。
 */
import ACCT_ISO_CMD_GOLDEN from "../__fixtures__/acct-iso-cmd.golden.json";

/** `chan_call` 的实参（判据按 `op` 分派）。 */
export interface ChanCallArgs {
  origin: string;
  op: string;
  payload: number[];
  leftMs: number;
  /** 〔MIG-3b 续〕带撤单的那一问的编号（撤单那一条 `chan_cancel` 按它找）；不带撤单 ⇒ `null`。 */
  callId?: string | null;
}

/** 这一发 `invoke` 是不是通道那一跳、问的是不是这条帧命令。 */
export function isChanCall(cmd: string, args: unknown, op: string): args is ChanCallArgs {
  return cmd === "chan_call" && (args as ChanCallArgs | undefined)?.op === op;
}

/** 一个 JSON 值 ⇒ monitor 交回的原样字节。 */
export function chanReply(v: unknown): ArrayBuffer {
  const u = new TextEncoder().encode(JSON.stringify(v));
  return u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength) as ArrayBuffer;
}

/** 按行那一族（`{"lines": [...]}`）的应答。 */
export function linesReply(rows: unknown[]): ArrayBuffer {
  return chanReply({ lines: rows.map((r) => (typeof r === "string" ? r : JSON.stringify(r))) });
}

/** 「那台没有控制通道」（`Hop{1 open, NotSent, Unreachable}`）—— monitor 那一跳会拒的那个值。 */
export const NO_CHANNEL = {
  err: { Hop: { idx: 1, tag: "open", reach: "NotSent", why: "Unreachable" } },
  body: [] as number[],
};

/** 一发 `chan_call` 的请求体解回 JSON。 */
export function chanArgsJson(args: ChanCallArgs): unknown {
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(args.payload)));
}

// ════════════════════════════════════════════════════════════════════════════
//  〔C4b · 第四波 4B〕会话读面三条（骨架索引 · 大纲清单 · 会话内查找）改走通道之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 这三条从前是三条 Tauri 命令（`read_session_index` / `list_user_inputs` / `find_in_session`〔散文墓碑〕），
// 判据按命令名答话、按旧形参断言。今天它们是一发 `chan_call`（op = 帧命令名）⇒ 本节把一发 `chan_call`
// 译回「哪一问 ＋ 那一问的参数」，把判据手里那份回包译成**后端的成品字节**（或通道的失败）。
// ⚠ 译法逐格照生产：请求体的键名是 `src/frontend/ui/session-reads.ts` 发的那几个，成品的键名是后端 `read_face.rs` 出的那几个
//   （跨语言金样 `tests/__fixtures__/session-reads.golden.json` 钉着两侧）。

/** 三问各自的名字（判据里的叫法 = 旧命令名，只为让断言读起来是「哪一问」）。 */
export type SessionRead = "read_session_index" | "list_user_inputs" | "find_in_session" | "probe_session_record";

const READ_OPS: Record<string, SessionRead> = {
  "history-index": "read_session_index",
  "history-user-inputs": "list_user_inputs",
  "history-find": "find_in_session",
  // 〔C4c · 第四波 4B〕第四问：resume 之前问记录还在不在（旧命令 `probe_session_record`）。
  "history-record": "probe_session_record",
};

/** 一发 `chan_call` 若是这三问之一 ⇒ `[哪一问, 那一问的参数（旧形参的形状）]`；否则 `null`。 */
export function sessionReadOf(cmd: string, args: unknown): [SessionRead, Record<string, unknown>] | null {
  if (cmd !== "chan_call") return null;
  const a = args as ChanCallArgs;
  const which = READ_OPS[a.op];
  if (!which) return null;
  const body = chanArgsJson(a) as Record<string, unknown>;
  const origin = a.origin;
  switch (which) {
    case "read_session_index":
      return [which, { origin, jsonlPath: body.path, fromOffset: body.offset }];
    case "list_user_inputs":
      return [which, { origin, jsonlPath: body.path, fromOffset: body.from }];
    case "find_in_session":
      return [which, { origin, jsonlPath: body.path, query: body.query, includeTools: body.include_tools }];
    case "probe_session_record":
      // 〔GP1 · 第四波〕这次 resume 要用的账号根（基座不带 ⇒ `undefined`）。
      return [which, { origin, sessionId: body.sid, configDir: body.configDir }];
  }
}

/** mock 过的 `invoke` 的调用记录里，某一问的那几发（参数是旧形参的形状）。 */
export function sessionReadCalls(calls: ReadonlyArray<readonly unknown[]>, which: SessionRead): Record<string, unknown>[] {
  return calls
    .map((c) => sessionReadOf(c[0] as string, c[1]))
    .filter((r): r is [SessionRead, Record<string, unknown>] => r !== null && r[0] === which)
    .map((r) => r[1]);
}

/** 通道那一跳「对端说不认」（`Peer{Unsupported}`）—— 那台后端比这条查询老。 */
export const UNSUPPORTED = { err: "Unsupported", body: [] as number[] };

/** 通道那一跳「对端说不行」（`Peer{Refused}`，体是后端的 `{code, message}`）。 */
export function refusedReply(code: string, message: string): { err: string; body: number[] } {
  return { err: "Refused", body: Array.from(new TextEncoder().encode(JSON.stringify({ code, message }))) };
}

// ════════════════════════════════════════════════════════════════════════════
//  〔DUP2 · J4〕帧命令 `acct-iso-cmd`（cc-acct-iso 步骤那一行由后端出）：「后端会怎么答」从跨语言金样里查
// ════════════════════════════════════════════════════════════════════════════
//
// 金样 `tests/__fixtures__/acct-iso-cmd.golden.json` 的每一问，后端那侧逐条喂给生产的 `answer_wire_cmd` 对过
// （`tests/backend/accounts/iso_tests.rs`）⇒ 这里**不在 JS 里再写一份渲染器**，只照金样答；金样里没有的请求当场抛
// （判据该拿金样里的请求来问 —— 否则就是在 JS 里偷偷长出第二份命令构造）。

interface AcctIsoCmdCase {
  args: Record<string, unknown>;
  cmd?: string;
  code?: string;
}
const ACCT_ISO_CMD_CASES = (ACCT_ISO_CMD_GOLDEN as { cases: AcctIsoCmdCase[] }).cases;

const canon = (v: unknown): string =>
  JSON.stringify(v, (_k, x: unknown) =>
    x && typeof x === "object" && !Array.isArray(x)
      ? Object.fromEntries(Object.entries(x as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)))
      : x,
  );

/** 金样里「这一问」那一条（按请求体逐格相等找）；没有 ⇒ 抛。 */
export function acctIsoCmdCase(args: ChanCallArgs): AcctIsoCmdCase {
  const want = canon(chanArgsJson(args));
  const hit = ACCT_ISO_CMD_CASES.find((c) => canon(c.args) === want);
  if (!hit) throw new Error(`acct-iso-cmd 金样里没有这一问：${want}（判据请用金样里的请求）`);
  return hit;
}

/** 一发 `acct-iso-cmd` 的 `chan_call` ⇒ monitor 那一跳会回什么（成品字节；拒 ⇒ reject 通道的拒绝体）。 */
export function acctIsoCmdInvoke(args: ChanCallArgs): Promise<ArrayBuffer> {
  const c = acctIsoCmdCase(args);
  return c.cmd !== undefined
    ? Promise.resolve(chanReply({ cmd: c.cmd }))
    : Promise.reject(refusedReply(c.code ?? "refused", `（金样）${c.code ?? "refused"}`));
}

/**
 * 判据手里那份回包（旧回包的形状：`{available, …}`）⇒ 通道那一跳的结局：
 * 可用 ⇒ 后端的成品字节；不可用 ⇒ 按它的种类拒（`oldBackend` ⇒ 对端不认 · `truncated` ⇒ 对端说装不下（`too_large`）·
 * 其余 ⇒ 对端说不行（`failed`）；原因原样带上）。
 * 回包是 `undefined` ⇒ 原样 `undefined`（让「形状不对」那一格照样可测）。
 */
export async function sessionReadReply(which: SessionRead, res: unknown): Promise<ArrayBuffer | undefined> {
  const r = (await res) as Record<string, unknown> | undefined;
  if (r === undefined) return undefined;
  // 〔C4c〕记录那一问的旧回包本来就是成品的形状（`{present, root}`，没有 `available` 那一格）。
  if (which === "probe_session_record") return chanReply({ present: r.present, root: r.root });
  if (r.available === false) {
    const reason = String(r.reason ?? "");
    if (r.failure === "oldBackend") throw UNSUPPORTED;
    throw refusedReply(r.failure === "truncated" ? "too_large" : "failed", reason);
  }
  switch (which) {
    case "read_session_index":
      return chanReply({ from: r.from, end: r.end, rows: r.rows });
    case "list_user_inputs":
      return chanReply({ from: r.from, end: r.end, entries: r.entries });
    case "find_in_session":
      return chanReply({ total: r.total, hits: r.hits });
  }
  return undefined;
}

/**
 * 包一层判据的 `invoke` 替身：这三问照旧按旧名字交给 `answer`（它回旧回包的形状），本层把一发 `chan_call`
 * 译过去、把回包译回成品字节；其余一切原样交给 `answer`。
 */
export function withSessionReads(
  answer: (cmd: string, args: Record<string, unknown>) => unknown,
): (cmd: string, args?: unknown) => Promise<unknown> {
  return async (cmd, args) => {
    const read = sessionReadOf(cmd, args);
    if (read) return sessionReadReply(read[0], answer(read[0], read[1]));
    // 〔MOD〕会话正文那几问同样译回旧名字（见下面那一节）。
    const rec = recordReadOf(cmd, args);
    if (rec) return recordReadReply(rec[0], rec[1], answer);
    return answer(cmd, (args ?? {}) as Record<string, unknown>);
  };
}

// ════════════════════════════════════════════════════════════════════════════
//  〔MOD · `设计/90 §3` 判据 3〕会话正文那四问改走通道之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 从前是四条 Tauri 命令（`stream_read_session_jsonl` · `read_session_range` · `read_session_lines` · `load_subagent`〔散文墓碑〕），
// 判据按命令名答话、回旧回包的形状（载荷数组 · `{from, next, eof, payloads}` · `{path, agent_id, records}`）。今天它们是一发
// `chan_call`（op = `history-page` / `history-lines` / `history-subagent`，`src/frontend/ui/record-reads.ts` 发）⇒ 本节译回「哪一问 ＋ 旧形参」，
// 把判据手里那份旧回包译成**后端的成品字节**（键名照后端 `observe/record_page.rs` 出的那几个）。

/** 四问各自的名字（判据里的叫法 = 旧命令名）。 */
export type RecordRead = "stream_read_session_jsonl" | "read_session_range" | "read_session_lines" | "load_subagent";

/** 一发 `chan_call` 若是这四问之一 ⇒ `[哪一问, 那一问的参数（旧形参的形状）]`；否则 `null`。 */
export function recordReadOf(cmd: string, args: unknown): [RecordRead, Record<string, unknown>] | null {
  if (cmd !== "chan_call") return null;
  const a = args as ChanCallArgs;
  const origin = a.origin;
  if (a.op === "history-page") {
    const b = chanArgsJson(a) as Record<string, unknown>;
    if (b.whole === true) return ["stream_read_session_jsonl", { origin, jsonlPath: b.path }];
    return ["read_session_range", { origin, jsonlPath: b.path, offset: b.offset, until: b.until, seqBase: b.seq }];
  }
  if (a.op === "history-lines") {
    const b = chanArgsJson(a) as Record<string, unknown>;
    // `leftMs`：旧命令收的是调用方给的那个数；今天是通道那一跳现算的「还剩多少」（造期限到过线之间走的那零点几毫秒会让它
    //   比整数少一点）⇒ 取到 10 ms 译回旧形参，判据照旧按那一件的整份比。
    const leftMs = Math.round(a.leftMs / 10) * 10;
    const out: Record<string, unknown> = { origin, jsonlPath: b.path, from: b.from, leftMs };
    if (b.until !== undefined) out.until = b.until;
    return ["read_session_lines", out];
  }
  if (a.op === "history-subagent") {
    const b = chanArgsJson(a) as Record<string, unknown>;
    return ["load_subagent", { parentJsonlPath: b.parent, description: b.description, toolUseTimestamp: b.timestamp, origin }];
  }
  return null;
}

/** mock 过的 `invoke` 的调用记录里，某一问的那几发（参数是旧形参的形状）。 */
export function recordReadCalls(calls: ReadonlyArray<readonly unknown[]>, which: RecordRead): Record<string, unknown>[] {
  return calls
    .map((c) => recordReadOf(c[0] as string, c[1]))
    .filter((r): r is [RecordRead, Record<string, unknown>] => r !== null && r[0] === which)
    .map((r) => r[1]);
}

/** 旧载荷（`JsonlLinePayload`）⇒ 后端的一条记录行（恰好那五格；`origin` / `skipped_from` 不在后端的成品里）。 */
function recordLine(p: Record<string, unknown>): Record<string, unknown> {
  return { session_id: p.session_id, path: p.path, seq: p.seq, cwd: p.cwd ?? null, message: p.message };
}

/**
 * 判据手里那份旧回包 ⇒ 通道那一跳的结局。整份读那一问从前经 `Channel` 交块：这里给 `answer` 一个假 `Channel`、
 * 收下它灌进来的块，再一次交成一页（`eof`）。回包是一次拒绝 ⇒ 对端说不行（`failed`，原因原样）。
 */
export async function recordReadReply(
  which: RecordRead,
  args: Record<string, unknown>,
  answer: (cmd: string, args: Record<string, unknown>) => unknown,
): Promise<ArrayBuffer | undefined> {
  const chunks: Record<string, unknown>[] = [];
  const onChunk = { onmessage: (v: unknown) => chunks.push(...(v as Record<string, unknown>[])) };
  let r: unknown;
  try {
    r = await answer(which, which === "stream_read_session_jsonl" ? { ...args, onChunk } : args);
  } catch (e) {
    throw refusedReply("failed", e instanceof Error ? e.message : String(e));
  }
  switch (which) {
    case "stream_read_session_jsonl": {
      const lines = chunks.map(recordLine);
      return chanReply({ lines, next: 1, nextSeq: lines.length, eof: true });
    }
    case "read_session_range": {
      if (r === undefined) return undefined;
      const lines = (r as Record<string, unknown>[]).map(recordLine);
      const next = Number(args.until);
      return chanReply({ lines, next, nextSeq: Number(args.seqBase) + lines.length, eof: true });
    }
    case "read_session_lines": {
      if (r === undefined) return undefined;
      const page = r as { from: number; next: number; eof: boolean; payloads: Record<string, unknown>[] };
      return chanReply({ from: page.from, next: page.next, eof: page.eof, lines: page.payloads.map(recordLine) });
    }
    case "load_subagent":
      return r === undefined ? undefined : chanReply(r);
  }
}

// ════════════════════════════════════════════════════════════════════════════
//  〔C4c · 第四波 4B〕账号那两问（清单 · 信任预检）改走通道之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 它们从前是三条 Tauri 命令（`list_remote_accounts` / `list_local_accounts` / `check_account_trust`〔散文墓碑〕），
// 判据按命令名答话、回旧回包的形状（`{available, error, meta, accounts, notice}` / `{available, trusted, known, error}`）。
// 今天它们是一发 `chan_call`（op = `accounts-list` / `accounts-trust`）⇒ 本节把一发 `chan_call` 译回「哪一问 ＋ 旧形参」，
// 把判据手里那份旧回包译成**后端的成品字节**（或通道的失败）。
// ⚠ 译法逐格照生产：请求体键名是 `src/frontend/ui/accounts.ts` 发的那几个，成品键名是后端 `observe/accounts_query.rs::list_product`
//   出的那几个（跨语言金样 `tests/__fixtures__/accounts.golden.json` 钉着两侧）。
// ⚠ 旧回包里「老后端」那几种缺格（账号缺 `authKind` / `authReady`、`meta: null`）在成品里不存在 ⇒ 按旧消费侧的回落补齐：
//   `authKind` 缺 ⇒ 订阅 · `authReady` 缺 ⇒ `loggedIn`（`accountReady` 那条旧回落）· `meta: null` ⇒ 没启用（`deriveUi` 那一档）。

/** 账号那两问各自的名字（判据里的叫法 = 旧命令名，只为让断言读起来是「哪一问」）。 */
export type AccountRead = "list_remote_accounts" | "list_local_accounts" | "check_account_trust";

/** 一发 `chan_call` 若是账号那两问之一 ⇒ `[哪一问, 那一问的参数（旧形参的形状）]`；否则 `null`。 */
export function accountReadOf(cmd: string, args: unknown): [AccountRead, Record<string, unknown>] | null {
  if (cmd !== "chan_call") return null;
  const a = args as ChanCallArgs;
  if (a.op === "accounts-list") {
    return a.origin === "<local>" ? ["list_local_accounts", {}] : ["list_remote_accounts", { origin: a.origin }];
  }
  if (a.op === "accounts-trust") {
    const body = chanArgsJson(a) as Record<string, unknown>;
    return ["check_account_trust", { origin: a.origin, configDir: body.configDir, cwd: body.cwd }];
  }
  return null;
}

/** mock 过的 `invoke` 的调用记录里，某一问的那几发（参数是旧形参的形状）。 */
export function accountReadCalls(calls: ReadonlyArray<readonly unknown[]>, which: AccountRead): Record<string, unknown>[] {
  return calls
    .map((c) => accountReadOf(c[0] as string, c[1]))
    .filter((r): r is [AccountRead, Record<string, unknown>] => r !== null && r[0] === which)
    .map((r) => r[1]);
}

const ACCOUNT_KEYS = ["name", "email", "configDir", "isDefault", "mode", "exists", "loggedIn"] as const;

/** 旧回包里的一个账号 ⇒ 成品里的一个账号（缺的两格按旧消费侧的回落补齐，多余的键丢掉）。 */
function productAccount(a: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const k of ACCOUNT_KEYS) out[k] = a[k];
  out.email = a.email ?? "";
  out.configDir = a.configDir ?? null;
  out.isDefault = a.isDefault ?? false;
  out.mode = a.mode ?? "isolated";
  out.exists = a.exists ?? true;
  out.loggedIn = a.loggedIn ?? false;
  out.authKind = a.authKind ?? "subscription";
  out.authReady = a.authReady ?? out.loggedIn;
  return out;
}

/**
 * 判据手里那份旧回包 ⇒ 通道那一跳的结局。`available:false` ⇒ 对端说不行（`failed`，原因原样带上）；
 * 回包本身是一次拒绝（旧判据用 `mockRejectedValue` 表示「invoke 抛了」）⇒ 同样按对端说不行；
 * 回包是 `undefined` ⇒ 原样 `undefined`（让「形状不对」那一格照样可测）。
 */
export async function accountReadReply(which: AccountRead, res: unknown): Promise<ArrayBuffer | undefined> {
  let r: Record<string, unknown> | undefined;
  try {
    r = (await res) as Record<string, unknown> | undefined;
  } catch (e) {
    throw refusedReply("failed", e instanceof Error ? e.message : String(e));
  }
  if (r === undefined) return undefined;
  if (r.available === false) throw refusedReply("failed", String(r.error ?? ""));
  if (which === "check_account_trust") return chanReply({ trusted: r.trusted ?? false, known: r.known ?? false });
  const accounts = Array.isArray(r.accounts) ? (r.accounts as Record<string, unknown>[]).map(productAccount) : [];
  const m = r.meta as Record<string, unknown> | null | undefined;
  const meta = {
    enabled: m?.enabled ?? false,
    acctsDir: m?.acctsDir ?? "",
    manifestPath: m?.manifestPath ?? "",
    updatedAt: m?.updatedAt ?? null,
    sharedStore: m?.sharedStore ?? null,
    count: m?.count ?? accounts.length,
    error: m?.error ?? null,
  };
  return chanReply({ meta, accounts, notice: r.notice ?? null });
}

/**
 * 包一层判据的 `invoke` 替身：账号那两问照旧按旧名字交给 `answer`（它回旧回包的形状），本层把一发 `chan_call`
 * 译过去、把回包译回成品字节；其余一切原样交给 `answer`（可以与 [`withSessionReads`] 叠着用）。
 */
export function withAccountReads(
  answer: (cmd: string, args: Record<string, unknown>) => unknown,
): (cmd: string, args?: unknown) => Promise<unknown> {
  return async (cmd, args) => {
    const read = accountReadOf(cmd, args);
    if (read) return accountReadReply(read[0], answer(read[0], read[1]));
    return answer(cmd, (args ?? {}) as Record<string, unknown>);
  };
}

// ════════════════════════════════════════════════════════════════════════════
//  〔C4d · 第四波 4B〕历史清单与注解改走通道（问本机常驻后端）之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 它们从前是五条 Tauri 命令（`list_history_projects` / `list_remote_history_projects` / `stream_history_sessions_in_project` /
// `update_history_metadata` / `list_last_accounts`〔散文墓碑〕），判据按命令名答话、回旧回包的形状。今天：
//   - 本机项目清单 = 一发 `chan_call`（`<local>`，op `history-projects`，请求体不带 `origin`）；
//   - 远端那一批 = 先问 `list_remote_mcp_origins`（哪几台）、再逐台一发 `chan_call`（`<local>`，op `history-projects`，带 `origin`）；
//   - 展开一个项目 = 一发 `chan_call`（op `history-sessions`，`{project_dir, origin?}`，一次交全、不再逐条流）；
//   - 改注解 / 上次账号表 / 删会话连带删注解 = `history-annotate` / `history-last-accounts` / `history-forget`。
// ⇒ 本节把这几发译回「哪一问 ＋ 旧形参」，把判据手里那份旧回包译成**本机后端的成品字节**（或通道的失败）。
// ⚠ 译法逐格照生产：请求体键名是 `src/frontend/ui/history-reads.ts` 发的那几个，成品键名是后端 `history_join.rs` 出的那几个
//   （跨语言金样 `tests/__fixtures__/history-products.golden.json` 钉着两侧）。旧回包里缺的格按旧消费侧的读法补齐（缺 ⇒ `null` / 缺省）。
// ⚠ 「一次 fan-out」从一发 `list_remote_history_projects` 变成「一发 `list_remote_mcp_origins` ＋ 逐台 N 发」—— 计数时一次 fan-out
//   按那一发 `list_remote_mcp_origins` 算（[`historyCalls`]）。

/** 历史那几问各自的名字（判据里的叫法 = 旧命令名；`history_forget` 是新的，旧世界里它是删会话那条命令的一部分）。 */
export type HistoryRead =
  | "list_history_projects"
  | "list_remote_history_projects"
  | "stream_history_sessions_in_project"
  | "update_history_metadata"
  | "list_last_accounts"
  | "history_forget"
  // 〔MIG-3b〕删会话改走通道（`files-delete-session`，发给那一台）；判据里仍叫旧命令名，形参是 `{origin, sessionId}`。
  | "delete_history_session";

/** 一发 `invoke` 若是历史那几问之一 ⇒ `[哪一问, 旧形参的形状]`；否则 `null`。 */
export function historyReadOf(
  cmd: string,
  args: unknown,
): [HistoryRead, Record<string, unknown>] | null {
  if (cmd === "list_remote_mcp_origins")
    return ["list_remote_history_projects", {}];
  if (cmd !== "chan_call") return null;
  const a = args as ChanCallArgs;
  switch (a.op) {
    case "history-projects": {
      const body = chanArgsJson(a) as Record<string, unknown>;
      // 带 `origin` 的那几发是「远端那一批」的逐台问 —— 一次 fan-out 已经按 `list_remote_mcp_origins` 那一发算过了。
      return body.origin === undefined ? ["list_history_projects", {}] : null;
    }
    case "history-sessions": {
      const body = chanArgsJson(a) as Record<string, unknown>;
      return [
        "stream_history_sessions_in_project",
        { origin: body.origin ?? "<local>", projectDir: body.project_dir },
      ];
    }
    case "history-annotate": {
      const body = chanArgsJson(a) as Record<string, unknown>;
      return [
        "update_history_metadata",
        { sessionId: body.sid, patch: body.patch },
      ];
    }
    case "history-last-accounts":
      return ["list_last_accounts", {}];
    case "history-forget": {
      const body = chanArgsJson(a) as Record<string, unknown>;
      return ["history_forget", { sessionId: body.sid }];
    }
    case "files-delete-session": {
      const body = chanArgsJson(a) as Record<string, unknown>;
      return ["delete_history_session", { origin: a.origin, sessionId: body.sid }];
    }
  }
  return null;
}

/** mock 过的 `invoke` 的调用记录里，历史某一问的那几发（参数是旧形参的形状）。 */
export function historyCalls(
  calls: ReadonlyArray<readonly unknown[]>,
  which: HistoryRead,
): Record<string, unknown>[] {
  return calls
    .map((c) => historyReadOf(c[0] as string, c[1]))
    .filter(
      (r): r is [HistoryRead, Record<string, unknown>] =>
        r !== null && r[0] === which,
    )
    .map((r) => r[1]);
}

const nul = (v: unknown): unknown => (v === undefined ? null : v);

/** 旧回包里的一个项目 ⇒ 成品里的一行（缺的格按旧消费侧的读法补齐）。 */
function productProject(p: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {
    projectPath: p.projectPath ?? p.path ?? "",
    projectName: p.projectName ?? p.name ?? "",
    projectDir: p.projectDir ?? "",
    sessionCount: p.sessionCount ?? 0,
    starredCount: nul(p.starredCount),
    hiddenCount: nul(p.hiddenCount),
    lastActivity: p.lastActivity ?? p.updatedAt ?? 0,
    hasLive: nul(p.hasLive),
  };
  if (typeof p.origin === "string") out.origin = p.origin;
  return out;
}

/** 旧回包里的一条会话 ⇒ 成品里的一行。 */
function productSession(e: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {
    sessionId: e.sessionId ?? "",
    projectPath: e.projectPath ?? "",
    projectName: e.projectName ?? "",
    aiTitle: nul(e.aiTitle),
    firstUserExcerpt: e.firstUserExcerpt ?? "",
    startedAt: e.startedAt ?? 0,
    updatedAt: e.updatedAt ?? 0,
    jsonlPath: e.jsonlPath ?? "",
    isLive: nul(e.isLive),
    messageCountApprox: e.messageCountApprox ?? 0,
    isBg: e.isBg ?? false,
    starred: e.starred ?? false,
    customTitle: nul(e.customTitle),
    hidden: e.hidden ?? false,
  };
  if (
    typeof e.forkedFromSessionId === "string" &&
    typeof e.forkedFromMessageUuid === "string"
  ) {
    out.forkedFromSessionId = e.forkedFromSessionId;
    out.forkedFromMessageUuid = e.forkedFromMessageUuid;
  }
  if (typeof e.origin === "string") out.origin = e.origin;
  return out;
}

/** 旧回包那一发要是抛了 ⇒ 按「对端说不行」拒（同账号那一节的译法）。 */
async function settled(res: unknown): Promise<unknown> {
  try {
    return await res;
  } catch (e) {
    throw refusedReply("failed", e instanceof Error ? e.message : String(e));
  }
}

/**
 * 包一层判据的 `invoke` 替身：历史那几问照旧按旧名字交给 `answer`（它回旧回包的形状），本层把新的那几发译过去、
 * 把回包译回成品字节；其余一切原样交给 `answer`（可以与 [`withSessionReads`] / [`withAccountReads`] 叠着用）。
 *
 * 远端那一批：`list_remote_mcp_origins` 那一发时问一次旧的 `list_remote_history_projects`（`{projects, failedHosts}` 或抛），
 * 把台名单交回去（项目里出现过的 origin ∪ `failedHosts`；整批抛了 ⇒ 一台占位名，逐台那一问一律拒）；
 * 逐台那几发按这份记下来的结果答（`failedHosts` 里的那台 ⇒ 拒）。
 */
export function withHistoryReads(
  answer: (cmd: string, args: Record<string, unknown>) => unknown,
): (cmd: string, args?: unknown) => Promise<unknown> {
  let remote:
    { projects: Record<string, unknown>[]; failedHosts: string[] } | "failed" =
    {
      projects: [],
      failedHosts: [],
    };
  return async (cmd, args) => {
    // 〔MIG-1 续〕列 tmux 会话那一发也在这里译回旧名字（本判据族大多经这一层，免逐条改）。
    const tmux = tmuxReadOf(cmd, args);
    if (tmux) return tmuxProduct(tmux[0], answer(tmux[0], tmux[1]));
    const mint = tmuxMintOf(cmd, args);
    if (mint) return tmuxMintProduct(answer(mint[0], mint[1]));
    if (cmd === "list_remote_mcp_origins") {
      try {
        const r = ((await answer("list_remote_history_projects", {})) ??
          {}) as Record<string, unknown>;
        remote = {
          projects: (Array.isArray(r.projects) ? r.projects : []) as Record<
            string,
            unknown
          >[],
          failedHosts: (Array.isArray(r.failedHosts)
            ? r.failedHosts
            : []) as string[],
        };
      } catch {
        remote = "failed";
        return ["__every_host_failed__"];
      }
      const origins = new Set<string>();
      for (const p of remote.projects)
        if (typeof p.origin === "string") origins.add(p.origin);
      for (const h of remote.failedHosts) origins.add(h);
      return [...origins];
    }
    if (cmd === "chan_call") {
      const a = args as ChanCallArgs;
      const body = [
        "history-projects",
        "history-sessions",
        "history-annotate",
        "history-last-accounts",
        "history-forget",
        "files-delete-session",
      ].includes(a.op)
        ? (chanArgsJson(a) as Record<string, unknown>)
        : null;
      if (body && a.op === "history-projects") {
        if (typeof body.origin === "string") {
          const o = body.origin;
          if (remote === "failed" || remote.failedHosts.includes(o))
            throw refusedReply("unreachable", `[${o}] 问不到`);
          return chanReply({
            rows: remote.projects
              .filter((p) => p.origin === o)
              .map(productProject),
            notice: null,
          });
        }
        const rows = ((await settled(answer("list_history_projects", {}))) ??
          []) as Record<string, unknown>[];
        return chanReply({ rows: rows.map(productProject), notice: null });
      }
      if (body && a.op === "history-sessions") {
        const got: Record<string, unknown>[] = [];
        const onEntry = {
          onmessage: (e: Record<string, unknown>) => got.push(e),
        };
        await settled(
          answer("stream_history_sessions_in_project", {
            origin: body.origin ?? "<local>",
            projectDir: body.project_dir,
            onEntry,
          }),
        );
        return chanReply({ rows: got.map(productSession), notice: null });
      }
      if (body && a.op === "history-annotate") {
        const e = ((await settled(
          answer("update_history_metadata", {
            sessionId: body.sid,
            patch: body.patch,
          }),
        )) ?? {}) as Record<string, unknown>;
        return chanReply({
          entry: {
            starred: e.starred ?? false,
            customTitle: nul(e.customTitle),
            hidden: e.hidden ?? false,
            updatedAt: e.updatedAt ?? 0,
            lastAccount: nul(e.lastAccount),
          },
        });
      }
      if (body && a.op === "history-last-accounts") {
        const m = (await settled(answer("list_last_accounts", {}))) ?? {};
        return chanReply({ accounts: m });
      }
      if (body && a.op === "history-forget") {
        await settled(answer("history_forget", { sessionId: body.sid }));
        return chanReply({ removed: true });
      }
      if (body && a.op === "files-delete-session") {
        await answer("delete_history_session", { origin: a.origin, sessionId: body.sid });
        return chanReply({ path: `<替身>/${String(body.sid)}.jsonl` });
      }
    }
    return answer(cmd, (args ?? {}) as Record<string, unknown>);
  };
}

// ════════════════════════════════════════════════════════════════════════════
//  〔C4e · 第四波 4C〕tmux 控制类（抓屏 · 杀会话 · 送键 · 就地 resume）改走通道之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 它们从前是四条 Tauri 命令（`capture_remote_pane` / `kill_remote_tmux` / `tmux_send_keys` / `backend_send_into`〔散文墓碑〕），
// 判据按命令名答话、断言旧形参。今天它们是一发 `chan_call`（op = `capture-pane` / `kill` / `launch`）⇒ 本节把一发 `chan_call`
// 译回「哪一问 ＋ 旧形参」交给判据手里那个 `invoke` 替身，再把它的旧回包译成**后端的成品字节**（或通道的失败）。
// ⚠ 译法逐格照生产：请求体键名是 `src/frontend/ui/tmux-control.ts` 发的那几个，成品键名是后端那三个构造器出的那几个
//   （跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 钉着两侧）。
// ⚠ `launch` 一个 op 有两个叫法（送键 · 就地 resume），调用方说这份判据里它该译成哪一个（同一份判据里只会出现其中一种）。

/** 旧回包里的失败（`Error` / 字符串）⇒ 那句原话。 */
function wordsOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * 一个「按旧命令名答话」的 `invoke` 替身 ⇒ 生产 `invoke` 该有的样子：tmux 控制类那几发 `chan_call` 译过去再译回来，
 * 其余一切原样交给它。判据对 `inner` 的断言（`toHaveBeenCalledWith("kill_remote_tmux", …)`）照旧成立。
 * 旧回包 ⇒ 通道结局：
 * - `kill_remote_tmux` / `tmux_send_keys`：解析成功 ⇒ 成品（`killed` / `typed` 为真）；抛 ⇒ 对端说「不行」（原话原样带上）。
 * - `backend_send_into`：`{typed:true}` ⇒ 成品；`mayFallBack:true` ⇒ 「那台没有控制通道」（能证明没发出去）；
 *   其余 `typed:false` ⇒ 对端说「键入没确认」（原因原样）；抛 ⇒ 原样抛（那一跳自己坏了 ⇒ 界面拿不准，按不回落处置）。
 * - `capture_remote_pane`：字符串 ⇒ 成品；抛 ⇒ 对端说「抓屏失败」（原话原样）。
 */
export function tmuxControlShim(
  inner: (cmd: string, args?: unknown) => unknown,
  launchAs: "tmux_send_keys" | "backend_send_into",
): (cmd: string, args?: unknown) => Promise<unknown> {
  return async (cmd, args) => {
    if (cmd !== "chan_call") return inner(cmd, args);
    const a = args as ChanCallArgs;
    if (a.op !== "kill" && a.op !== "launch" && a.op !== "capture-pane") return inner(cmd, args);
    const body = chanArgsJson(a) as Record<string, unknown>;
    const name = body.name;
    if (a.op === "kill") {
      try {
        await inner("kill_remote_tmux", { origin: a.origin, target: name });
      } catch (e) {
        throw refusedReply("kill_failed", wordsOf(e));
      }
      return chanReply({ session: name, killed: true, bus: { removed: [], failed: [], unread: null } });
    }
    if (a.op === "capture-pane") {
      let screen: unknown;
      try {
        screen = await inner("capture_remote_pane", { origin: a.origin, target: name });
      } catch (e) {
        throw refusedReply("capture_failed", wordsOf(e));
      }
      return chanReply({ name, screen });
    }
    if (launchAs === "tmux_send_keys") {
      try {
        await inner("tmux_send_keys", { origin: a.origin, target: name, keys: body.payload, enter: body.mode === "send-into" });
      } catch (e) {
        throw refusedReply("typed_unconfirmed", wordsOf(e));
      }
      return chanReply({ session: name, created: false, typed: true });
    }
    const res = (await inner("backend_send_into", { req: { origin: a.origin, name, payload: body.payload } })) as
      | { typed?: unknown; mayFallBack?: unknown; reason?: unknown; code?: unknown }
      | undefined;
    if (res?.typed === true) return chanReply({ session: name, created: false, typed: true });
    if (res?.mayFallBack === true) throw NO_CHANNEL;
    // 〔RESYNC〕夹具可以点名拒绝码（例 `wrong_owner` = 关卡 2）；不点名 ⇒ 照旧「拿不准」那一档。
    throw refusedReply(typeof res?.code === "string" ? res.code : "typed_unconfirmed", String(res?.reason ?? ""));
  };
}

/** 一串 `invoke` 调用里「杀会话」那几发 `chan_call`（op `kill`）⇒ 旧形参 `["kill_remote_tmux", {origin, target}]`（判据按旧叫法断言）。 */
export function killCallsOf(calls: ReadonlyArray<readonly unknown[]>): [string, { origin: string; target: unknown }][] {
  return calls
    .filter(([cmd, args]) => isChanCall(String(cmd), args, "kill"))
    .map(([, args]) => {
      const a = args as ChanCallArgs;
      return ["kill_remote_tmux", { origin: a.origin, target: (chanArgsJson(a) as Record<string, unknown>).name }];
    });
}

// ─── 〔C4e 批 3b〕cc-bus 驾驶舱写面那几发（`src/frontend/ui/cc-bus-control.ts`）───
// 它们从前是五条 Tauri 命令（`check_cc_bus_agent_online` / `cc_bus_send` / `cc_bus_kill` / `cc_bus_spawn` / `cc_bus_broadcast`〔散文墓碑〕），
// 驾驶舱的 DOM 判据按命令名答话、断言旧形参。今天它们是一发 `chan_call`（op = `bus-list` / `bus-send` / `bus-kill` / `bus-spawn` /
// `bus-broadcast`）⇒ 本节把一发 `chan_call` 译回「哪一问 ＋ 旧形参」交给判据手里那个 `invoke` 替身，再把它的旧回包译成**后端的成品字节**。
// ⚠ 译法逐格照生产：请求体键名是 `src/frontend/ui/cc-bus-control.ts` 发的那几个，成品键名是后端那几个构造器出的那几个
//   （跨语言金样 `tests/__fixtures__/cc-bus-control.golden.json` 钉着两侧）。
// ⚠ 查在线是**唯一译不回旧形参的一格**：`bus-list` 的请求体里没有 id（问的是整份名单，挑人在界面）⇒ 旧形参只剩 `{origin}`，
//   替身回 `{<id>: true | false | null}`（每人一格 `live`），本节把它铺成名单。

/**
 * 一个「按旧命令名答话」的 `invoke` 替身 ⇒ 生产 `invoke` 该有的样子：cc-bus 那几发 `chan_call` 译过去再译回来，其余一切原样交给它。
 * 旧回包 ⇒ 通道结局：解析成功 ⇒ 成品（发消息：在线；收掉：真收了；派生：从回显里认 `已 spawn: <名>`，认不出 ⇒ `id:null`；
 * 广播：发到 1 个）；抛 ⇒ 对端说「不行」（码 `failed`，原话原样带上）。
 */
export function ccBusControlShim(
  inner: (cmd: string, args?: unknown) => unknown,
): (cmd: string, args?: unknown) => Promise<unknown> {
  return async (cmd, args) => {
    if (cmd !== "chan_call") return inner(cmd, args);
    const a = args as ChanCallArgs;
    const old: Record<string, string> = {
      "bus-list": "check_cc_bus_agent_online",
      "bus-send": "cc_bus_send",
      "bus-kill": "cc_bus_kill",
      "bus-spawn": "cc_bus_spawn",
      "bus-broadcast": "cc_bus_broadcast",
      // 〔SH1 · V136〕驾驶舱读面那两条（原是 monitor 的 Tauri 命令，今天界面经通道直接问后端）。
      "bus-state": "read_cc_bus_state",
      "bus-inbox": "read_cc_bus_inbox",
    };
    const name = old[a.op];
    if (name === undefined) return inner(cmd, args);
    const body = chanArgsJson(a) as Record<string, unknown>;
    const oldArgs: Record<string, unknown> =
      a.op === "bus-list"
        ? { origin: a.origin }
        : a.op === "bus-send"
          ? { origin: a.origin, id: body.to, text: body.text }
          : a.op === "bus-kill"
            ? { origin: a.origin, id: body.id }
            : a.op === "bus-spawn"
              ? { origin: a.origin, dir: body.dir, task: body.task, tool: body.tool, account: body.account ?? "" }
              : a.op === "bus-state"
                ? { origin: a.origin }
                : a.op === "bus-inbox"
                  ? { origin: a.origin, id: body.id }
                  : { origin: a.origin, text: body.text };
    let res: unknown;
    try {
      res = await inner(name, oldArgs);
    } catch (e) {
      throw refusedReply("failed", wordsOf(e));
    }
    switch (a.op) {
      case "bus-list": {
        const live = (res ?? {}) as Record<string, boolean | null>;
        return chanReply({
          agents: Object.entries(live).map(([id, l]) => ({ id, target: `${id}:0.0`, unread: 0, live: l, ccm_sid: null })),
        });
      }
      case "bus-send":
        return chanReply({ to: body.to, sent: true, registered: true, live: true, from: body.from });
      case "bus-kill":
        return chanReply({ id: body.id, killed: true, stale_only: false });
      case "bus-spawn": {
        const said = String(res ?? "");
        const m = /已 spawn: (\S+)/.exec(said);
        return chanReply({ spawned: true, id: m ? m[1] : null, said });
      }
      case "bus-state": {
        // 旧形（`pane`）→ 后端成品形（`target` ＋ 身份空间两格）。
        const st = res as { agents: { id: string; pane: string; registered_at: string }[]; spawned: { id: string; dir: string; spawned_at: string; task: string }[]; skipped: number };
        return chanReply({
          agents: st.agents.map((x) => ({ id: x.id, target: x.pane, registered_at: x.registered_at, unread: 0, live: null, ccm_sid: null })),
          spawned: st.spawned.map((x) => ({ id: x.id, dir: x.dir, spawned_at: x.spawned_at, task: x.task, live: null })),
          skipped: st.skipped,
        });
      }
      case "bus-inbox":
        return chanReply({ messages: res, skipped: 0, truncated: false });
      default:
        return chanReply({ sent: 1, skipped_offline: 0, liveness_unknown: false, failed: [] });
    }
  };
}

// ════════════════════════════════════════════════════════════════════════════
//  〔MIG-2 · `99 §2.1 ⑬`〕起会话的计划与渲染四问改走通道之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 它们从前是 monitor 的 Tauri 命令（`render_launch_payload` · `render_ccm_launch` · `relay_endpoint_for_launch` ·
// `resume_history_session` / `new_local_session` / `render_local_attach`〔散文墓碑〕），判据按命令名答话、按旧形参断言。
// 今天是一发 `chan_call`（op = `launch-render-payload` · `launch-render-cli` · `launch-endpoint` · `launch-local`）⇒ 本节把那一发
// 译回旧名字交给判据手里的 `invoke` 替身，再把旧回包译成后端的成品字节：
// - 载荷：串 ⇒ `{cmd}`；抛出带 `REFUSE:` 的 ⇒ 对端拒（码 `refused`，标摘掉）；别的抛 ⇒ 那台没有控制通道（证明没发出去）。
// - `ccm …` 调用行：`{ok, cmd, reason}` 原样；抛 ⇒ 没有控制通道。
// - 中转地址：`string | null` ⇒ `{baseUrl}`；抛 ⇒ 对端拒（码 `relay_down`，原话）。
// - 本机起会话：按 `action.kind` 译回三条旧命令之一（旧形参），回 `{cmd, launchId}`（新起那一格的 token = 替身回的串）；
//   抛 ⇒ 对端拒（码 `refused`）。开窗那一跳（`open_local_terminal`）原样交给替身。
/**
 * 一发 `launch-local` 的请求体 ⇒ 它从前那三条 Tauri 命令（见本节头注）里的哪一条 ＋ 旧形参。
 * ⚠ 译法逐格照 `src/frontend/ui/launch-render.ts::planLocalLaunch` 发的键：`account` 缺席 ⇒ 旧形参里也缺席（三态不许压成两态）。
 */
function localLaunchOldArgs(b: Record<string, unknown>): [string, Record<string, unknown>] {
  const action = b.action as { kind: string; sid?: string };
  const account = "account" in b ? { account: b.account } : {};
  if (action.kind === "attach") return ["render_local_attach", { tmuxName: b.tmuxName }];
  if (action.kind === "new") return ["new_local_session", { cwd: b.cwd, launcher: b.launcher, ...account }];
  return ["resume_history_session", { sessionId: action.sid, cwd: b.cwd, launcher: b.launcher, tmuxName: b.tmuxName, ...account }];
}

/** 判据手里那个 `invoke` 替身收到的全部调用里，本机起会话那几发（译回旧名字 ＋ 旧形参）。 */
export function localLaunchCalls(calls: ReadonlyArray<readonly unknown[]>, which: string): Record<string, unknown>[] {
  return calls
    .filter(([cmd, args]) => isChanCall(String(cmd), args, "launch-local"))
    .map(([, args]) => localLaunchOldArgs(chanArgsJson(args as ChanCallArgs) as Record<string, unknown>))
    .filter(([name]) => name === which)
    .map(([, a]) => a);
}

export function launchRenderShim(
  inner: (cmd: string, args?: unknown) => unknown,
): (cmd: string, args?: unknown) => Promise<unknown> {
  const term = terminalShim(inner);
  return async (cmd, args) => {
    // 〔FIX4 · ⑬〕开终端那三步也在这里译回旧的那一条（起会话那几条判据都要开终端）。
    if (isTerminalStep(cmd, args)) return term(cmd, args);
    if (cmd !== "chan_call") return inner(cmd, args);
    // 〔MIG-1 续〕列 tmux 会话那一发也在这里译回旧名字（起会话那几条判据都要问名单）。
    const tmux = tmuxReadOf(cmd, args);
    if (tmux) return tmuxProduct(tmux[0], inner(tmux[0], tmux[1]));
    const mint = tmuxMintOf(cmd, args);
    if (mint) return tmuxMintProduct(inner(mint[0], mint[1]));
    const a = args as ChanCallArgs;
    const body = () => chanArgsJson(a) as Record<string, unknown>;
    switch (a.op) {
      case "launch-render-payload": {
        let out: unknown;
        try {
          out = await inner("render_launch_payload", { req: body() });
        } catch (e) {
          const w = wordsOf(e);
          if (w.startsWith("REFUSE:")) throw refusedReply("refused", w.slice("REFUSE:".length).trimStart());
          throw NO_CHANNEL;
        }
        return chanReply({ cmd: out });
      }
      case "launch-render-cli": {
        let out: unknown;
        try {
          out = await inner("render_ccm_launch", { req: body() });
        } catch {
          throw NO_CHANNEL;
        }
        return chanReply(out ?? { ok: false, cmd: null, reason: "桩没答" });
      }
      case "launch-endpoint": {
        const b = body();
        let url: unknown;
        try {
          url = await inner("relay_endpoint_for_launch", { origin: a.origin, account: b.account });
        } catch (e) {
          throw refusedReply("relay_down", wordsOf(e));
        }
        return chanReply({ baseUrl: url ?? null });
      }
      case "launch-local": {
        const [name, old] = localLaunchOldArgs(body());
        let out: unknown;
        try {
          out = await inner(name, old);
        } catch (e) {
          throw refusedReply("refused", wordsOf(e));
        }
        if (name === "render_local_attach") return chanReply({ cmd: out ?? "<backend-rendered-attach>", launchId: null });
        if (name === "new_local_session")
          return chanReply({ cmd: "<backend-rendered-local-line>", launchId: typeof out === "string" && out !== "" ? out : null });
        return chanReply({ cmd: "<backend-rendered-local-line>", launchId: null });
      }
      default:
        return inner(cmd, args);
    }
  };
}

// ════════════════════════════════════════════════════════════════════════════
//  〔MIG-1 续 · `99 §2.1 ⑬`〕列 tmux 会话改走通道之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 它们从前是两条 Tauri 命令（本机 `list_local_tmux` · 远端 `list_remote_tmux`〔散文墓碑〕），判据按命令名答话、回 `TmuxSession[] | null`。
// 今天是一发 `chan_call`（op = `tmux-list`，本机远端同一问）⇒ 本节把那一发译回旧名字交给判据手里的替身，再把旧回包译成后端成品：
// - 列表 ⇒ `{installed: true, sessions}`（缺的字段按旧桩的意思补齐：`path` / `command` 空串 · `attached` 否 · `windows` 1 · `sid` null）；
// - `null`：远端 = 那台没装 tmux ⇒ `{installed: false, sessions: []}`；本机 = 旧口径的「不知道」⇒ 通道那一层失败（新口径里「不知道」就是抛）；
// - `undefined`（桩没答）⇒ 那台没有控制通道（不知道）；抛 ⇒ 那台后端拒（码 `unobservable`，原话带着 —— 「不知道」要说得出为什么）。
/** 一发 `chan_call` 若是列 tmux 会话 ⇒ `[旧名字, 旧形参]`；否则 `null`。 */
export function tmuxReadOf(cmd: string, args: unknown): [string, Record<string, unknown>] | null {
  if (!isChanCall(cmd, args, "tmux-list")) return null;
  return args.origin === "<local>" ? ["list_local_tmux", {}] : ["list_remote_tmux", { origin: args.origin }];
}

/** 旧回包 ⇒ `tmux-list` 成品（见本节头注）。 */
async function tmuxProduct(name: string, got: Promise<unknown> | unknown): Promise<ArrayBuffer> {
  let v: unknown;
  try {
    v = await got;
  } catch (e) {
    throw refusedReply("unobservable", wordsOf(e));
  }
  if (v === undefined || (v === null && name === "list_local_tmux")) throw NO_CHANNEL;
  if (v === null) return chanReply({ installed: false, sessions: [] });
  const rows = (v as Record<string, unknown>[]).map((r) => ({
    name: r.name,
    path: r.path ?? "",
    command: r.command ?? "",
    attached: r.attached ?? false,
    windows: r.windows ?? 1,
    sid: r.sid ?? null,
  }));
  return chanReply({ installed: true, sessions: rows });
}

/** 判据手里那个替身外面包一层：列 tmux 会话那一发译回旧名字（见本节头注）；别的原样交进去。 */
export function withTmuxReads(
  inner: (cmd: string, args?: unknown) => unknown,
): (cmd: string, args?: unknown) => Promise<unknown> {
  return async (cmd, args) => {
    const t = tmuxReadOf(cmd, args);
    if (t) return tmuxProduct(t[0], inner(t[0], t[1]));
    const m = tmuxMintOf(cmd, args);
    if (m) return tmuxMintProduct(inner(m[0], m[1]));
    return inner(cmd, args);
  };
}

// ════════════════════════════════════════════════════════════════════════════
//  〔FIX4 · `设计/90 §3` J7〕起会话要的 tmux 名改问那台后端（`tmux-name-mint`）之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 同上一节的译法：一发 `chan_call`（op = `tmux-name-mint`）译成判据手里那个替身认得的名字 `tmux_name_mint`
// （形参 `{origin, cwd}` / `{origin, forkOf}`），替身答一个**名字**。派生 ＋ 避让的规则只在后端（`control/ccm/plan.rs`）⇒
// 这里**不重抄**：判据自己写死「那台铸了什么」，钉的是前端问了谁、问的什么、用的是不是它铸回来的、问不到时怎么办。
// - 名字（字符串）⇒ 成品 `{name}`；`{ bad: v }` ⇒ 原样回 `v`（形状不认那一格）；
// - `undefined`（替身没答）⇒ 那台没有控制通道（问不到）；抛 ⇒ 那台后端拒（码 `invalid_args`，原话带着）。
/** 一发 `chan_call` 若是铸名那一问 ⇒ `["tmux_name_mint", {origin, …入参}]`；否则 `null`。 */
export function tmuxMintOf(cmd: string, args: unknown): [string, Record<string, unknown>] | null {
  if (!isChanCall(cmd, args, "tmux-name-mint")) return null;
  return ["tmux_name_mint", { origin: args.origin, ...(chanArgsJson(args) as Record<string, unknown>) }];
}

/** 替身答的 ⇒ 后端成品（见本节头注）。 */
async function tmuxMintProduct(got: Promise<unknown> | unknown): Promise<ArrayBuffer> {
  let v: unknown;
  try {
    v = await got;
  } catch (e) {
    throw refusedReply("invalid_args", wordsOf(e));
  }
  if (v === undefined) throw NO_CHANNEL;
  if (typeof v === "string") return chanReply({ name: v });
  return chanReply((v as { bad: unknown }).bad);
}

/** 被问过的铸名那几发（`[origin, 入参]`，入参去掉 `origin`）。 */
export function tmuxMintCalls(calls: ReadonlyArray<readonly unknown[]>): [string, Record<string, unknown>][] {
  return calls
    .map(([c, a]) => tmuxMintOf(String(c), a))
    .filter((x): x is [string, Record<string, unknown>] => x !== null)
    .map(([, a]) => {
      const { origin, ...rest } = a;
      return [String(origin), rest];
    });
}

// ════════════════════════════════════════════════════════════════════════════
//  〔FIX4 · `设计/99 §2.1 ⑬`〕开终端改成三步之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 今天开终端是三步（`src/frontend/ui/terminal-open.ts`）：monitor `terminal_dial {origin}`（机器事实）→ 本机后端 `terminal-ssh`
// （渲 `ssh -t …` 那一行）→ monitor `open_terminal_window {command, rbindToken, ssh}`。判据手里的替身按**旧的那一条**答话、断言
// （`launch_remote_terminal {origin, remoteCmd, rbindToken}`）⇒ 本节把三步译回那一条：
// - `terminal-ssh` **原样回**交进来的那串（ssh 外壳的字节归 Rust：`tests/backend/dial_terminal_tests.rs`，这里不重抄渲染）；
// - 开窗那一步凭「上一次 `terminal_dial` 问的是哪台」补回 origin；`ssh: false` ⇒ 本机串 `<local>`。
/** 这一发是不是开终端那三步之一。 */
export function isTerminalStep(cmd: string, args: unknown): boolean {
  return cmd === "terminal_dial" || cmd === "open_terminal_window" || isChanCall(cmd, args, "terminal-ssh");
}

/** 把开终端那三步译回旧的 `launch_remote_terminal`（见本节头注）；别的原样交进去。 */
export function terminalShim(
  inner: (cmd: string, args?: unknown) => unknown,
): (cmd: string, args?: unknown) => Promise<unknown> {
  let asked: string | null = null;
  return async (cmd, args) => {
    if (cmd === "terminal_dial") {
      asked = (args as { origin: string }).origin;
      return { machine: { label: asked } };
    }
    if (isChanCall(cmd, args, "terminal-ssh")) {
      return chanReply({ command: (chanArgsJson(args) as { command: string }).command });
    }
    if (cmd === "open_terminal_window") {
      const a = args as { command: string; rbindToken: string | null; ssh: boolean };
      const origin = a.ssh ? (asked ?? "<没问过 terminal_dial>") : "<local>";
      asked = null;
      return inner("launch_remote_terminal", { origin, remoteCmd: a.command, rbindToken: a.rbindToken });
    }
    return inner(cmd, args);
  };
}
