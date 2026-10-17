/**
 * 截图台架的盘上世界：每台机器一个家目录里**原始格式**的那几份文件（claude 与 cc-monitor 自己落盘的样子），交给真后端去读。
 * 这里只写输入，不写成品：额度行 · 时刻字 · 状态字 …… 一律是真后端读了这些文件算出来的（`real/pool.mjs` 起它）。
 *
 * 家目录在后端那一侧挂成 `/home/user`（`real/pool.mjs` 的 bwrap），所以文件里的绝对路径都写 `/home/user/…`。
 * 号名 · 邮箱 · key 都是编的（key 只是占位串，末四位进掩码）。
 */

export const HOME = "/home/user";
const ACCTS = ".cc-monitor/accounts";

/**
 * 一个活会话（`real/pool.mjs` 给它配一个替身进程、写 `.claude/sessions/<pid>.json`）。替身缺省住在那台私有 tmux 的一个窗格里。
 * `fate`：开页之前就收场的那两种 —— `ended`（替身在世界起好之后被杀、不在 tmux 里了）· `reconnectable`（claude 退了、窗格还在、标签还挂着，没有 pidfile）。
 */
export interface LiveSession {
  sid: string;
  cwd: string;
  kind?: "interactive" | "bg";
  /** 会话名（pidfile 的 `name`）。 */
  name?: string;
  /** 不住 tmux（替身是个光进程 ⇒ 容器那一格「不在终端宿主里」）。缺 ⇒ 住。 */
  tmux?: false;
  fate?: "ended" | "reconnectable";
  startedAt?: number;
  /** claude 写在 pidfile 里的那几格（原词）：`busy` · `waiting` · `idle` · `shell`；在等什么框；状态几点写的（ms）。 */
  status?: "busy" | "waiting" | "idle" | "shell";
  waitingFor?: string;
  statusUpdatedAt?: number;
  /** 它那个窗格里此刻那一屏（原样的终端输出，`./screen`）：替身起来先印进自己的窗格，真后端抓屏 · 实时画面读的就是它。缺 ⇒ 空屏。 */
  screen?: string;
  /** 连着它那个 tmux 会话的终端窗口数（台架在同一台私有 tmux 里起几个嵌套的客户端 attach 上去）。缺 ⇒ 0（在后台）。 */
  windows?: number;
}

