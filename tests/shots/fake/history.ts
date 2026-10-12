/**
 * 历史页那几十条历史会话落盘：照 Claude Code / Codex 落盘的原始文件写（记录 · 修改时刻 · 注解 · 起会话账号记录），
 * 交真后端出清单与搜索（`history-list` · `history-search` · `history-search-merge` · `history-find` 都在 `backend.ts::REAL_OPS`）。
 * 世界里那几个会话（`world.sessions`）本来就落了盘，这里只写清单里另外那几条。正文全是编的占位。
 */
import { Convo } from "./records";
import type { SessionSpec } from "./types";
import type { MachineDisk } from "../disk";

/** 「搜内容」那一场敲的词（下面几条会话的原文里有它）。 */
export const SEARCH_WORD = "重试";

interface Past {
  sid: string;
  cwd: string;
  title: string;
  /** 人说的第一句。 */
  ask: string;
  /** 最后一次写是几分钟前（文件修改时刻）。 */
  agoMin: number;
  /** 盘上大约几行（一问一答往后补）。 */
  lines: number;
  starred?: boolean;
  /** 后台分身那一种（记录上 `sessionKind: "bg"`）。 */
  bg?: boolean;
  /** 从哪个会话分叉来的（记录上 `forkedFrom`）。 */
  fork?: string;
  /** 上次用哪个号起的（`launch-accounts.json`）。 */
  lastAccount?: string;
  /** 回复里说过的几句（含 [`SEARCH_WORD`] 的那几句「搜内容」搜得到）。 */
  replies?: string[];
}

const sid = (n: number): string => `0000${n.toString(16).padStart(4, "0")}-0000-4000-8000-0000000000${(n % 100).toString().padStart(2, "0")}`;
const DEVBOX_BILLING = "5e550004-0000-4000-8000-000000000004";

/** 世界以外的那几条（按机器）。 */
const PAST: Record<string, Past[]> = {
  "<local>": [
    { sid: sid(11), cwd: "/home/user/work/orders", title: "支付回调验签", ask: "支付回调偶尔验签失败，查一下", agoMin: 60 * 26, lines: 88, starred: true, lastAccount: "work" },
    { sid: sid(12), cwd: "/home/user/work/orders", title: "订单导出 CSV", ask: "导出订单列表要支持按月份筛选", agoMin: 60 * 24 * 4, lines: 31, replies: [`导出失败时自动${SEARCH_WORD}一次`] },
    { sid: sid(13), cwd: "/home/user/work/orders", title: "日志采样率", ask: "生产日志太多，加采样", agoMin: 60 * 24 * 9, lines: 12, bg: true },
    { sid: sid(21), cwd: "/home/user/work/web-console", title: "暗色主题对比度", ask: "暗色下按钮文字看不清", agoMin: 60 * 50, lines: 54 },
  ],
  devbox: [
    { sid: sid(42), cwd: "/srv/app/billing", title: "流式导出加断点续传", ask: "从上面那一轮分出来试断点续传", agoMin: 200, lines: 9, fork: DEVBOX_BILLING },
    { sid: sid(43), cwd: "/srv/app/billing", title: "流式导出改用游标", ask: "换一种做法：数据库游标", agoMin: 260, lines: 14, fork: DEVBOX_BILLING, replies: [`游标读失败时${SEARCH_WORD}三次后放弃`] },
    { sid: sid(41), cwd: "/srv/app/billing", title: "发票号生成冲突", ask: "并发开票时发票号重复", agoMin: 60 * 30, lines: 67 },
  ],
};

/** 本机那条 Codex 会话：开了没说话（rollout 只有开头那一条 `session_meta`）。 */
export const CODEX_SID = "019a0000-0000-7000-8000-00000000c0de";
const CODEX_CWD = "/home/user/work/ranker";
const CODEX_AGO_MIN = 180;

