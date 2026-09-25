/**
 * 〔C4a〕判据里假扮 monitor 那一跳（包装层 `chan_call`）的几样小工具。
 *
 * 生产上 `chan.call` ⇒ `commands.chan_call` ⇒ `invoke("chan_call", { origin, op, payload, leftMs })`，
 * monitor 回原样字节（`ArrayBuffer`）或 `{ err, body }`（`wire::err_to_wire` 的线上形状）。
 * 各判据 mock 的是 `@tauri-apps/api/core` 的 `invoke` —— 本文件只帮它们造「那一跳会回什么」。
 */

/** `chan_call` 的实参（判据按 `op` 分派）。 */
export interface ChanCallArgs {
  origin: string;
  op: string;
  payload: number[];
  leftMs: number;
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
// ⚠ 译法逐格照生产：请求体的键名是 `src/session-reads.ts` 发的那几个，成品的键名是后端 `read_face.rs` 出的那几个
//   （跨语言金样 `tests/__fixtures__/session-reads.golden.json` 钉着两侧）。

/** 三问各自的名字（判据里的叫法 = 旧命令名，只为让断言读起来是「哪一问」）。 */
export type SessionRead = "read_session_index" | "list_user_inputs" | "find_in_session";

const READ_OPS: Record<string, SessionRead> = {
  "history-index": "read_session_index",
  "history-user-inputs": "list_user_inputs",
  "history-find": "find_in_session",
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

/**
 * 判据手里那份回包（旧回包的形状：`{available, …}`）⇒ 通道那一跳的结局：
 * 可用 ⇒ 后端的成品字节；不可用 ⇒ 按它的种类拒（`oldBackend` ⇒ 对端不认 · `truncated` ⇒ 对端说装不下（`too_large`）·
 * 其余 ⇒ 对端说不行（`failed`）；原因原样带上）。
 * 回包是 `undefined` ⇒ 原样 `undefined`（让「形状不对」那一格照样可测）。
 */
export async function sessionReadReply(which: SessionRead, res: unknown): Promise<ArrayBuffer | undefined> {
  const r = (await res) as Record<string, unknown> | undefined;
  if (r === undefined) return undefined;
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
    return answer(cmd, (args ?? {}) as Record<string, unknown>);
  };
}

// ════════════════════════════════════════════════════════════════════════════
//  〔C4c · 第四波 4B〕账号那两问（清单 · 信任预检）改走通道之后，判据那一侧的翻译
// ════════════════════════════════════════════════════════════════════════════
//
// 它们从前是三条 Tauri 命令（`list_remote_accounts` / `list_local_accounts` / `check_account_trust`〔散文墓碑〕），
// 判据按命令名答话、回旧回包的形状（`{available, error, meta, accounts, notice}` / `{available, trusted, known, error}`）。
// 今天它们是一发 `chan_call`（op = `accounts-list` / `accounts-trust`）⇒ 本节把一发 `chan_call` 译回「哪一问 ＋ 旧形参」，
// 把判据手里那份旧回包译成**后端的成品字节**（或通道的失败）。
// ⚠ 译法逐格照生产：请求体键名是 `src/accounts.ts` 发的那几个，成品键名是后端 `observe/accounts_query.rs::list_product`
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
