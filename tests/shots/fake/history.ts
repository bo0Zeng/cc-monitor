/**
 * 历史页的合成数据：几台机器上的项目与会话、全文搜索的命中。形状照 `history-products.golden.json` 与界面的解码器。
 */
import type { OpHandler } from "./types";

const NOW = Date.parse("2026-10-01T12:00:00Z");
const ago = (min: number): number => NOW - min * 60_000;

interface Proj {
  origin: string;
  path: string;
  sessions: { sid: string; title: string; excerpt: string; agoMin: number; messages: number; live?: boolean; starred?: boolean; bg?: boolean; hits?: { kind: string; before: string; after: string }[] }[];
}

const sid = (n: number): string => `0000${n.toString(16).padStart(4, "0")}-0000-4000-8000-0000000000${(n % 100).toString().padStart(2, "0")}`;

export const SEARCH_WORD = "重试";

export const HISTORY: Proj[] = [
  {
    origin: "<local>",
    path: "/home/user/work/orders",
    sessions: [
      { sid: "5e550001-0000-4000-8000-000000000001", title: "给订单服务加重试与超时", excerpt: "订单服务调用库存接口时偶尔超时…", agoMin: 3, messages: 42, live: true, hits: [{ kind: "user", before: "帮我加上", after: "（指数退避）和整体超时" }, { kind: "assistant", before: "统一做", after: " ＋ 退避 ＋ 整体超时" }] },
      { sid: sid(11), title: "支付回调验签", excerpt: "支付回调偶尔验签失败，查一下", agoMin: 60 * 26, messages: 88, starred: true },
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
      { sid: sid(41), title: "发票号生成冲突", excerpt: "并发开票时发票号重复", agoMin: 60 * 30, messages: 67 },
    ],
  },
  {
    origin: "gpu-01",
    path: "/data/train/ranker",
    sessions: [{ sid: "5e550006-0000-4000-8000-000000000006", title: "排序模型训练脚本", excerpt: "训练脚本加断点续训", agoMin: 40, messages: 18 }],
  },
];

const projectDir = (path: string): string => path.replace(/[/\\:]/g, "-");
const baseName = (path: string): string => path.slice(path.lastIndexOf("/") + 1);
const historyPath = (p: Proj, s: string): string => `/home/user/.claude/projects/${projectDir(p.path)}/${s}.jsonl`;
const withOrigin = (origin: string): Record<string, unknown> => (origin === "<local>" ? {} : { origin });
const projectsOn = (origin: unknown): Proj[] => HISTORY.filter((p) => p.origin === ((origin as string | undefined) ?? "<local>"));

export function historyOps(): Record<string, OpHandler> {
  return {
    "history-projects": (_o, req) => ({
      rows: projectsOn(req.origin).map((p) => ({
        projectPath: p.path,
        projectName: baseName(p.path),
        projectDir: projectDir(p.path),
        sessionCount: p.sessions.length,
        starredCount: p.sessions.filter((s) => s.starred).length,
        hiddenCount: 0,
        lastActivity: Math.max(...p.sessions.map((s) => ago(s.agoMin))),
        hasLive: p.sessions.some((s) => s.live === true),
        ...withOrigin(p.origin),
      })),
      notice: null,
    }),
    "history-sessions": (_o, req) => {
      const p = projectsOn(req.origin).find((x) => projectDir(x.path) === req.project_dir);
      if (!p) return { rows: [], notice: null };
      return {
        rows: p.sessions.map((s) => ({
          sessionId: s.sid,
          projectPath: p.path,
          projectName: baseName(p.path),
          aiTitle: s.title,
          firstUserExcerpt: s.excerpt,
          startedAt: ago(s.agoMin + 40),
          updatedAt: ago(s.agoMin),
          jsonlPath: historyPath(p, s.sid),
          isLive: s.live === true,
          messageCountApprox: s.messages,
          isBg: s.bg === true,
          starred: s.starred === true,
          customTitle: null,
          hidden: false,
          ...withOrigin(p.origin),
        })),
        notice: null,
      };
    },
    "history-search": (origin) => ({
      lines: HISTORY.filter((p) => p.origin === origin).flatMap((p) =>
        p.sessions
          .filter((s) => s.hits)
          .map((s) =>
            JSON.stringify({
              sessionId: s.sid,
              projectPath: p.path,
              projectName: baseName(p.path),
              jsonlPath: historyPath(p, s.sid),
              title: s.title,
              updatedAt: ago(s.agoMin),
              hitCount: s.hits!.length,
              hits: s.hits!.map((h, i) => ({ uuid: `${s.sid}-h${i}`, tsMs: ago(s.agoMin + 3 - i), kind: h.kind, before: h.before, matched: SEARCH_WORD, after: h.after })),
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
