/**
 * 截图台架的盘上世界：每台机器一个家目录里**原始格式**的那几份文件（claude 与 cc-monitor 自己落盘的样子），交给真后端去读。
 * 这里只写输入，不写成品：额度行 · 时刻字 · 状态字 …… 一律是真后端读了这些文件算出来的（`real/pool.mjs` 起它）。
 *
 * 家目录在后端那一侧挂成 `/home/user`（`real/pool.mjs` 的 bwrap），所以文件里的绝对路径都写 `/home/user/…`。
 * 号名 · 邮箱 · key 都是编的（key 只是占位串，末四位进掩码）。
 */

export const HOME = "/home/user";
const ACCTS = ".cc-monitor/accounts";

/** 一个活会话（`real/pool.mjs` 给它配一个替身进程、写 `.claude/sessions/<pid>.json`）。 */
export interface LiveSession {
  sid: string;
  cwd: string;
  kind?: "interactive" | "bg";
  startedAt?: number;
  /** claude 写在 pidfile 里的那几格（原词）：`busy` · `waiting` · `idle` · `shell`；在等什么框；状态几点写的（ms）。 */
  status?: "busy" | "waiting" | "idle" | "shell";
  waitingFor?: string;
  statusUpdatedAt?: number;
}

export interface MachineDisk {
  /** 相对家目录的路径 → 内容。 */
  files: Record<string, string>;
  live: LiveSession[];
  /** quota-warm 在跑（`real/pool.mjs` 配一个替身进程、写 `.cc-monitor/quota-warm.json` 带它的 pid）：号 → 下一次开窗的时刻。 */
  warm?: { account: string; at: number }[];
}

export const blankDisk = (): MachineDisk => ({ files: {}, live: [] });

const json = (v: unknown): string => JSON.stringify(v, null, 2) + "\n";

// ── 账号库（`<家>/.cc-monitor/accounts/accounts.json` ＋ 每号一个配置目录；按量号的 key 在 `apikey-credentials.json`）──

export interface AccountSpec {
  name: string;
  kind: "sub" | "api";
  email?: string;
  isDefault?: boolean;
  /** 订阅号登没登录（配置目录里有没有 `.credentials.json`）；按量号有没有 key。缺 ⇒ 有。 */
  signedIn?: boolean;
  /** 按量号的端点。 */
  baseUrl?: string;
}

/**
 * 照一台正常机器的样子写账号清单：具名的号 ＋ 末尾那一条账号 0（不设 `CLAUDE_CONFIG_DIR` 那个状态，没有 `configDir`；后端写清单时也是这么合成的）。
 * `zero: false` ＝ 清单缺了账号 0（只给专门截「缺默认账号」那一条提示的场景用）。
 */
export function putAccounts(d: MachineDisk, accounts: AccountSpec[], opts: { zero?: boolean } = {}): void {
  d.files[`${ACCTS}/accounts.json`] = json({
    version: 1,
    updatedAt: "2026-10-01T08:00:00Z",
    accounts: [
      ...accounts.map((a) => ({
        name: a.name,
        email: a.email ?? (a.kind === "api" ? "" : `${a.name}@example.com`),
        configDir: `${HOME}/${ACCTS}/${a.name}`,
        isDefault: a.isDefault ?? false,
        mode: "isolated",
        ...(a.kind === "api" ? { authKind: "api-key" } : {}),
      })),
      ...(opts.zero === false ? [] : [{ name: "0", email: "0@example.com", isDefault: false, mode: "bare" }]),
    ],
  });
  const keys: Record<string, { api_key: string; base_url: string }> = {};
  // 这台本来那一份登录（`~/.claude`，账号 0）也登着。
  signIn(d, ".claude", ".claude.json", "0");
  for (const a of accounts) {
    d.files[`${ACCTS}/${a.name}/.keep`] = "";
    if (a.signedIn === false) continue;
    if (a.kind === "sub") signIn(d, `${ACCTS}/${a.name}`, `${ACCTS}/${a.name}/.claude.json`, a.name);
    else keys[a.name] = { api_key: `sk-placeholder-${a.name}-a1b2`, base_url: a.baseUrl ?? "https://api.example.com" };
  }
  if (Object.keys(keys).length > 0) d.files[".cc-monitor/apikey-credentials.json"] = json({ accounts: keys });
}

/** 订阅号登着：配置目录里一份 `.credentials.json`（令牌是占位串）＋ 那份配置里的账号身份（`oauthAccount.accountUuid`，按名字编的）。 */
function signIn(d: MachineDisk, dir: string, config: string, name: string): void {
  d.files[`${dir}/.credentials.json`] = json({ claudeAiOauth: { accessToken: "placeholder", refreshToken: "placeholder", expiresAt: 4_102_444_800_000 } });
  const hex = Array.from(name, (c) => c.charCodeAt(0).toString(16).padStart(2, "0")).join("").slice(0, 12).padEnd(12, "0");
  d.files[config] = json({ oauthAccount: { accountUuid: `00000000-0000-4000-8000-${hex}`, emailAddress: `${name}@example.com` } });
}

