/**
 * 历史页的合成数据：几台机器上的项目与会话、全文搜索的命中。形状照 `history-products.golden.json` 与界面的解码器。
 */
import type { OpHandler } from "./types";

// 历史页按看的人这台的日历分段（今天 · 昨天 · 本周 …）⇒ 合成时间跟着截图那一刻走。
const NOW = Date.now();
const ago = (min: number): number => NOW - min * 60_000;

interface Proj {
  origin: string;
  path: string;
  /** 缺 = claude。 */
  agent?: string;
  sessions: { sid: string; title: string; excerpt: string; agoMin: number; messages: number; live?: boolean; starred?: boolean; bg?: boolean; fork?: string; lastAccount?: string; hits?: { kind: string; before: string; after: string }[] }[];
}

const sid = (n: number): string => `0000${n.toString(16).padStart(4, "0")}-0000-4000-8000-0000000000${(n % 100).toString().padStart(2, "0")}`;

export const SEARCH_WORD = "重试";

export const HISTORY: Proj[] = [
  {
    origin: "<local>",
    path: "/home/user/work/orders",
    sessions: [
      { sid: "5e550001-0000-4000-8000-000000000001", title: "给订单服务加重试与超时", excerpt: "订单服务调用库存接口时偶尔超时…", agoMin: 3, messages: 42, live: true, hits: [{ kind: "user", before: "帮我加上", after: "（指数退避）和整体超时" }, { kind: "assistant", before: "统一做", after: " ＋ 退避 ＋ 整体超时" }] },
      { sid: sid(11), title: "支付回调验签", excerpt: "支付回调偶尔验签失败，查一下", agoMin: 60 * 26, messages: 88, starred: true, lastAccount: "work" },
      { sid: sid(12), title: "订单导出 CSV", excerpt: "导出订单列表要支持按月份筛选", agoMin: 60 * 24 * 4, messages: 31, hits: [{ kind: "assistant", before: "导出失败时自动", after: "一次" }] },
      { sid: sid(13), title: "日志采样率", excerpt: "生产日志太多，加采样", agoMin: 60 * 24 * 9, messages: 12, bg: true },
    ],
  },
  {
    origin: "<local>",
    path: "/home/user/work/web-console",
    sessions: [
      { sid: "5e550002-0000-4000-8000-000000000002", title: "表格分页改成虚拟滚动", excerpt: "表格超过一万行就卡", agoMin: 8, messages: 6, live: true },
      { sid: sid(21), title: "暗色主题对比度", excerpt: "暗色下按钮文字看不清", agoMin: 60 * 50, messages: 54 },
    ],
  },
  {
    origin: "<local>",
    path: "/home/user/work/notes",
    sessions: [{ sid: "5e550003-0000-4000-8000-000000000003", title: "周报草稿", excerpt: "把这周的提交整理成周报", agoMin: 90, messages: 4 }],
  },
  {
    origin: "devbox",
    path: "/srv/app/billing",
    sessions: [
      { sid: "5e550004-0000-4000-8000-000000000004", title: "账单导出改成流式", excerpt: "导出大账单时内存会涨到 4G", agoMin: 15, messages: 20, live: true, hits: [{ kind: "assistant", before: "写入失败时", after: "三次后放弃" }] },
      { sid: sid(42), title: "流式导出加断点续传", excerpt: "从上面那一轮分出来试断点续传", agoMin: 200, messages: 9, fork: "5e550004-0000-4000-8000-000000000004" },
      { sid: sid(43), title: "流式导出改用游标", excerpt: "换一种做法：数据库游标", agoMin: 260, messages: 14, fork: "5e550004-0000-4000-8000-000000000004" },
      { sid: sid(41), title: "发票号生成冲突", excerpt: "并发开票时发票号重复", agoMin: 60 * 30, messages: 67 },
    ],
  },
  {
    origin: "gpu-01",
    path: "/data/train/ranker",
    sessions: [{ sid: "5e550006-0000-4000-8000-000000000006", title: "排序模型训练脚本", excerpt: "训练脚本加断点续训", agoMin: 40, messages: 18 }],
  },
  {
    origin: "<local>",
    path: "/home/user/work/ranker",
    agent: "codex",
    sessions: [{ sid: "019a0000-0000-7000-8000-00000000c0de", title: "", excerpt: "排序模型评测脚本", agoMin: 180, messages: 0 }],
  },
];

const projectDir = (path: string): string => path.replace(/[/\\:]/g, "-");
const baseName = (path: string): string => path.slice(path.lastIndexOf("/") + 1);
const historyPath = (p: Proj, s: string): string => `/home/user/.claude/projects/${projectDir(p.path)}/${s}.jsonl`;
const withOrigin = (origin: string): Record<string, unknown> => (origin === "<local>" ? {} : { origin });

