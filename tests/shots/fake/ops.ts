/**
 * 帧命令的默认答法（按 op 名）。形状照后端的跨语言金样（`tests/__fixtures__/*.golden.json`）与界面那一侧的解码器。
 */
import type { OpHandler } from "./types";

export const ACCOUNTS = [
  { name: "work", email: "work@example.com", authKind: "subscription", isDefault: true },
  { name: "personal", email: "me@example.com", authKind: "subscription", isDefault: false },
  { name: "api", email: "", authKind: "api-key", isDefault: false, keyMasked: "••••••••a1b2", baseUrl: "https://api.example.com" },
];

export function defaultOps(): Record<string, OpHandler> {
  return {
    // 会话外的需手动清单：默认世界里没有（「需手动」浮层与登录抽屉那几张的世界 `scenes/login.ts` 换掉这一条）。
    "manual-list": () => ({ items: [] }),
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
    // 起新会话框的另两问：那个目录在不在 · 起会话那一下的结局（那份事实交真后端答）。
    "session-open-dir": (_o, req) => ({ exists: !String(req.cwd).includes("nope"), tmuxName: `${String(req.cwd).split("/").filter(Boolean).pop() ?? "session"}-cc` }),
    // 起新的 · 恢复 · 接回：那台照意图答落在哪 ＋ 那一行（新起 ⇒ tmux 里起好 / 开窗起；恢复 ⇒ 开窗起；接回 ⇒ 接上在跑的那个）。
    "session-open": (_o, req) => {
      const what = req.what as { kind: string; sid?: string; session?: string; agent?: string; cwd?: string };
      if (what.kind === "look") return { landed: "running", session: what.session ?? "cc-proj", line: `ccm --attach ${what.session ?? "cc-proj"}`, sid: what.sid ?? null, agent: null, cwd: null, account: null, note: null, prompt: null };
      if (what.kind === "new") {
        const tmux = req.run !== "direct";
        return { landed: tmux ? "started" : "window", session: tmux ? "orders-cc-2" : null, line: tmux ? "ccm --attach orders-cc-2" : "ccm -- new", sid: null, agent: what.agent ?? "claude", cwd: what.cwd ?? "~", account: null, note: null, prompt: null };
      }
      return { landed: "window", session: null, line: `ccm --resume ${what.sid ?? ""}`, sid: what.sid ?? null, agent: "claude", cwd: "~", account: null, note: null, prompt: null };
    },
  };
}
