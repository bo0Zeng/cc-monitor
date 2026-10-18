/**
 * 帧命令的默认答法（按 op 名）。形状照后端的跨语言金样（`tests/__fixtures__/*.golden.json`）与界面那一侧的解码器。
 */
import type { OpHandler } from "./types";
import type { Opened } from "../../../src/frontend/ui/generated/Opened";
import type { ArriveAsk } from "../../../src/frontend/ui/generated/ArriveAsk";

/** 新起的那一个先定好的 sid（世界里没有它 ⇒ 一直不报到）。 */
const NEW_SID = "5e55fff0-0000-4000-8000-00000000fff0";

/** 等报到那一问（`session-open` 回包里那一格，原样交回真后端 `session-arrive`）：认那个 sid · 起在哪个 tmux 会话（开窗起的 ⇒ `null`）。 */
export const arrivalOf = (sid: string, session: string | null): ArriveAsk => ({ by: { kind: "sid", sid }, since: 0, session });

export const ACCOUNTS = [
  { name: "work", email: "work@example.com", authKind: "subscription", isDefault: true },
  { name: "personal", email: "me@example.com", authKind: "subscription", isDefault: false },
  { name: "api", email: "", authKind: "api-key", isDefault: false, keyMasked: "••••••••a1b2", baseUrl: "https://api.example.com" },
];

export function defaultOps(): Record<string, OpHandler> {
  return {
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
    "session-open": (_o, req): Opened => {
      const what = req.what as { kind: string; sid?: string; session?: string; agent?: string; cwd?: string };
      if (what.kind === "look") return { landed: "running", session: what.session ?? "cc-proj", line: `ccm --attach ${what.session ?? "cc-proj"}`, sid: what.sid ?? null, agent: null, cwd: null, account: null, note: null, prompt: null, arrival: null };
      if (what.kind === "new") {
        const session = req.run !== "direct" ? "orders-cc-2" : null;
        return { landed: session ? "started" : "window", session, line: session ? `ccm --attach ${session}` : "ccm -- new", sid: NEW_SID, agent: (what.agent ?? "claude") as Opened["agent"], cwd: what.cwd ?? "/home/user", account: null, note: null, prompt: null, arrival: arrivalOf(NEW_SID, session) };
      }
      const sid = what.sid ?? null;
      return { landed: "window", session: null, line: `ccm --resume ${sid ?? ""}`, sid, agent: "claude", cwd: "/home/user", account: null, note: null, prompt: null, arrival: sid === null ? null : arrivalOf(sid, null) };
    },
  };
}