function convoOf(p: Past, now: number): Convo {
  const end = now - p.agoMin * 60_000;
  const c = new Convo(p.sid, p.cwd, new Date(end - 40 * 60_000).toISOString());
  c.title(p.title).user(p.ask);
  const replies = p.replies ?? [];
  for (let i = 0; c.lines.length < p.lines; i++) {
    if (i > 0) c.user(`接着第 ${i + 1} 步。`);
    c.say(replies[i] ?? `第 ${i + 1} 步做完了。`);
  }
  for (const l of c.lines) {
    if (typeof l === "string" || (l.type !== "user" && l.type !== "assistant")) continue;
    if (p.bg) l.sessionKind = "bg";
    if (p.fork) l.forkedFrom = { sessionId: p.fork, messageUuid: "00000000-0000-4000-8000-0000000000f0" };
  }
  c.writtenAt = end;
  return c;
}

const json = (v: unknown): string => JSON.stringify(v, null, 2) + "\n";

/**
 * 那台盘上添历史那几份：世界以外的会话记录（带修改时刻）· 注解（星标）· 起会话账号记录（世界里那台各会话 ＋ 清单里记着的那几条）· 本机一条 Codex 会话。
 * 世界里各会话上次用的号照 `accounts` 轮着记（同主窗口那几问一直以来的样子）。
 */
export function putHistory(m: string, d: MachineDisk, sessions: readonly SessionSpec[], accounts: readonly string[], now: number): void {
  const past = PAST[m] ?? [];
  for (const p of past) {
    const c = convoOf(p, now);
    Object.assign(d.files, c.files());
    d.mtimes = { ...d.mtimes, ...c.mtimes() };
  }
  const starred = past.filter((p) => p.starred);
  if (starred.length > 0) {
    d.files[".cc-monitor/history-metadata.json"] = json({ version: 1, entries: Object.fromEntries(starred.map((p) => [p.sid, { starred: true, updatedAt: now - p.agoMin * 60_000 }])) });
  }
  const book: Record<string, string> = {};
  sessions.forEach((s, i) => {
    if (s.origin === m) book[s.sid] = accounts[i % accounts.length];
  });
  for (const p of past) if (p.lastAccount) book[p.sid] = p.lastAccount;
  if (Object.keys(book).length > 0) d.files[".cc-monitor/launch-accounts.json"] = json({ sessions: book });
  if (m === "<local>") {
    const at = now - CODEX_AGO_MIN * 60_000;
    const rel = codexRolloutRel(now);
    d.files[rel] = JSON.stringify({ timestamp: new Date(at).toISOString(), type: "session_meta", payload: { id: CODEX_SID, cwd: CODEX_CWD } }) + "\n";
    d.mtimes = { ...d.mtimes, [rel]: at };
  }
}

/** 本机那条 Codex 会话的 rollout 在家目录下的相对路径（场景要它说过话时整份换掉）。 */
export function codexRolloutRel(now: number): string {
  const day = new Date(now - CODEX_AGO_MIN * 60_000).toISOString().slice(0, 10);
  return `.codex/sessions/${day.split("-").join("/")}/rollout-${day}T06-30-00-${CODEX_SID}.jsonl`;
}

/** 同一条 Codex 会话说过一轮话的那一份 rollout（开头 · 你说的一句 · 它回的一句 · 一轮完了）。 */
export function codexSaidRollout(now: number): string {
  const t = (s: number): string => new Date(now - CODEX_AGO_MIN * 60_000 + s * 1000).toISOString();
  const item = (s: number, type: string, payload: Record<string, unknown>): string => JSON.stringify({ timestamp: t(s), type, payload });
  return [
    item(0, "session_meta", { id: CODEX_SID, cwd: CODEX_CWD }),
    item(2, "event_msg", { type: "task_started", turn_id: "t1" }),
    item(2, "response_item", { type: "message", role: "user", content: [{ type: "input_text", text: "评测脚本里的指标改成按查询平均。" }] }),
    item(9, "response_item", { type: "message", role: "assistant", content: [{ type: "output_text", text: "改好了：每个查询先各自算指标，再按查询数取平均。" }] }),
    item(9, "event_msg", { type: "task_complete", turn_id: "t1" }),
  ].map((l) => l + "\n").join("");
}