export interface MachineDisk {
  /** 相对家目录的路径 → 内容。 */
  files: Record<string, string>;
  /** 修改时刻不是开页那一刻的那几份：相对家目录的路径 → epoch ms。 */
  mtimes?: Record<string, number>;
  live: LiveSession[];
  /** quota-warm 在跑（`real/pool.mjs` 配一个替身进程、写 `.cc-monitor/quota-warm.json` 带它的 pid）：号 → 下一次开窗的时刻。 */
  warm?: { account: string; at: number }[];
  /**
   * 「开着即活」那一家（没有 pidfile，在世期间写记录的进程一直开着那份记录）的活会话：每份配一个替身进程开着它写（相对家目录）。
   * 后端照真的那一套认写者、宣告。
   */
  writers?: string[];
  /**
   * 预读演成一直没读完的那几个会话（sid）：后端标准输出往壳那一跳，这几个会话宣告之后的格（读完了 · 状态 · 行 · 子运行 · 主线外 · 一轮结束）不交 ——
   * 与后端预读没完成时压着的那几种恰好一样，壳与界面照真的那一套收宣告那一帧。
   */
  primeHeld?: string[];
  /** 不住 claude 的 tmux 会话（起了、启动器报错退回 shell 的那一种）：会话名 ＋ 窗格里那一屏。 */
  bareTmux?: { name: string; screen: string }[];
  /**
   * 那台的 tmux 在边界上的替身（后端照常敲 `tmux`，先碰到它）：报的版本（演「只能快照」）· 哪几个子命令照「没有 server」失败
   * （演「画面读不出」）。缺 ⇒ 真 tmux 原样。
   */
  tmuxShim?: { version?: string; noServer?: string[] };
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

/**
 * 用户级 MCP 配置（`~/.claude.json` 的 `mcpServers`，并进登录那一份身份里）＋ 哪几个号连它时碰到要登录（各号配置目录里
 * `mcp-needs-auth-cache.json`，Claude Code 记的那一笔：服务器名 → `{timestamp}`；后端只认 15 分钟内的）。
 */
export function putMcp(d: MachineDisk, servers: Record<string, unknown>, needsAuth: { account: string; atMs: number; server: string }[] = []): void {
  const cfg = JSON.parse(d.files[".claude.json"] ?? "{}") as Record<string, unknown>;
  d.files[".claude.json"] = json({ ...cfg, mcpServers: servers });
  const caches = new Map<string, Record<string, { timestamp: number }>>();
  for (const n of needsAuth) caches.set(n.account, { ...(caches.get(n.account) ?? {}), [n.server]: { timestamp: n.atMs } });
  for (const [account, cache] of caches) d.files[`${ACCTS}/${account}/mcp-needs-auth-cache.json`] = json(cache);
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

// ── 轮换账本（`<家>/.cc-monitor/rotation.json` 第三版：每个会话各一条，带它载入的那份策略副本 ＋ `rotation-uses.json` 的 forNew）──

export interface RotationSessionSpec {
  agent?: string;
  start: string;
  current: string;
  since: number;
  seen?: number;
  /** 载入哪份策略（策略目录里的文件 id，副本照 `putRotation` 交来的原文现拼）；不给 ⇒ 不轮换。 */
  strategy?: string;
  /** 拖出来的「接下来先用谁」。 */
  temp?: string[];
  /** 这个会话先不用的号。 */
  off?: string[];
  /** 换号记录。 */
  log?: { at: number; from: string; to: string; cause: { k: string; until?: number }; why?: string }[];
  /** 副本是旧版：载入那一刻的设置（整份）· 版本（跟文件此刻的不同 ⇒「策略改过了 · 载入新版」）。 */
  stale?: { settings: unknown; rev: string };
}

/** SHA-256（十六进制）：副本的版本照后端的写法（`h:` ＋ 原文摘要前 12 位）。 */
function sha256(text: string): string {
  const k = [] as number[];
  const h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
  for (let n = 2, c = 0; c < 64; n++) {
    if ([...Array(n).keys()].slice(2).every((d) => n % d !== 0)) k[c++] = (Math.cbrt(n) % 1) * 2 ** 32 | 0;
  }
  const bytes = [...new TextEncoder().encode(text)];
  const len = bytes.length * 8;
  bytes.push(0x80);
  while (bytes.length % 64 !== 56) bytes.push(0);
  for (let i = 7; i >= 0; i--) bytes.push(Math.floor(len / 2 ** (i * 8)) & 0xff);
  const r = (x: number, n: number): number => (x >>> n) | (x << (32 - n));
  for (let o = 0; o < bytes.length; o += 64) {
    const w: number[] = [];
    for (let i = 0; i < 64; i++) {
      if (i < 16) w[i] = (bytes[o + 4 * i] << 24) | (bytes[o + 4 * i + 1] << 16) | (bytes[o + 4 * i + 2] << 8) | bytes[o + 4 * i + 3];
      else {
        const s0 = r(w[i - 15], 7) ^ r(w[i - 15], 18) ^ (w[i - 15] >>> 3);
        const s1 = r(w[i - 2], 17) ^ r(w[i - 2], 19) ^ (w[i - 2] >>> 10);
        w[i] = (w[i - 16] + s0 + w[i - 7] + s1) | 0;
      }
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let i = 0; i < 64; i++) {
      const t1 = (hh + (r(e, 6) ^ r(e, 11) ^ r(e, 25)) + ((e & f) ^ (~e & g)) + k[i] + w[i]) | 0;
      const t2 = ((r(a, 2) ^ r(a, 13) ^ r(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) | 0;
      [hh, g, f, e, d, c, b, a] = [g, f, e, (d + t1) | 0, c, b, a, (t1 + t2) | 0];
    }
    [a, b, c, d, e, f, g, hh].forEach((x, i) => (h[i] = (h[i] + x) | 0));
  }
  return h.map((x) => (x >>> 0).toString(16).padStart(8, "0")).join("");
}

/** 一份策略文件原文的副本（会话载入的样子：头里的设置 · 时区 · 声明，代码那一段前面垫好空行）。 */
function copyOf(id: string, source: string, stale?: { settings: unknown; rev: string }): unknown {
  const open = source.indexOf("/*--- rotation\n");
  const close = source.indexOf("\n---*/", open);
  const head = JSON.parse(source.slice(open + "/*--- rotation\n".length, close)) as Record<string, unknown>;
  const lead = close + "\n---*/".length;
  const code = "\n".repeat(source.slice(0, lead).split("\n").length - 1) + source.slice(lead);
  return {
    id,
    rev: stale?.rev ?? `h:${sha256(source).slice(0, 12)}`,
    code,
    settings: stale?.settings ?? head.settings,
    ...(head.tz ? { tz: head.tz } : {}),
    ...(head.timeVarying ? { timeVarying: true } : {}),
    ...(Array.isArray(head.form) && head.form.length > 0 ? { form: head.form } : {}),
  };
}

/** `strategies`：策略目录里那几份的原文（会话的副本照它拼）。 */
export function putRotation(
  d: MachineDisk,
  book: { forNew?: string; sessions?: Record<string, RotationSessionSpec>; strategies?: Record<string, string> },
): void {
  const sessions = Object.entries(book.sessions ?? {});
  d.files[".cc-monitor/rotation.json"] = json({
    version: 3,
    sessions: Object.fromEntries(
      sessions.map(([sid, s]) => {
        const src = s.strategy !== undefined ? book.strategies?.[s.strategy] : undefined;
        return [
          sid,
          {
            agent: s.agent ?? "claude-code",
            start: s.start,
            current: s.current,
            since: Math.round(s.since),
            seen: Math.round(s.seen ?? s.since),
            ...(s.strategy !== undefined && src !== undefined ? { uses: copyOf(s.strategy, src, s.stale) } : {}),
            ...(s.temp ? { temp: s.temp } : {}),
            ...(s.off ? { off: s.off } : {}),
            ...(s.log ? { log: s.log.map((l) => ({ ...l, at: Math.round(l.at) })) } : {}),
          },
        ];
      }),
    ),
  });
  d.files[".cc-monitor/rotation-uses.json"] = json({ version: 1, forNew: book.forNew ?? "" });
}

// ── 策略文件（`<家>/.cc-monitor/strategies/<文件名>.rhai`：标题 ＝ 文件名）──

export function putStrategy(d: MachineDisk, name: string, source: string): void {
  d.files[`.cc-monitor/strategies/${name}.rhai`] = source;
}

/** 升级时挪进 `strategies/v3/` 的第三版脚本（不加载，界面提示「N 份旧格式的策略没有加载」）。 */
export function putOldScript(d: MachineDisk, file: string, source: string): void {
  d.files[`.cc-monitor/strategies/v3/${file}`] = source;
}

// ── 会话血缘（`<家>/.cc-monitor/lineage.json`）──

export function putLineage(d: MachineDisk, parents: Record<string, string>, at: number): void {
  d.files[".cc-monitor/lineage.json"] = json({ origins: {}, parents: Object.fromEntries(Object.entries(parents).map(([sid, parent]) => [sid, { parent, agent: "claude-code", at: Math.round(at) }])) });
}

// ── 那台的偏好（`<家>/.cc-monitor/prefs.json`，第 1 版；只写给了的那几格，别的照后端缺省）──

export function putPrefs(d: MachineDisk, prefs: Record<string, unknown>): void {
  d.files[".cc-monitor/prefs.json"] = json({ version: 1, ...prefs });
}

// ── quota-warm 在跑（`<家>/.cc-monitor/quota-warm.json`：写它的那个进程的 pid ＋ 每号下一次开窗的时刻）──

export function putWarm(d: MachineDisk, next: { account: string; at: number }[]): void {
  d.warm = next.map((n) => ({ account: n.account, at: Math.round(n.at) }));
}
