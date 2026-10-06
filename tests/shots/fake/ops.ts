/**
 * 帧命令的默认答法（按 op 名）。形状照后端的跨语言金样（`tests/__fixtures__/*.golden.json`）与界面那一侧的解码器。
 */
import type { JsonlRecord } from "../../../src/frontend/ui/generated/JsonlRecord";
import type { OpHandler, SessionSpec, World } from "./types";

export const jsonlPathOf = (s: SessionSpec): string => `${s.cwd}/${s.sid}.jsonl`;

function sessionByPath(w: World, path: unknown): SessionSpec | undefined {
  return w.sessions.find((s) => typeof path === "string" && path.endsWith(`${s.sid}.jsonl`));
}

/** 记录行的字节布局（骨架 / 大纲按偏移读）：每条一行 JSON。 */
function layout(records: JsonlRecord[]): { at: { o: number; n: number }[]; end: number } {
  let o = 0;
  const at = records.map((r) => {
    const n = JSON.stringify(r).length + 1;
    const here = { o, n };
    o += n;
    return here;
  });
  return { at, end: o };
}

const textOf = (r: JsonlRecord): string => {
  if (r.type === "user") return r.userText.text;
  if (r.type === "assistant" && Array.isArray(r.message.content)) {
    return (r.message.content as { type: string; text?: string }[])
      .filter((b) => b.type === "text")
      .map((b) => b.text ?? "")
      .join("\n");
  }
  return "";
};

export const ACCOUNTS = [
  { name: "work", email: "work@example.com", authKind: "subscription", isDefault: true },
  { name: "personal", email: "me@example.com", authKind: "subscription", isDefault: false },
  { name: "api", email: "", authKind: "api-key", isDefault: false },
];

