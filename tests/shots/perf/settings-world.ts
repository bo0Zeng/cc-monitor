/**
 * 设置窗性能台架的合成世界：照真实规模放大 —— 十几台机器、二十几个账号、三十条轮换规则、几百个扩展 × 十几台。
 * 名字全是按模板拼的占位；账号 · 额度 · 轮换落成家目录里的原始文件（`../disk`），摘要 · 说明 · 在用名单由真后端算（与 `fake/world.ts` 同一套口径），不取任何真配置。
 *
 * 量法与读数住 `settings-bench.mjs` 头注；这里只造数据。
 */
import { putAccounts, putQuota, putRotation, putWarm } from "../disk";
import type { OpHandler, World } from "../fake/types";
import { defaultConfig, defaultDisk, defaultWorld, LOCAL, session, sidOf } from "../fake/world";
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
  w.disk = defaultDisk(w.machines, w.sessions);

  // ── 账号 · 额度 · 轮换规则：落成本机家目录里的原始文件，真后端读 ──
  const d = w.disk[LOCAL];
  const t = now();
  putAccounts(d, accts.map((name, i) => ({ name, kind: i % 5 === 4 ? ("api" as const) : ("sub" as const), isDefault: i === 0, ...(i % 5 === 4 ? { email: "" } : {}) })));
  putQuota(
    d,
    accts.map((account, i) => {
      const p5 = (i * 37) % 101;
      return i % 5 === 4
        ? { account, seenAt: t - 60 * (i + 1), status: "allowed" as const, windows: [] }
        : {
            account,
            seenAt: t - 60 * (i + 1),
            status: p5 >= 100 ? ("rejected" as const) : ("allowed" as const),
            refused: p5 >= 100,
            limiting: "five_hour",
            ...(p5 >= 100 ? { resetsAt: t + 600 * (i + 1) } : {}),
            windows: [
              { name: "five_hour", used: p5 / 100, resetsAt: t + 600 * (i + 1) },
              { name: "seven_day", used: ((i * 13) % 100) / 100, resetsAt: t + 3600 * (i + 20) },
            ],
          };
    }),
  );
  const sids = w.sessions.map((s) => s.sid);
  const ruleId = (k: number): string => `r_${String(k).padStart(3, "0")}`;
  putRotation(d, {
    defaultRule: ruleId(0),
    rules: Object.fromEntries(
      Array.from({ length: SETTINGS_SCALE.rules }, (_, k) => {
        const order = [{ start: true }, ...Array.from({ length: 4 }, (_, j) => accts[(k + j) % accts.length])];
        return [ruleId(k), { name: `规则 ${k + 1}`, rotation: { order, enabled: order.slice(1), ...(k % 2 ? {} : { cap: { "*": { "5h": 80 + (k % 20) } } }), atLimit: "continue", wait: 40 }, rev: 2, updatedAt: t - 86_400 }];
      }),
    ),
    sessions: Object.fromEntries(sids.map((sid, j) => [sid, { start: accts[0], current: accts[0], since: t - 3600, source: j % SETTINGS_SCALE.rules === 0 ? "follow" : { rule: ruleId(j % SETTINGS_SCALE.rules) } }])),
  });
  putWarm(d, [{ account: accts[1], at: t + 2.5 * 3600 }]);
  w.ops["accounts-sessions"] = (origin, _req, world) => ({
    lines: world.sessions
      .filter((s) => s.origin === origin)
      .map((s, i) => JSON.stringify({ pid: 4100 + i, sessionId: s.sid, cwd: s.cwd, configDir: `/home/user/.cc-monitor/accounts/${accts[i % accts.length]}`, account: accts[i % accts.length], bare: false, alive: true, viaRelay: null })),
  });
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
