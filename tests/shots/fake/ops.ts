/**
 * 帧命令的默认答法（按 op 名）。形状照后端的跨语言金样（`tests/__fixtures__/*.golden.json`）与界面那一侧的解码器。
 */
import { hm } from "./clock";
import type { OpHandler } from "./types";

export const ACCOUNTS = [
  { name: "work", email: "work@example.com", authKind: "subscription", isDefault: true },
  { name: "personal", email: "me@example.com", authKind: "subscription", isDefault: false },
  { name: "api", email: "", authKind: "api-key", isDefault: false, keyMasked: "••••••••a1b2", baseUrl: "https://api.example.com" },
];

export function defaultOps(): Record<string, OpHandler> {
  return {
    // 计划清单：默认世界里没有计划（计划页的世界 `plan.ts::planWorld` 换掉这一条）。
    "plan-list": () => ({ pb: { state: "ok", said: null }, workspaces: [] }),
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
            aliasNames: name ? [`${name}cc`, `${name}cct`] : [],
            keyMasked: null,
            keyProblem: null,
            aliases: [],
          };
        },
      ]),
    ),
    "tasks-list": (_o, req, w) => ({ tasks: w.sessions.find((s) => s.sid === req.sid)?.tasks ?? [] }),
    // 起新会话框那三问：那台最近用过的目录（它上面那几个会话的目录）· 有 tmux · 只有一家能起；分叉那一形照源会话给三格。
    "session-new-facts": (origin, req, w) => {
      const mine = w.sessions.filter((s) => s.origin === origin);
      const src = w.sessions.find((s) => s.sid === req.forkOf);
      return {
        recent: [...new Set(mine.map((s) => s.cwd))].map((cwd, i) => ({ cwd, lastMs: Date.now() - i * 60_000, lastText: "09:30" })),
        tmux: !/win/.test(origin),
        agents: [{ kind: "claude", text: "Claude Code", models: [], permissionModes: [] }],
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
      prompt: null,
    }),
  };
}