export function defaultOps(): Record<string, OpHandler> {
  return {
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
      })),
      meta: {
        enabled: true,
        acctsDir: "/home/user/.cc-monitor/accounts",
        manifestPath: "/home/user/.cc-monitor/accounts/accounts.json",
        updatedAt: "2026-10-01T08:00:00Z",
        sharedStore: "/home/user/.cc-monitor/accounts/shared",
        count: ACCOUNTS.length,
        error: null,
        unsupported: null,
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
            launchId: null,
            viaRelay: null,
          }),
        ),
    }),
    "accounts-trust": () => ({ known: true, trusted: true }),
    // 额度与轮换：缺省世界里中转还没见过任何回包、任何会话（额度场景在 `scenes/acct.ts` 整条覆盖）。
    "quota-read": () => ({ state: "absent", reason: null, path: "/home/user/.cc-monitor/quota.json", now: Math.floor(Date.now() / 1000), accounts: [], unseen: [], usableNow: [], earliestReturn: null }),
    "rotation-read": () => ({ state: "absent", reason: null, path: "/home/user/.cc-monitor/rotation.json", rotation: { order: [{ start: true }], enabled: [], when: "full", atLimit: "continue" }, followers: 0 }),
    "rotation-session-read": (_o, req) => ({
      state: "absent",
      reason: null,
      now: Math.floor(Date.now() / 1000),
      sessions: Object.fromEntries((req.sids as string[]).map((sid) => [sid, { state: "absent", inPlace: "noRelay" }])),
    }),
    "history-last-accounts": (_o, _r, w) => ({
      accounts: Object.fromEntries(w.sessions.map((s, i) => [s.sid, ACCOUNTS[i % 2].name])),
    }),
    "tasks-list": (_o, req, w) => ({ tasks: w.sessions.find((s) => s.sid === req.sid)?.tasks ?? [] }),
    // 会打断什么：合成世界里每个会话都当它有一轮在跑、带它在做的任务。
    "session-interrupts": (_o, req, w) => {
      const tasks = (w.sessions.find((s) => s.sid === req.sid)?.tasks ?? []).filter((t) => t.status === "in_progress").map((t) => t.subject);
      return { families: [{ family: "turn", names: [] }, ...(tasks.length ? [{ family: "task", names: tasks }] : [])] };
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
        lines: recs.map((message, i) => ({ session_id: s.sid, path: jsonlPathOf(s), seq: from + i, cwd: s.cwd, message })),
      };
    },
    "history-index": (_o, req, w) => {
      const recs = sessionByPath(w, req.path)?.records ?? [];
      const { at, end } = layout(recs);
      const rows = recs.map((r, i) => {
        const t = textOf(r);
        const row: Record<string, unknown> = { o: at[i].o, n: at[i].n, t: r.type, ch: t.length, pl: t.split("\n").filter((l) => l.trim() !== "").length };
        if ("uuid" in r && r.uuid) row.u = r.uuid;
        if (r.type === "user" && t) {
          row.x = t;
          row.ts = r.timestamp;
        }
        if ((r.type === "user" || r.type === "assistant") && Array.isArray(r.message.content)) {
          row.fd = (r.message.content as { type: string }[]).filter((b) => b.type !== "text").length;
        }
        return row;
      });
      return { from: 0, end, rows };
    },
    "history-user-inputs": (_o, req, w) => {
      const recs = sessionByPath(w, req.path)?.records ?? [];
      return {
        from: 0,
        end: layout(recs).end,
        entries: recs
          .filter((r): r is Extract<JsonlRecord, { type: "user" }> => r.type === "user" && r.userText.speaker.kind === "human" && r.userText.text !== "")
          .map((r) => ({ uuid: r.uuid, excerpt: r.userText.text.slice(0, 120), timestamp: r.timestamp })),
      };
    },
    // 一轮的摘要：照后端 `observe/turns.rs` 那几条口径（你说的一句起、到下一句；结论 ＝ 最后一次工具调用之后带正文的那几条）。
    "history-turns": (_o, req, w) => {
      const recs = sessionByPath(w, req.path)?.records ?? [];
      const { at, end } = layout(recs);
      type T = { at: number; uuid: string; start: string; end: string; said: string; tools: number; thinking: number; fails: number; conclusion: string[]; reply: string; done: boolean };
      const turns: T[] = [];
      let cur: T | null = null;
      let afterTool: string[] = [];
      const close = (t: T): void => {
        t.conclusion = afterTool;
        const texts = recs.filter((r): r is Extract<JsonlRecord, { type: "assistant" }> => r.type === "assistant" && afterTool.includes(r.uuid));
        const body = texts.flatMap((r) => ((r.message.content as { type: string; text?: string }[]).filter((b) => b.type === "text").map((b) => b.text ?? "")));
        t.reply = body.join("\n").split("\n").filter((l) => l.trim()).slice(0, 3).join("\n").slice(0, 120);
      };
      recs.forEach((r, i) => {
        if (r.type === "user" && r.userText.speaker.kind === "human" && r.userText.text !== "" && r.uuid) {
          if (cur) {
            cur.done = true;
            close(cur);
          }
          cur = { at: at[i].o, uuid: r.uuid, start: r.timestamp, end: r.timestamp, said: r.userText.text.split("\n")[0].slice(0, 50), tools: 0, thinking: 0, fails: 0, conclusion: [], reply: "", done: false };
          turns.push(cur);
          afterTool = [];
          return;
        }
        if (!cur || (r.type !== "user" && r.type !== "assistant")) return;
        cur.end = r.timestamp;
        const content = Array.isArray(r.message.content) ? (r.message.content as { type: string; text?: string; is_error?: boolean }[]) : [];
        if (r.type === "assistant") {
          for (const b of content) {
            if (b.type === "tool_use") {
              cur.tools++;
              afterTool = [];
            }
            if (b.type === "thinking") cur.thinking++;
          }
          if (!(r as { isApiErrorMessage?: boolean }).isApiErrorMessage && content.some((b) => b.type === "text" && b.text?.trim())) afterTool.push(r.uuid);
          if (r.message.stop_reason === "end_turn") cur.done = true;
        } else {
          cur.fails += content.filter((b) => b.type === "tool_result" && b.is_error).length;
        }
      });
      if (cur) close(cur);
      return { from: 0, end, turns };
    },
    "history-facts": (_o, req, w) => {
      const s = sessionByPath(w, req.path);
      const recs = s?.records ?? [];
      const last = [...recs].reverse().find((r) => r.type === "assistant" && r.message.usage);
      const touched = new Set<string>();
      for (const r of recs) {
        if (r.type !== "assistant" || !Array.isArray(r.message.content)) continue;
        for (const b of r.message.content as { type: string; name?: string; input?: { file_path?: string } }[]) {
          if (b.type === "tool_use" && (b.name === "Edit" || b.name === "Write") && b.input?.file_path) touched.add(b.input.file_path);
        }
      }
      let usage: { promptTokens: number; model: string | null; peakPromptTokens: number; limit: number; limitFrom: string } | null = null;
      if (last && last.type === "assistant" && last.message.usage) {
        const sum = (u: { input_tokens: number; cache_creation_input_tokens: number; cache_read_input_tokens: number }) =>
          u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens;
        let peak = 0;
        for (const r of recs) if (r.type === "assistant" && r.message.usage) peak = Math.max(peak, sum(r.message.usage));
        // 上限照后端那一条判：中转看见过 ⇒ 带没带扩展上下文那一项；没看见过 ⇒ 带 [1m] / 见过超过 200k ⇒ 1M；判不出 ⇒ assumed
        const relay = s?.relay;
        const from = relay ? "relay" : (last.message.model ?? "").includes("[1m]") ? "model" : peak > 200_000 ? "observed" : "assumed";
        const limit = relay === "std" && peak <= 200_000 ? 200_000 : 1_000_000;
        usage = { promptTokens: sum(last.message.usage), model: last.message.model, peakPromptTokens: peak, limit, limitFrom: from };
      }
      // 没结果的调用 · 最后一句 · 需要你：照后端 `facts_query` 那几条口径（结果按 id 摘、你发一句全摘；在等 ⇒ 配上没结果的那一步）。
      const what = (name: string, input: Record<string, unknown> | undefined): string | null => {
        if (!input) return null;
        if (name === "AskUserQuestion") return ((input.questions as { question?: string }[] | undefined)?.[0]?.question ?? null) || null;
        const key = ({ Bash: "command", Read: "file_path", Edit: "file_path", Write: "file_path", Grep: "pattern", Glob: "pattern", WebFetch: "url" } as Record<string, string>)[name];
        const v = key ? input[key] : undefined;
        return typeof v === "string" ? (v.split("\n").find((l) => l.trim()) ?? "").trim() || null : null;
      };
      let pending: { id: string; name: string; what: string | null; at: string | null }[] = [];
      let lastSay: { text: string; at: string | null } | null = null;
      for (const r of recs) {
        const content = (r as { message?: { content?: unknown } }).message?.content;
        const at = (r as { timestamp?: string }).timestamp ?? null;
        if (r.type === "assistant" && Array.isArray(content)) {
          for (const b of content as { type: string; id?: string; name?: string; text?: string; input?: Record<string, unknown> }[]) {
            if (b.type === "text" && b.text?.trim()) lastSay = { text: b.text.split("\n").find((l) => l.trim())!.trim().slice(0, 160), at };
            if (b.type === "tool_use" && b.id && b.name) pending.push({ id: b.id, name: b.name, what: what(b.name, b.input), at });
          }
        } else if (r.type === "user") {
          const results = Array.isArray(content) ? (content as { type: string; tool_use_id?: string }[]).filter((b) => b.type === "tool_result") : [];
          pending = results.length > 0 ? pending.filter((p) => !results.some((b) => b.tool_use_id === p.id)) : [];
        }
      }
      let needs: { kind: string; tool: string | null; what: string | null; sinceMs: number | null } | null = null;
      if (s?.status === "waiting") {
        const ask = pending.find((p) => p.name === "AskUserQuestion");
        const plan = pending.find((p) => p.name === "ExitPlanMode");
        const sinceMs = s.waitingSinceMs ?? null;
        if (ask) needs = { kind: "answer", tool: ask.name, what: ask.what, sinceMs };
        else if (plan) needs = { kind: "plan", tool: plan.name, what: null, sinceMs };
        else if (pending[0] && /permission/i.test(s.waitingFor ?? "")) needs = { kind: "approve", tool: pending[0].name, what: pending[0].what, sinceMs };
        else needs = { kind: "unknown", tool: null, what: null, sinceMs };
      }
      return { end: layout(recs).end, forkedFrom: null, projectDir: s?.cwd ?? null, touchedFiles: [...touched], usage, writers: [], pending, lastSay, needs };
    },
    "history-run": (_o, req, w) => {
      const s = sessionByPath(w, req.parent);
      const run = s?.runs.find((r) => r.run === req.run || (req.tool !== undefined && r.tool === req.tool));
      const rows = run && s ? (s.runRecords[run.run] ?? []).map((message) => ({ message })) : [];
      const path = `${s?.cwd ?? ""}/${s?.sid ?? ""}/subagents/${run?.run ?? "x"}.jsonl`;
      return { run: run?.run ?? String(req.run ?? ""), path, rows, end: rows.length * 400, more: false };
    },
  };
}
