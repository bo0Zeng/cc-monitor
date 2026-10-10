/**
 * 设置窗性能台架的合成世界：照真实规模放大 —— 十几台机器、二十几个账号、三十条轮换规则、几百个扩展 × 十几台。
 * 名字与摘要全是按模板拼的占位（与 `fake/world.ts` 同一套口径），不取任何真配置。
 *
 * 量法与读数住 `settings-bench.mjs` 头注；这里只造数据。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import { fakePlan } from "../fake/timeline";
import type { OpHandler, World } from "../fake/types";
import { defaultConfig, defaultWorld, LOCAL, session, sidOf } from "../fake/world";
import { Convo } from "../fake/records";

/** 规模表（改它就改整个世界的量级）。 */
export const SETTINGS_SCALE = { remotes: 11, accounts: 24, rules: 30, ext: 300, sessions: 48 };

const remoteNames = (): string[] => Array.from({ length: SETTINGS_SCALE.remotes }, (_, i) => `node-${String(i + 1).padStart(2, "0")}`);
const acctNames = (): string[] => Array.from({ length: SETTINGS_SCALE.accounts }, (_, i) => `acct-${String(i + 1).padStart(2, "0")}`);
const now = (): number => Math.floor(Date.now() / 1000);

function smallConvo(n: number, cwd: string): Convo {
  const c = new Convo(sidOf(n), cwd, "2026-10-01T08:00:00Z");
  c.title(`占位会话 ${n}`);
  c.user(`第 ${n} 个会话：把 mod-${n % 7} 的日志改成结构化输出。`);
  c.say("改好了。", 20_000, "end_turn");
  return c;
}