export function historyOps(): Record<string, OpHandler> {
  return {
    // 平铺清单（`history-list`，形状照 `history-list.golden.json`）：本机后端答各台（`origin` 在请求里）。
    "history-list": (_o, req, w) => {
      const origin = (req.origin as string | undefined) ?? "<local>";
      if (w.historyDown?.includes(origin)) throw new Error(`连不上 ${origin}`);
      const q = typeof req.query === "string" ? req.query.toLowerCase() : "";
      const rows = HISTORY.filter((p) => p.origin === origin).flatMap((p) =>
        p.sessions
          .filter((s) => !q || `${s.title}\n${s.excerpt}\n${baseName(p.path)}`.toLowerCase().includes(q))
          .map((s) => {
            const status = s.live === true ? "live" : p.agent === "codex" ? "unknown" : "ended";
            return {
              agent: p.agent ?? "claude",
              agentTag: p.agent === "codex" ? "Codex" : null,
              sessionId: s.sid,
              projectDir: projectDir(p.path),
              projectPath: p.path,
              projectName: baseName(p.path),
              group: `${p.agent ?? "claude"}:${p.path}`,
              aiTitle: s.title || null,
              firstUserExcerpt: s.excerpt,
              title: s.title || s.excerpt || s.sid.slice(0, 8),
              label: s.title || s.excerpt || s.sid.slice(0, 8),
              untitled: !s.title && !s.excerpt,
              startedAt: ago(s.agoMin + 40),
              updatedAt: ago(s.agoMin),
              at: ago(s.agoMin),
              jsonlPath: historyPath(p, s.sid),
              messageCountApprox: s.messages,
              isBg: s.bg === true,
              starred: s.starred === true,
              customTitle: null,
              hidden: false,
              ...(s.fork ? { forkedFromSessionId: s.fork, forkedFromMessageUuid: "m-1" } : {}),
              ...(s.lastAccount ? { lastAccount: s.lastAccount } : {}),
              status,
              can: {
                resume: status === "live" ? "switch" : s.bg === true ? "bg" : "yes",
                accounts: p.agent !== "codex",
                fork: p.agent !== "codex" && s.bg !== true,
                delete: status === "live" ? "live" : status === "unknown" ? "unsure" : "yes",
              },
              ...withOrigin(p.origin),
            };
          }),
      );
      rows.sort((a, b) => b.at - a.at);
      const groups = HISTORY.filter((p) => p.origin === origin).map((p) => {
        const mine = rows.filter((r) => r.projectPath === p.path);
        const live = mine.some((r) => r.status === "live");
        const last = Math.max(0, ...mine.map((r) => r.updatedAt));
        return {
          key: `${p.agent ?? "claude"}:${p.path}`,
          agent: p.agent ?? "claude",
          projectName: baseName(p.path),
          projectPath: p.path,
          projectDir: projectDir(p.path),
          count: mine.length,
          hasLive: live,
          starred: mine.some((r) => r.starred),
          lastActivity: last,
          order: (live ? 2 : 0) * 1e14 + (mine.some((r) => r.starred) ? 1e13 : 0) + last,
          failed: null,
          ...withOrigin(p.origin),
        };
      }).filter((g) => g.count > 0).sort((a, b) => b.order - a.order);
      return { rows, groups, total: rows.length, truncated: false, notice: null };
    },
    // `titles: true` ⇒ 只比标题与第一句（「按项目」那一路）；否则按内容命中。本机还有 Codex 的会话（不在内容搜索里）。
    "history-search": (origin, req) => ({
      unreadable: 0,
      skipped: origin === "<local>" && req.titles !== true ? ["Codex"] : [],
      lines: HISTORY.filter((p) => p.origin === origin).flatMap((p) =>
        p.sessions
          .filter((s) =>
            req.titles === true
              ? `${s.title}\n${s.excerpt}`.includes(String(req.query))
              : s.hits,
          )
          .map((s) =>
            JSON.stringify({
              agent: "claude",
              sessionId: s.sid,
              projectPath: p.path,
              projectName: baseName(p.path),
              jsonlPath: historyPath(p, s.sid),
              title: s.title,
              updatedAt: ago(s.agoMin),
              hitCount: req.titles === true ? 0 : s.hits!.length,
              hits: (req.titles === true ? [] : s.hits!).map((h, i) => ({ uuid: `${s.sid}-h${i}`, tsMs: ago(s.agoMin + 3 - i), kind: h.kind, before: h.before, matched: SEARCH_WORD, after: h.after })),
              hitsTruncated: false,
            }),
          ),
      ),
    }),
    "history-search-merge": (_o, req) => {
      const rows = [...(req.sessions as { updatedAt: number; hitCount: number }[])].sort((a, b) => b.updatedAt - a.updatedAt);
      return { totalHits: rows.reduce((n, r) => n + r.hitCount, 0), sessionCount: rows.length, truncated: false, sessions: rows };
    },
  };
}
