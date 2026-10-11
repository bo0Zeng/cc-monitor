/**
 * 帧命令的默认答法（按 op 名）。形状照后端的跨语言金样（`tests/__fixtures__/*.golden.json`）与界面那一侧的解码器。
 */
import { packIndex } from "../../test-support/pack-index";
import { hm } from "./clock";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { fmtDur } from "../../../src/frontend/ui/duration-format";
import type { LineRecord } from "../../../src/frontend/ui/generated/LineRecord";
import { usageOf } from "./records";
import type { OpHandler, SessionSpec, World } from "./types";

export const jsonlPathOf = (s: SessionSpec): string => `${s.cwd}/${s.sid}.jsonl`;

function sessionByPath(w: World, path: unknown): SessionSpec | undefined {
  return w.sessions.find((s) => typeof path === "string" && path.endsWith(`${s.sid}.jsonl`));
}

/** 记录行的字节布局（骨架 / 大纲按偏移读）：每条一行 JSON。 */
const layouts = new WeakMap<LineRecord[], { at: { o: number; n: number }[]; end: number }>();
function layout(records: LineRecord[]): { at: { o: number; n: number }[]; end: number } {
  // 记一份（同一个数组只量一次）：长会话每问都整份 stringify 一遍，假后端自己就占掉页里一大截主线程（性能台架量的是产品）。
  const had = layouts.get(records);
  if (had && had.at.length === records.length) return had;
  // 只往后长（实时来行）⇒ 只量新来的那几条，不整份重量
  if (had && had.at.length < records.length) {
    for (const r of records.slice(had.at.length)) {
      const n = JSON.stringify(r).length + 1;
      had.at.push({ o: had.end, n });
      had.end += n;
    }
    return had;
  }
  const fresh = layoutOnce(records);
  layouts.set(records, fresh);
  return fresh;
}
function layoutOnce(records: LineRecord[]): { at: { o: number; n: number }[]; end: number } {
  let o = 0;
  const at = records.map((r) => {
    const n = JSON.stringify(r).length + 1;
    const here = { o, n };
    o += n;
    return here;
  });
  return { at, end: o };
}

type Said = Extract<LineRecord, { t: "said" }>;
type Reply = Extract<LineRecord, { t: "reply" }>;
const isSaid = (r: LineRecord): r is Said => r.t === "said";
const isReply = (r: LineRecord): r is Reply => r.t === "reply";
/** 回复里的正文块（按块）。 */
const replyTexts = (r: Reply): string[] => r.blocks.flatMap((b) => (b.type === "text" ? [b.text] : []));
const textOf = (r: LineRecord): string => (isSaid(r) ? r.who.text : isReply(r) ? replyTexts(r).join("\n") : "");

export const ACCOUNTS = [
  { name: "work", email: "work@example.com", authKind: "subscription", isDefault: true },
  { name: "personal", email: "me@example.com", authKind: "subscription", isDefault: false },
  { name: "api", email: "", authKind: "api-key", isDefault: false, keyMasked: "••••••••a1b2", baseUrl: "https://api.example.com" },
];