export function settingsPerfWorld(): World {
  const w = defaultWorld();
  const remotes = remoteNames();
  const accts = acctNames();
  w.machines = [LOCAL, ...remotes];
  const base = defaultConfig() as { remote: { hosts: Record<string, unknown>[] } };
  const tmpl = base.remote.hosts[0];
  w.config = { ...w.config, remote: { hosts: remotes.map((label, i) => ({ ...tmpl, label, host: `10.0.1.${11 + i}`, hostKeyFingerprint: `SHA256:Bb${String(i).padStart(2, "0")}${"R".repeat(38)}` })) } };
  w.sessions = Array.from({ length: SETTINGS_SCALE.sessions }, (_, k) => {
    const n = 0x200 + k;
    const origin = w.machines[k % w.machines.length];
    const cwd = `/home/user/work/p${k}`;
    return session(n, origin, cwd, smallConvo(n, cwd), { activity: k % 4 === 0 ? "working" : "idle" });
  });

  // ── 账号 · 额度 ──
  w.ops["accounts-list"] = () => ({
    accounts: accts.map((name, i) => ({
      name,
      email: i % 5 === 4 ? "" : `${name}@example.com`,
      configDir: `/home/user/.cc-monitor/accounts/${name}`,
      isDefault: i === 0,
      mode: "isolated",
      exists: true,
      loggedIn: i % 5 !== 4,
      authKind: i % 5 === 4 ? "api-key" : "subscription",
      authReady: true,
      selectable: true,
      keyMasked: i % 5 === 4 ? "••••••••a1b2" : null,
      baseUrl: i % 5 === 4 ? "https://api.example.com" : null,
    })),
    meta: {
      enabled: true,
      acctsDir: "/home/user/.cc-monitor/accounts",
      manifestPath: "/home/user/.cc-monitor/accounts/accounts.json",
      updatedAt: "2026-10-01T08:00:00Z",
      sharedStore: "/home/user/.cc-monitor/accounts/shared",
      count: accts.length,
      error: null,
      unsupported: null,
      nextDefault: accts[1],
      effectiveDefault: accts[0],
      home: "/home/user",
    },
    notice: null,
  });
  w.ops["accounts-sessions"] = (origin, _req, world) => ({
    lines: world.sessions
      .filter((s) => s.origin === origin)
      .map((s, i) => JSON.stringify({ pid: 4100 + i, sessionId: s.sid, cwd: s.cwd, configDir: `/home/user/.cc-monitor/accounts/${accts[i % accts.length]}`, account: accts[i % accts.length], bare: false, alive: true, viaRelay: null })),
  });
  w.ops["quota-read"] = () => {
    const t = now();
    return {
      state: "present",
      reason: null,
      path: "/home/user/.cc-monitor/quota.json",
      now: t,
      accounts: accts.map((account, i) => {
        const api = i % 5 === 4;
        const p5 = (i * 37) % 101;
        return {
          agent: "claude-code",
          account,
          seenAt: t - 60 * (i + 1),
          kind: api ? "api" : "sub",
          state: p5 >= 100 ? "refused" : "ok",
          stale: false,
          ...(api ? {} : { limiting: "5h" }),
          slots: api
            ? []
            : [
                { slot: "5h", pct: p5, resetsAt: t + 600 * (i + 1) },
                { slot: "7d", pct: (i * 13) % 100, resetsAt: t + 3600 * (i + 20) },
              ],
          login: "ok",
        };
      }),
      unseen: [],
      usableNow: accts.filter((_, i) => (i * 37) % 101 < 100),
      earliestReturn: null,
    };
  };

  // ── 轮换规则 ──
  const sids = w.sessions.map((s) => s.sid);
  w.ops["rotation-rules-read"] = () => ({
    state: "present",
    reason: null,
    path: "/home/user/.cc-monitor/rotation.json",
    defaultRule: "r_000",
    rules: Array.from({ length: SETTINGS_SCALE.rules }, (_, k) => {
      const live = sids.filter((_, j) => j % SETTINGS_SCALE.rules === k);
      const order = [{ start: true }, ...Array.from({ length: 4 }, (_, j) => accts[(k + j) % accts.length])];
      return {
        id: `r_${String(k).padStart(3, "0")}`,
        name: `规则 ${k + 1}`,
        rotation: { order, enabled: order.slice(1), ...(k % 2 ? {} : { cap: { "*": { "5h": 80 + (k % 20) } } }), atLimit: "continue", wait: 40 },
        rev: 2,
        updatedAt: 0,
        isDefault: k === 0,
        users: {
          live: live.length,
          ended: k % 3,
          follow: k === 0 ? live.length : 0,
          doing: Object.fromEntries(live.map((sid) => [sid, { state: "working", needs: null, text: copyText("beSession.activity.working"), tone: "now" }])),
          sids: live,
          endedSids: [],
        },
        summary: `${order.slice(1).join(" → ")} · ${k % 2 ? "满" : `≥${80 + (k % 20)}%`}`,
        explain: "起始账号先用 · 被拒才换 · 不主动换回",
        missing: [],
        atLimitApplies: k % 2 === 0,
      };
    }),
  });
  w.ops["rotation-plan"] = (_o, req) => (req.machine ? fakePlan({ view: (req.view as "6h" | "24h" | "7d") ?? "24h", session: false, warm: true }) : { errors: [] });

  // ── 扩展：几百个 × 十几台 ──
  // 造一次、记成线上那一串：之后每问只 `JSON.parse` 一遍（真壳也是后端交一串、页里解析）—— 每问现拼几百行的深拷贝是假后端自己的活，不该算进产品读数。
  const extList = w.ops["ext-list"];
  let wire: string | null = null;
  w.ops["ext-list"] = (origin, req, world) => JSON.parse((wire ??= JSON.stringify(bigExt(origin, req, world)))) as unknown;
  const bigExt: OpHandler = (origin, req, world) => {
    const g = extList(origin, req, world) as { machines: Record<string, unknown>[]; rows: { name: string; cells: unknown[] }[] };
    const want = 1 + SETTINGS_SCALE.remotes;
    while (g.machines.length < want) {
      const k = g.machines.length;
      g.machines.push({ here: false, key: `node-x${k}`, name: `node-x${k}`, projects: [], reachable: k % 4 !== 0 });
      for (const r of g.rows) r.cells.push(JSON.parse(JSON.stringify(r.cells[r.cells.length - 1])));
    }
    const seed = g.rows.filter((r) => (r as unknown as { builtin: unknown }).builtin === null);
    const rows = [...g.rows];
    for (let k = 0; rows.length < SETTINGS_SCALE.ext; k++) {
      const from = seed[k % seed.length];
      const kind = k % 3 === 0 ? "mcp" : "skill";
      rows.push({ ...JSON.parse(JSON.stringify(from)), name: `${kind}-tool-${String(k).padStart(3, "0")}`, kind, new: k % 17 === 0 });
    }
    return { ...g, rows };
  };
  return w;
}