// ── 额度账（`<家>/.cc-monitor/quota.json`：中转从回包头记下的那几格，窗口名照 claude 回包头的原名）──

export interface QuotaWindowSpec {
  name: "five_hour" | "seven_day" | string;
  /** 已用比例（0–1）。 */
  used?: number;
  resetsAt?: number;
}

export interface QuotaSpec {
  account: string;
  agent?: string;
  seenAt: number;
  status?: "allowed" | "warning" | "rejected";
  refused?: boolean;
  /** 卡着的那个窗口（原名）。 */
  limiting?: string;
  resetsAt?: number;
  windows: QuotaWindowSpec[];
  overage?: { status?: "allowed" | "warning" | "rejected"; resetsAt?: number; disabled?: string; inUse: boolean };
}

export function putQuota(d: MachineDisk, entries: QuotaSpec[]): void {
  d.files[".cc-monitor/quota.json"] = json({
    accounts: entries.map((e) => ({
      agent: e.agent ?? "claude-code",
      account: e.account,
      seenAt: Math.round(e.seenAt),
      reading: {
        ...(e.status ? { status: e.status } : {}),
        refused: e.refused ?? false,
        ...(e.limiting ? { limiting: e.limiting } : {}),
        ...(e.resetsAt !== undefined ? { resetsAt: Math.round(e.resetsAt) } : {}),
        windows: e.windows.map((w) => ({ name: w.name, ...(w.used !== undefined ? { used: w.used } : {}), ...(w.resetsAt !== undefined ? { resetsAt: Math.round(w.resetsAt) } : {}) })),
        ...(e.overage ? { overage: { ...e.overage, ...(e.overage.resetsAt !== undefined ? { resetsAt: Math.round(e.overage.resetsAt) } : {}) } } : {}),
      },
    })),
  });
}

/** 额度账写坏了（读不懂的那一形）。 */
export function putBrokenQuota(d: MachineDisk): void {
  d.files[".cc-monitor/quota.json"] = "";
}

// ── 轮换账本（`<家>/.cc-monitor/rotation.json`：规则 ＋ 每个会话记下的那一份）──

export interface RuleSpec {
  name: string;
  rotation: Record<string, unknown>;
  rev?: number;
  updatedAt: number;
}

export interface RotationSessionSpec {
  agent?: string;
  start: string;
  current: string;
  since: number;
  /** `"follow"` · `{rule: id}` · `"custom"` · `{parent: sid}`。 */
  source: unknown;
  custom?: Record<string, unknown>;
  history?: Record<string, unknown>[];
  blockedAbove?: string[];
  seen?: number;
}

export function putRotation(d: MachineDisk, book: { defaultRule: string; rules: Record<string, RuleSpec>; sessions?: Record<string, RotationSessionSpec> }): void {
  d.files[".cc-monitor/rotation.json"] = json({
    defaultRule: book.defaultRule,
    rules: Object.fromEntries(Object.entries(book.rules).map(([id, r]) => [id, { name: r.name, rotation: r.rotation, rev: r.rev ?? 1, updatedAt: Math.round(r.updatedAt) }])),
    sessions: Object.fromEntries(
      Object.entries(book.sessions ?? {}).map(([sid, s]) => [
        sid,
        {
          agent: s.agent ?? "claude-code",
          start: s.start,
          current: s.current,
          since: Math.round(s.since),
          source: s.source,
          ...(s.custom ? { custom: s.custom } : {}),
          history: s.history ?? [],
          ...(s.blockedAbove?.length ? { blockedAbove: s.blockedAbove } : {}),
          seen: Math.round(s.seen ?? s.since),
        },
      ]),
    ),
  });
}

export function putBrokenRotation(d: MachineDisk): void {
  d.files[".cc-monitor/rotation.json"] = "{";
}

// ── 会话血缘（`<家>/.cc-monitor/lineage.json`）──

export function putLineage(d: MachineDisk, parents: Record<string, string>, at: number): void {
  d.files[".cc-monitor/lineage.json"] = json({ origins: {}, parents: Object.fromEntries(Object.entries(parents).map(([sid, parent]) => [sid, { parent, agent: "claude-code", at: Math.round(at) }])) });
}

// ── quota-warm 在跑（`<家>/.cc-monitor/quota-warm.json`：写它的那个进程的 pid ＋ 每号下一次开窗的时刻）──

export function putWarm(d: MachineDisk, next: { account: string; at: number }[]): void {
  d.warm = next.map((n) => ({ account: n.account, at: Math.round(n.at) }));
}