export function defaultOps(): Record<string, OpHandler> {
  return {
    // 计划清单：默认世界里没有计划（计划页的世界 `plan.ts::planWorld` 换掉这一条）。
    "plan-list": () => ({ pb: { state: "ok", said: null }, workspaces: [] }),
    // 账号库只有 win-laptop 那台由这里答（`backend.ts::WINDOWS_ONLY`：Linux 编的真后端演不出 Windows 那一支）：那台做不了多账号。
    "accounts-list": () => ({
      accounts: ACCOUNTS.map((a) => ({
        name: a.name,
        email: a.email,
        configDir: `/home/user/.cc-monitor/accounts/${a.name}`,
        isDefault: a.isDefault,
        mode: "isolated",
        exists: true,
        loggedIn: a.authKind === "subscription",
        authKind: a.authKind,
        authReady: true,
        selectable: true,
        keyMasked: a.keyMasked ?? null,
        baseUrl: a.baseUrl ?? null,
      })),
      meta: {
        enabled: true,
        acctsDir: "/home/user/.cc-monitor/accounts",
        manifestPath: "/home/user/.cc-monitor/accounts/accounts.json",
        updatedAt: "2026-10-01T08:00:00Z",
        sharedStore: "/home/user/.cc-monitor/accounts/shared",
        count: ACCOUNTS.length,
        error: null,
        unsupported: "Windows 不支持多账号",
        nextDefault: ACCOUNTS.find((a) => !a.isDefault)?.name ?? null,
        effectiveDefault: "work",
        home: "/home/user",
      },
      notice: null,
    }),
    "accounts-sessions": (origin, _req, w) => ({
      lines: w.sessions
        .filter((s) => s.origin === origin && !s.ended)
        .map((s, i) =>
          JSON.stringify({
            pid: 4100 + i,
            sessionId: s.sid,
            cwd: s.cwd,
            configDir: `/home/user/.cc-monitor/accounts/${ACCOUNTS[i % 2].name}`,
            account: ACCOUNTS[i % 2].name,
            bare: false,
            alive: true,
            viaRelay: null,
          }),
        ),
    }),
    "accounts-trust": () => ({ known: true, trusted: true }),
    // 账号页打开时顺手核一次：缺省世界里都对得上。
    "accounts-verify": () => ({ pass: true, fails: 0, warns: 0, checks: [] }),
    // 账号库的改动类（新建 · 设默认 · 删 · 修复 · 回滚）：预演回一步、真做回已做（形状照 `AccountChange`）。
    ...Object.fromEntries(
      ["accounts-add", "accounts-set-default", "accounts-remove", "accounts-repair", "accounts-rollback", "accounts-init"].map((op) => [
        op,
        (_o: unknown, req: Record<string, unknown>) => {
          const name = typeof req.name === "string" ? req.name : "";
          return {
            applied: req.dryRun !== true,
            steps: [`${op} ${name}`.trim()],
            notes: [],
            backup: null,
            account: null,
            loginCmd: null,
            aliasNames: name ? [`${name}cc`, `${name}cct`] : [],
            keyMasked: null,
            keyProblem: null,
            aliases: [],
          };
        },
      ]),
    ),
    "history-last-accounts": (_o, _r, w) => ({
      accounts: Object.fromEntries(w.sessions.map((s, i) => [s.sid, ACCOUNTS[i % 2].name])),
    }),
    "tasks-list": (_o, req, w) => ({ tasks: w.sessions.find((s) => s.sid === req.sid)?.tasks ?? [] }),
    // 起新会话框那三问：那台最近用过的目录（它上面那几个会话的目录）· 有 tmux · 只有一家能起；分叉那一形照源会话给三格。
    "session-new-facts": (origin, req, w) => {
      const mine = w.sessions.filter((s) => s.origin === origin);
      const src = w.sessions.find((s) => s.sid === req.forkOf);
      return {
        recent: [...new Set(mine.map((s) => s.cwd))].map((cwd, i) => ({ cwd, lastMs: Date.now() - i * 60_000 })),
        tmux: !/win/.test(origin),
        agents: ["claude"],
        fork: src
          ? {
              agent: "claude",
              launch: {
                cwd: { kind: "known", value: src.cwd, from: "record" },
                account: src.ended ? { kind: "unknown", why: "exited" } : { kind: "known", value: "personal", from: "process" },
                terminal: src.ended ? { kind: "unknown", why: "exited" } : { kind: "known", value: { host: "tmux", terminal: "tmux:orders-cc" }, from: "terminal_list" },
              },
              turn: 9,
              startText: hm(Date.now() - 3_600_000),
            }
          : null,
      };
    },
    "session-new-dir": (_o, req) => ({ exists: !String(req.cwd).includes("nope"), tmuxName: `${String(req.cwd).split("/").filter(Boolean).pop() ?? "session"}-cc` }),
    "session-new": (_o, req) => ({
      outcome: req.place === "tmux" ? "started" : "open",
      session: req.place === "tmux" ? "orders-cc-2" : null,
      sid: null,
      cmd: req.place === "tmux" ? null : "ccm -- new",
      account: null,
      agent: req.agent,
      cwd: req.cwd,
    }),
    // 会打断什么：合成世界里每个会话都当它有一轮在跑、带它在做的任务。
    "session-interrupts": (_o, req, w) => {
      const tasks = (w.sessions.find((s) => s.sid === req.sid)?.tasks ?? []).filter((t) => t.status === "in_progress").map((t) => t.subject);
      return { families: [{ family: "turn", names: [] }, ...(tasks.length ? [{ family: "task", names: tasks }] : [])] };
    },
    // 查看器整份读（按字节分页，这里一页交完）：历史页右边、只读查看器用它。
    // 带 `until` ＝ 按字节取一段（骨架按偏移取正文）：照字节布局切 `[offset, until)` 那几条；不带 ＝ 从 `offset` 起到末尾。
    "history-page": (_o, req, w) => {
      const s = sessionByPath(w, req.path);
      const recs = s?.records ?? [];
      const path = String(req.path);
      const { at, end } = layout(recs);
      const off = Number(req.offset ?? 0);
      const until = req.until === undefined ? end : Number(req.until);
      let i0 = at.findIndex((a) => a.o >= off);
      if (i0 < 0) i0 = recs.length;
      let i1 = i0;
      while (i1 < recs.length && at[i1].o < until) i1++;
      const base = req.seq === undefined ? i0 : Number(req.seq);
      return {
        lines: recs.slice(i0, i1).map((record, k) => ({ session_id: s?.sid ?? "", path, seq: base + k, cwd: s?.cwd ?? null, record })),
        next: i1 < recs.length ? at[i1].o : end,
        nextSeq: base + (i1 - i0),
        eof: i1 >= recs.length || (i1 < recs.length && at[i1].o >= until),
      };
    },
    "history-lines": (_o, req, w) => {
      const s = sessionByPath(w, req.path);
      const from = Number(req.from ?? 0);
      if (!s) return { from, next: from, eof: true, lines: [] };
      const until = req.until === undefined ? s.records.length : Number(req.until);
      const recs = s.records.slice(from, until);
      return {
        from,
        next: from + recs.length,
        eof: from + recs.length >= s.records.length,
        lines: recs.map((record, i) => ({ session_id: s.sid, path: jsonlPathOf(s), seq: from + i, cwd: s.cwd, record })),
      };
    },
    "history-index": (_o, req, w) => {
      const recs = sessionByPath(w, req.path)?.records ?? [];
      const { at, end } = layout(recs);
      const rows = recs.map((r, i) => {
        const t = textOf(r);
        const row: Record<string, unknown> = { o: at[i].o, n: at[i].n, t: r.t, ch: t.length, pl: t.split("\n").filter((l) => l.trim() !== "").length };
        row.u = r.id;
        if (isSaid(r) && t) {
          row.x = t;
          row.ts = r.at;
        }
        if (isSaid(r) || isReply(r)) row.fd = r.blocks.filter((b) => b.type !== "text").length;
        return row;
      });
      return packIndex(0, end, rows);
    },
    "history-user-inputs": (_o, req, w) => {
      const recs = sessionByPath(w, req.path)?.records ?? [];
      return {
        from: 0,
        end: layout(recs).end,
        entries: recs
          .filter((r): r is Said => isSaid(r) && r.who.speaker.kind === "human" && r.who.text !== "")
          .map((r) => ({ uuid: r.id, excerpt: r.who.text.slice(0, 120), timestamp: r.at })),
      };
    },
    // 一轮的摘要：照后端 `observe/turns.rs` 那几条口径（人说的一句起、到下一句；带正文的与停下这一轮的中断标记 / 报错卡露着，
    // 被它们隔开的连续的其余记录各成一段过程、一段一行；派 agent 不算工具；后台任务通知不数交回过的那个子 agent；
    // 行上的字与右端那一截后端写好；最后一轮还在跑 ⇒ 按会话事实在尾巴那一段补「现在 / 等你」）。
    "history-turns": (o, req, w) => {
      const recs = sessionByPath(w, req.path)?.records ?? [];
      const { at, end } = layout(recs);
      type Part = { text: string; tone: string };
      type Span = { text: string; from: number | null; to: number | null };
      type Fold = { after: string; parts: Part[]; span: Span; phase: string };
      type T = {
        at: number; uuid: string; start: string; end: string; startText: string; endText: string; said: string;
        tools: number; thinking: number; agents: number; background: number; retries: number; peers: number; fails: number;
        shown: string[]; landed: string[]; reply: string; done: boolean; phase: string; folds: Fold[];
      };
      type Item = { id: string; at: string; shown: boolean; tools: number; thinking: number; agents: number; retries: number; peers: number; fails: number; compacts: number; notices: { task: string | null; failed: boolean }[] };
      type Acc = { items: Item[]; stop: number | null; runs: string[]; reply: string[]; pending: { id: string; name: string; what: string | null }[]; tail: boolean; tailStart: string; queued: [string, string][] };
      const turns: T[] = [];
      const accs: Acc[] = [];
      let cur: T | null = null;
      let acc: Acc | null = null;
      const n = (key: Parameters<typeof copyText>[0], v: number): string => copyText(key, { n: String(v) });
      const keys = ["tools", "thinking", "agents", "background", "retries", "peers"] as const;
      const tally = (items: Item[], runs: string[]) => {
        const c = { tools: 0, thinking: 0, agents: 0, background: 0, retries: 0, peers: 0, fails: 0, compacts: 0 };
        for (const it of items) {
          c.tools += it.tools; c.thinking += it.thinking; c.agents += it.agents; c.retries += it.retries; c.peers += it.peers; c.fails += it.fails; c.compacts += it.compacts;
          for (const no of it.notices) {
            if (!(no.task && runs.includes(no.task))) c.background++;
            if (no.failed) c.fails++;
          }
        }
        return c;
      };
      const close = (t: T, x: Acc): void => {
        if (x.stop !== null) x.items[x.stop].shown = true;
        const all = tally(x.items, x.runs);
        for (const k of keys) t[k] = all[k];
        t.fails = all.fails;
        // 只取正文行：代码块整块不算；只有代码 ⇒「仅代码」（同后端 `turns.rs::reply_head`）。
        let fenced = false;
        let code = false;
        const prose = x.reply.join("\n").split("\n").map((l) => l.trim()).filter((l) => {
          if (l.startsWith("```")) {
            fenced = !fenced;
            code = true;
            return false;
          }
          return !fenced && l !== "";
        }).map((l) => l.replace(/^[#>]+\s*/, "").replace(/\*\*|__|`/g, "")).filter((l) => l !== "");
        t.reply = prose.length === 0 && code ? copyText("rsTurns.reply.codeOnly") : prose.slice(0, 3).join("\n").slice(0, 120);
        // 划段（同后端 `Open::close`）。
        let after = t.uuid;
        let seg: Item[] = [];
        const flush = (): void => {
          if (seg.length === 0) return;
          const c = tally(seg, x.runs);
          const others = keys.reduce((s, k) => s + c[k], 0);
          const stamps = seg.map((it) => it.at).filter((a) => a !== "");
          seg = [];
          if (others === 0 && c.compacts <= 1) return;
          const parts: Part[] = [{ text: copyText("rsTurns.proc.head"), tone: "plain" }];
          const counted: [Parameters<typeof copyText>[0], number][] = [
            ["rsTurns.proc.tools", c.tools], ["rsTurns.proc.thinking", c.thinking], ["rsTurns.proc.agents", c.agents],
            ["rsTurns.proc.background", c.background], ["rsTurns.proc.retries", c.retries], ["rsTurns.proc.peers", c.peers],
          ];
          parts.push(...counted.filter(([, v]) => v > 0).map(([k, v]) => ({ text: n(k, v), tone: "plain" })));
          if (c.fails > 0) parts.push({ text: n("rsTurns.proc.fails", c.fails), tone: "fail" });
          const [s0, s1] = [stamps[0] ?? "", stamps.at(-1) ?? ""];
          const [f0, f1] = [hm(Date.parse(s0)), hm(Date.parse(s1))];
          const when = f0 === f1 ? f0 : copyText("rsTurns.span.range", { from: f0, to: f1 });
          const [a, b] = [Date.parse(s0), Date.parse(s1)];
          const span = b - a >= 1000 ? { text: copyText("rsTurns.span.done", { when, dur: "{dur}" }), from: a, to: b } : { text: when, from: null, to: null };
          t.folds.push({ after, parts, span, phase: "idle" });
          x.tailStart = f0;
        };
        for (const it of x.items) {
          if (it.shown) {
            flush();
            t.shown.push(it.id);
            after = it.id;
            continue;
          }
          seg.push(it);
        }
        const open = seg.length > 0;
        const before = t.folds.length;
        flush();
        x.tail = open && t.folds.length > before;
      };
      recs.forEach((r, i) => {
        const sp = isSaid(r) ? r.who.speaker : null;
        const head = sp !== null && isSaid(r) && ((sp.kind === "human" && r.who.text !== "") || sp.kind === "slashCommand" || sp.kind === "bashInput");
        if (head && isSaid(r)) {
          if (cur && acc) {
            cur.done = true;
            // 排队插话落成了这一句（原文一字不差）⇒ 记进排队那一条所在的那一轮（同后端 `TurnScan::line`）。
            const text = r.who.text.trim();
            cur.landed = acc.queued.filter(([, q]) => q === text).map(([id]) => id);
            close(cur, acc);
          }
          const said = sp!.kind === "slashCommand" ? `${(sp as { name: string }).name} ${(sp as { args?: string }).args ?? ""}`.trim() : r.who.text;
          const when = r.at ?? "";
          cur = {
            at: at[i].o, uuid: r.id, start: when, end: when, startText: hm(Date.parse(when)), endText: hm(Date.parse(when)), said: said.split("\n")[0].slice(0, 50),
            tools: 0, thinking: 0, agents: 0, background: 0, retries: 0, peers: 0, fails: 0, shown: [], landed: [], reply: "", done: false, phase: "idle", folds: [],
          };
          acc = { items: [], stop: null, runs: [], reply: [], pending: [], tail: false, tailStart: "", queued: [] };
          turns.push(cur);
          accs.push(acc);
          return;
        }
        if (!cur || !acc) return;
        const it: Item = { id: "id" in r ? String(r.id) : "", at: ("at" in r ? r.at : undefined) ?? "", shown: false, tools: 0, thinking: 0, agents: 0, retries: 0, peers: 0, fails: 0, compacts: 0, notices: [] };
        const here = acc.items.length;
        acc.items.push(it);
        if (r.t === "retry") {
          it.retries++;
          return;
        }
        if (r.t === "queued" && r.who.speaker.kind === "human") acc.queued.push([r.id, r.who.text.trim()]);
        if (!isSaid(r) && !isReply(r)) return;
        cur.end = r.at ?? cur.end;
        cur.endText = hm(Date.parse(cur.end));
        if (isReply(r)) {
          acc.stop = null;
          const kids = r.runs ?? {};
          for (const b of r.blocks) {
            if (b.type === "tool_use") {
              if (b.id in kids) it.agents++;
              else it.tools++;
              const input = (b.input ?? {}) as Record<string, unknown>;
              const what = [input.pattern, input.command, input.file_path].find((v): v is string => typeof v === "string") ?? null;
              acc.pending.push({ id: b.id, name: b.name, what });
              acc.reply = [];
            }
            if (b.type === "thinking") it.thinking++;
          }
          const hasText = r.blocks.some((b) => b.type === "text" && b.text.trim());
          if (r.error) {
            acc.stop = here;
            cur.done = true;
          } else if (hasText) {
            it.shown = true;
            if (it.tools + it.agents === 0) acc.reply.push(...replyTexts(r));
          }
          if (r.endsTurn) cur.done = true;
        } else {
          for (const b of r.blocks) {
            if (b.type !== "tool_result") continue;
            acc.pending = acc.pending.filter((p) => p.id !== b.for);
            if (b.isError) it.fails++;
          }
          const k = r.who.speaker as { kind: string; taskId?: string; status?: string; handback?: boolean; from?: string };
          if (k.kind === "taskNotification") it.notices.push({ task: k.taskId ?? null, failed: k.status === "failed" });
          else if (k.kind === "agentMessage" && k.handback && k.from) acc.runs.push(k.from);
          else if (k.kind === "agentMessage" || k.kind === "peerSession") it.peers++;
          else if (k.kind === "compactSummary") it.compacts++;
          else if (k.kind === "interrupt") {
            acc.stop = here;
            cur.done = true;
            acc.pending = [];
          }
        }
      });
      if (cur && acc) close(cur, acc);
      // 最后一轮还没收尾：按会话事实在尾巴那一段补那一截（同后端 `turns.rs::dress_live`）。
      const last = turns.at(-1);
      const lastAcc = accs.at(-1);
      if (last && lastAcc && !last.done) {
        const f = (defaultOps()["history-facts"](o, { path: req.path }, w) as { needs: { kind: string; tool: string | null; what: string | null; sinceMs: number | null } | null; writers: number[] });
        const step = (name: string, what: string | null): string => (what ? `${name} ${what}` : name);
        const tail = lastAcc.tail ? last.folds.at(-1) : undefined;
        // 假后端的会话时刻是固定的几天前 ⇒「起了多久」按「此刻往前 42 秒」画（真后端是这一段第一条的时刻）。
        const since = { text: copyText("rsTurns.span.since", { hm: lastAcc.tailStart, dur: "{dur}" }), from: Date.now() - 42_000, to: null };
        if (f.needs) {
          last.phase = "awaiting";
          if (tail) {
            tail.phase = "awaiting";
            const nd = f.needs;
            const text = (nd.kind === "approve" || nd.kind === "plan") && nd.tool ? copyText("rsTurns.proc.awaitApprove", { step: step(nd.tool, nd.what) }) : nd.kind === "answer" ? copyText("rsTurns.proc.awaitAnswer", { step: nd.what ?? nd.tool ?? "" }) : copyText("rsTurns.proc.awaitYou");
            tail.parts.push({ text, tone: "need" });
            tail.span = nd.sinceMs !== null ? { text: copyText("rsTurns.span.waited", { dur: "{dur}" }), from: nd.sinceMs, to: null } : since;
          }
        } else if (f.writers.length > 0) {
          last.phase = "running";
          if (tail) {
            tail.phase = "running";
            const p = lastAcc.pending.at(-1);
            if (p) tail.parts.push({ text: copyText("rsTurns.proc.now", { step: step(p.name, p.what) }), tone: "now" });
            tail.span = since;
          }
        }
      }
      // 续取从 `from`（还没收尾的那一轮的开头）起：只交那之后开头的轮（同后端 `turns.rs` 从偏移读）。
      const from = typeof req.from === "number" ? req.from : 0;
      return { from, end, turns: turns.filter((t) => t.at >= from) };
    },
    "history-facts": (_o, req, w) => {
      const s = sessionByPath(w, req.path);
      const recs = s?.records ?? [];
      const last = [...recs].reverse().find((r): r is Reply => isReply(r) && usageOf.has(r));
      const touched = new Set<string>();
      for (const r of recs) {
        if (!isReply(r)) continue;
        for (const b of r.blocks) {
          const fp = b.type === "tool_use" ? (b.input as { file_path?: unknown } | null)?.file_path : undefined;
          if (b.type === "tool_use" && (b.name === "Edit" || b.name === "Write") && typeof fp === "string") touched.add(fp);
        }
      }
      let usage: Record<string, unknown> | null = null;
      const lastUsage = last ? usageOf.get(last) : undefined;
      if (last && lastUsage) {
        const sum = (u: { input_tokens: number; cache_creation_input_tokens: number; cache_read_input_tokens: number }) =>
          u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens;
        let peak = 0;
        for (const r of recs) {
          const u = usageOf.get(r);
          if (u) peak = Math.max(peak, sum(u));
        }
        // 上限照后端那一条判：中转看见过 ⇒ 带没带扩展上下文那一项；没看见过 ⇒ 带 [1m] / 见过超过 200k ⇒ 1M；判不出 ⇒ assumed
        const relay = s?.relay;
        const model = last.model ?? null;
        const from = relay ? "relay" : (model ?? "").includes("[1m]") ? "model" : peak > 200_000 ? "observed" : "assumed";
        const limit = relay === "std" && peak <= 200_000 ? 200_000 : 1_000_000;
        // 字照后端 `facts_query::UsageFact::settle_words`（百分比 · 用了多少 · 上限 · 来源；同一张文案表的 `beUsage.*`）。
        const prompt = sum(lastUsage);
        const short = (n: number) => (n >= 1_000_000 ? `${Math.round(n / 100_000) / 10}M` : n >= 1000 ? `${Math.round(n / 1000)}k` : String(n));
        const percent = from === "assumed" ? null : Math.min(100, Math.round((prompt / limit) * 100));
        const fromText = from === "assumed" ? null : copyText(`beUsage.from.${from}` as "beUsage.from.relay");
        usage = { promptTokens: prompt, model, peakPromptTokens: peak, limit, limitFrom: from, percent, contextText: percent === null ? short(prompt) : copyText("beUsage.context.pct", { n: String(percent) }), contextTone: percent !== null && percent >= 80 ? "warn" : "plain", promptTokensText: short(prompt), limitText: short(limit), limitFromText: fromText };
      }
      // 没结果的调用 · 最后一句 · 需手动：照后端 `facts_query` 那几条口径（结果按 id 摘、你发一句全摘；在等 ⇒ 配上没结果的那一步）。
      const what = (name: string, input: Record<string, unknown> | undefined): string | null => {
        if (!input) return null;
        if (name === "AskUserQuestion") return ((input.questions as { question?: string }[] | undefined)?.[0]?.question ?? null) || null;
        const key = ({ Bash: "command", Read: "file_path", Edit: "file_path", Write: "file_path", Grep: "pattern", Glob: "pattern", WebFetch: "url" } as Record<string, string>)[name];
        const v = key ? input[key] : undefined;
        return typeof v === "string" ? (v.split("\n").find((l) => l.trim()) ?? "").trim() || null : null;
      };
      let pending: { id: string; name: string; what: string | null; at: string | null; atMs: number | null }[] = [];
      let lastSay: { text: string; at: string | null; atMs: number | null } | null = null;
      for (const r of recs) {
        const at = r.at ?? null;
        // 假后端替核心解时刻（真后端在 `facts_query` 里解好交 `atMs`）。
        const atMs = at === null ? null : Date.parse(at);
        if (isReply(r)) {
          for (const b of r.blocks) {
            if (b.type === "text" && b.text.trim()) lastSay = { text: b.text.split("\n").find((l) => l.trim())!.trim().slice(0, 160), at, atMs };
            if (b.type === "tool_use") pending.push({ id: b.id, name: b.name, what: what(b.name, b.input as Record<string, unknown>), at, atMs });
          }
        } else if (isSaid(r)) {
          const results = r.blocks.flatMap((b) => (b.type === "tool_result" ? [b.for] : []));
          pending = results.length > 0 ? pending.filter((p) => !results.includes(p.id)) : [];
        }
      }
      let needs: { kind: string; tool: string | null; call: string | null; what: string | null; sinceMs: number | null; text: string; tone: string; rank?: number } | null = null;
      // 字照后端 `facts_query::needs_of`（同一张文案表的 `beSession.needs.*`）。
      // 字照后端 `needs_words` · `needs_head`（钉条第一行：种类 ＋ 工具名；等宽那一段只批准 · 回答带）。
      const said = (kind: "approve" | "answer" | "plan" | "unknown", tool: string | null = null, code: string | null = null) => ({
        text: copyText(`beSession.needs.${kind}`),
        head: copyText(`needs.bar.${kind}`, { tool: tool ?? "" }),
        headCode: code,
        tone: "need",
      });
      if (s?.activity === "needs_you") {
        const ask = pending.find((p) => p.name === "AskUserQuestion");
        const plan = pending.find((p) => p.name === "ExitPlanMode");
        const sinceMs = s.waitingSinceMs ?? null;
        if (ask) needs = { kind: "answer", tool: ask.name, call: ask.id, what: ask.what, sinceMs, ...said("answer", ask.name, ask.what) };
        else if (plan) needs = { kind: "plan", tool: plan.name, call: plan.id, what: null, sinceMs, ...said("plan") };
        else if (pending[0] && /permission/i.test(s.waitingFor ?? "")) needs = { kind: "approve", tool: pending[0].name, call: pending[0].id, what: pending[0].what, sinceMs, ...said("approve", pending[0].name, pending[0].what) };
        else needs = { kind: "unknown", tool: null, call: null, what: null, sinceMs, ...said("unknown") };
      }
      // 已等多久在那台算（同后端 `needs_of`：读 pidfile 那一刻减起点，字由时长那一处写）；没有起点 ⇒ 两格 null。
      if (needs) {
        const waitedMs = needs.sinceMs === null ? null : Math.max(0, Date.now() - needs.sinceMs);
        needs = { ...needs, rank: 0, waitedMs, waitedText: waitedMs === null ? null : fmtDur(waitedMs / 1000) } as typeof needs;
      }
      // 交回了的子运行：成品里「谁说的」是 agent 交回的那几条的 `from`（去重、文件序；同后端 `facts_query::note_handback`）。
      const handedBack: string[] = [];
      for (const r of recs) {
        const sp = isSaid(r) ? r.who.speaker : null;
        if (sp?.kind === "agentMessage" && sp.handback === true && sp.from && !handedBack.includes(sp.from)) handedBack.push(sp.from);
      }
      // 每步状态（同后端 `facts_query::settle_pending`）：在等的那一步 ⇒ 在等你；会话活着 ⇒ 在跑；否则状态不明（没有进程）。
      const live = s?.activity === "working" || s?.activity === "needs_you";
      const steps = pending.map((p) =>
        needs?.call === p.id ? { ...p, state: "awaiting", why: null } : live ? { ...p, state: "running", why: null } : { ...p, state: "unclear", why: "noWriter" },
      );
      // 一串串重试的结局（同后端 `facts_query::note_retry`）：按首条 uuid；下文是正常回复 ⇒ 接上 · 报错那条 ⇒ 没接上 · 人发一句 ⇒ 中断。
      const retries: { id: string; outcome: string }[] = [];
      for (const r of recs) {
        const open = retries.at(-1)?.outcome === "retrying";
        if (r.t === "retry") {
          if (!open) retries.push({ id: r.id, outcome: "retrying" });
        } else if (open && isReply(r)) {
          retries.at(-1)!.outcome = r.error ? "failed" : "recovered";
        } else if (open && isSaid(r) && r.who.speaker.kind !== "system") {
          if (!r.blocks.some((b) => b.type === "tool_result")) retries.at(-1)!.outcome = "interrupted";
        }
      }
      // 后台任务运行中那一句：照后端 `facts_query::background_of`（命令取最早起的 · 几条 ⇒「等 N 条」· 时长 ＝ 它起了多久；同一张文案表的 `beSession.activity.*`）。
      let background: { text: string; clock: { text: string; from: number } | null; what: string | null; count: number; tone: string } | null = null;
      if (s?.activity === "background_work") {
        const cmds = s.bgCommands ?? [];
        const first = cmds[0];
        const what = first ? (cmds.length > 1 ? copyText("beSession.activity.backgroundMany", { cmd: first.cmd, n: String(cmds.length) }) : first.cmd) : null;
        const line = (dur: string) => copyText("beSession.activity.backgroundFor", { cmd: what ?? "", dur });
        background = first
          ? { text: line(fmtDur((Date.now() - first.sinceMs) / 1000)), clock: { text: line("{dur}"), from: first.sinceMs }, what, count: cmds.length, tone: "busy" }
          : { text: copyText("beSession.activity.backgroundWork"), clock: null, what: null, count: 0, tone: "busy" };
      }
      return { agent: s?.agent ?? "claude", end: layout(recs).end, forkedFrom: null, projectDir: s?.cwd ?? null, touchedFiles: [...touched], usage, writers: live ? [4242] : [], pending: steps, lastSay, needs, handedBack, retries, permissionMode: null, tokens: null, cost: null, bgTasks: [], background, mcp: [] };
    },
    // 主线外清单：假世界的会话都没有回退过。
    "history-branch": (_o, req, w) => ({ off: [], end: layout(sessionByPath(w, req.path)?.records ?? []).end }),
    "history-run": (_o, req, w) => {
      const s = sessionByPath(w, req.parent);
      const run = s?.runs.find((r) => r.run === req.run || (req.tool !== undefined && r.tool === req.tool));
      const all = run && s ? (s.runRecords[run.run] ?? []).map((record) => ({ record })) : [];
      // 一条记录当 400 字节：续读从 `from` 那一条往后。
      const rows = all.slice(Math.floor(Number(req.from ?? 0) / 400));
      const path = `${s?.cwd ?? ""}/${s?.sid ?? ""}/subagents/${run?.run ?? "x"}.jsonl`;
      return { run: run?.run ?? String(req.run ?? ""), path, rows, end: all.length * 400, more: false };
    },
  };
}
