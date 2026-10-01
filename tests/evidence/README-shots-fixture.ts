/**
 * 用户 09-29「readme全面搞一下 … 看看有没有什么截图 … 突出这个app是干嘛的」—— README 截图（`README-shots.ts`）用的**合成**数据。
 *
 * 全部是编的：机器 · 仓 · 对话 · 账号 · 历史 · 全景图都不来自任何真会话（纪律：测试夹具采结构不采内容）。
 * 形状照后端成品（`generated/*.ts` 的线型 ＋ 各读面解码器收的键），内容照一次普通的开发对话写。
 */
import type { JsonlRecord } from "../../src/frontend/ui/generated/JsonlRecord";

/** 远端那一台（设置里的机器名 ＝ 会话流里的 origin）。 */
export const REMOTE = { label: "devbox", host: "devbox.lan", user: "dev", port: 22 } as const;
export const LOCAL = "<local>";
export const MACHINES = [LOCAL, REMOTE.label] as const;

/** 当天的钟面：所有时间戳都从这里往后排（UTC；截图钉 `Asia/Shanghai` ⇒ 下午两点多）。 */
const T0 = Date.parse("2026-09-29T06:10:00.000Z");
const at = (sec: number): string => new Date(T0 + sec * 1000).toISOString();

export interface ShotSession {
  sid: string;
  origin: string;
  cwd: string;
  title: string;
  /** 会话红绿灯（`activity` 格）。 */
  status: "busy" | "idle";
  records: JsonlRecord[];
}

const MODEL = "claude-sonnet-4-5";
const usage = (input: number, output: number) => ({
  input_tokens: input,
  cache_creation_input_tokens: 0,
  cache_read_input_tokens: 0,
  output_tokens: output,
});

/** 一条用户话（正文 ＝ 成品 `userText.clean`，夹具没有注入噪声）。 */
function user(sid: string, uuid: string, sec: number, text: string, cwd: string): JsonlRecord {
  return {
    type: "user",
    uuid,
    timestamp: at(sec),
    message: { role: "user", content: text, model: null, usage: null },
    cwd,
    sessionId: sid,
    isMeta: false,
    parentUuid: null,
    forkedFrom: null,
    userText: { clean: text, interrupt: false },
  };
}

/** 工具结果（user 记录里的 `tool_result` 块）。 */
function result(sid: string, uuid: string, sec: number, toolUseId: string, content: string, cwd: string): JsonlRecord {
  return {
    type: "user",
    uuid,
    timestamp: at(sec),
    message: { role: "user", content: [{ type: "tool_result", tool_use_id: toolUseId, content }], model: null, usage: null },
    cwd,
    sessionId: sid,
    isMeta: false,
    parentUuid: null,
    forkedFrom: null,
    userText: { clean: "", interrupt: false },
  };
}

function assistant(
  sid: string,
  uuid: string,
  sec: number,
  content: unknown[],
  tokens: number,
  toolCards?: Record<string, "diff">,
): JsonlRecord {
  return {
    type: "assistant",
    uuid,
    timestamp: at(sec),
    message: { role: "assistant", content, model: MODEL, usage: usage(tokens, 400) },
    sessionId: sid,
    requestId: null,
    parentUuid: null,
    forkedFrom: null,
    isApiErrorMessage: false,
    error: null,
    apiErrorStatus: null,
    ...(toolCards ? { toolCards } : {}),
  };
}

const title = (sid: string, t: string): JsonlRecord => ({ type: "ai-title", aiTitle: t, sessionId: sid });

/** 串成一条主线（每条的 `parentUuid` 指前一条）；标题记录排在末尾（CLI 也是聊过一轮才写它）。 */
function chain(sid: string, t: string, recs: JsonlRecord[]): JsonlRecord[] {
  let prev: string | null = null;
  const out = recs.map((r) => {
    if (r.type !== "user" && r.type !== "assistant") return r;
    const linked = { ...r, parentUuid: prev };
    prev = r.uuid;
    return linked;
  });
  return [...out, title(sid, t)];
}

// ─── 主角：本机 api-server，一次完整的「提需求 → 读 → 改 → 测 → 总结」 ───

const API = "7c1e4a52-3f0b-4d8e-9a61-2b5c8d0e4f13";
const API_CWD = "/home/dev/projects/api-server";
const ORDERS = `${API_CWD}/src/routes/orders.ts`;

const ORDERS_BEFORE = [
  'import { Router } from "express";',
  'import { requireUser } from "../middleware/auth";',
  'import { listOrders } from "../services/orderService";',
  'import { sendJson } from "../lib/http";',
  "",
  "export const orders = Router();",
  "",
  'orders.get("/", async function getOrders(req, res) {',
  "  await requireUser(req);",
  "  const rows = await listOrders();",
  "  sendJson(res, { items: rows });",
  "});",
];

const OLD_HANDLER = ["  const rows = await listOrders();", "  sendJson(res, { items: rows });"].join("\n");

const NEW_HANDLER = [
  "  const { page, size } = parsePage(req.query, { size: 20, max: 100 });",
  "  const rows = await listOrders({ skip: (page - 1) * size, take: size });",
  "  sendJson(res, { items: rows, page, page_size: size });",
].join("\n");

const TEST_OUT = [
  "> api-server@1.4.0 test",
  "> vitest run orders",
  "",
  " ✓ tests/orders.test.ts (6 tests) 412ms",
  "",
  " Test Files  1 passed (1)",
  "      Tests  6 passed (6)",
  "   Duration  1.38s",
].join("\n");

const SUMMARY = [
  "分页加好了，订单相关的 6 个测试全部通过。",
  "",
  "- `GET /orders` 支持 `page`（默认 1）和 `page_size`（默认 20，上限 100），非法值回落到默认值",
  "- 复用 `lib/paging.ts` 里的 `parsePage`，把 `skip` / `take` 传给 `listOrders`，不再一次读全表",
].join("\n");

const apiRecords: JsonlRecord[] = chain(API, "订单列表加分页", [
  user(
    API,
    "a1",
    0,
    "GET /orders 现在一次把全部订单都返回，数据一多就很慢。加上分页：page 和 page_size 两个参数，默认每页 20 条、最多 100 条。改完跑一下订单相关的测试。",
    API_CWD,
  ),
  assistant(
    API,
    "a2",
    8,
    [
      { type: "text", text: "先看一下现在的路由是怎么查的。" },
      { type: "tool_use", id: "toolu_read1", name: "Read", input: { file_path: ORDERS } },
    ],
    18_400,
  ),
  result(API, "a3", 9, "toolu_read1", ORDERS_BEFORE.map((l, i) => `${String(i + 1).padStart(6)}\t${l}`).join("\n"), API_CWD),
  assistant(
    API,
    "a4",
    52,
    [
      { type: "text", text: "现在是整表查询。`lib/paging.ts` 里已经有 `parsePage`，直接用它，把 `skip` / `take` 传给 `listOrders`。" },
      { type: "tool_use", id: "toolu_edit1", name: "Edit", input: { file_path: ORDERS, old_string: OLD_HANDLER, new_string: NEW_HANDLER } },
    ],
    21_900,
    { toolu_edit1: "diff" },
  ),
  result(API, "a5", 53, "toolu_edit1", `The file ${ORDERS} has been updated successfully.`, API_CWD),
  assistant(
    API,
    "a6",
    71,
    [
      { type: "text", text: "改好了，跑一下订单相关的测试。" },
      { type: "tool_use", id: "toolu_bash1", name: "Bash", input: { command: "npm test -- orders", description: "运行订单路由的测试" } },
    ],
    24_300,
  ),
  result(API, "a7", 76, "toolu_bash1", TEST_OUT, API_CWD),
  assistant(API, "a8", 104, [{ type: "text", text: SUMMARY }], 25_100),
]);

// ─── 配角：另外三个 tab（各一两轮，只为 tab 栏像样） ───

function short(sid: string, cwd: string, t: string, ask: string, reply: string, base: number): JsonlRecord[] {
  return chain(sid, t, [
    user(sid, `${sid.slice(0, 4)}-1`, base, ask, cwd),
    assistant(sid, `${sid.slice(0, 4)}-2`, base + 9, [{ type: "text", text: reply }], 12_000),
  ]);
}

const WEB = "b52d9e07-81c4-4f3a-b6d2-9e04a7c1f588";
const INFRA = "c93f0a18-6d27-4b1e-8c45-07e2b9d4a6c1";
const WORKER = "d0a47b3c-2e91-4c6d-a8f3-5b1e9c7d2e40";

export const SESSIONS: ShotSession[] = [
  { sid: API, origin: LOCAL, cwd: API_CWD, title: "订单列表加分页", status: "idle", records: apiRecords },
  {
    sid: WEB,
    origin: LOCAL,
    cwd: "/home/dev/projects/web-app",
    title: "登录表单校验",
    status: "busy",
    records: short(
      WEB,
      "/home/dev/projects/web-app",
      "登录表单校验",
      "登录页的邮箱输入框没有格式校验，提交空表单也能发请求。加上前端校验，错误提示放在输入框下方。",
      "好的，我先看一下 `LoginForm.tsx` 和现有的表单组件。",
      -240,
    ),
  },
  {
    sid: INFRA,
    origin: REMOTE.label,
    cwd: "/srv/infra",
    title: "开启 gzip 压缩",
    status: "idle",
    records: short(
      INFRA,
      "/srv/infra",
      "开启 gzip 压缩",
      "给 nginx/site.conf 打开 gzip，只压缩文本类型，改完用 nginx -t 检查一下配置。",
      "已加上 `gzip on` 和 `gzip_types`，`nginx -t` 通过，可以 reload 了。",
      -900,
    ),
  },
  {
    sid: WORKER,
    origin: REMOTE.label,
    cwd: "/srv/jobs",
    title: "重试改指数退避",
    status: "busy",
    records: short(
      WORKER,
      "/srv/jobs",
      "重试改指数退避",
      "扣款失败的重试现在是固定 5 秒一次，改成指数退避，最多重试 6 次。",
      "我先看一下 `queue/retry.py` 里的调度逻辑。",
      -60,
    ),
  },
];

/** 主截图里停在哪个 tab。 */
export const ACTIVE_SID = API;

// ─── 历史：两台机器上的项目与会话，外加一次全文搜索「缓存」的命中 ───

export interface ShotHit {
  kind: "user" | "assistant";
  before: string;
  after: string;
}
export interface ShotHistorySession {
  sid: string;
  title: string;
  excerpt: string;
  /** 距「现在」多少分钟之前最后一次活动。 */
  agoMin: number;
  messages: number;
  live?: boolean;
  hits?: ShotHit[];
}
export interface ShotProject {
  origin: string;
  path: string;
  sessions: ShotHistorySession[];
}

/** 截图钉的「现在」（页里的钟停在这一刻）。 */
export const NOW_ISO = at(150);
export const SEARCH_WORD = "缓存";

const hid = (n: number): string => `e${String(n).padStart(7, "0")}-5a1c-4b2e-9d3f-${String(n * 7919).padStart(12, "0")}`;

export const HISTORY: ShotProject[] = [
  {
    origin: LOCAL,
    path: API_CWD,
    sessions: [
      { sid: API, title: "订单列表加分页", excerpt: "GET /orders 现在一次把全部订单都返回", agoMin: 1, messages: 9, live: true },
      {
        sid: hid(1),
        title: "商品详情加缓存",
        excerpt: "商品详情接口每次都查库",
        agoMin: 190,
        messages: 42,
        hits: [
          { kind: "user", before: "商品详情接口每次都查库，加一层 Redis ", after: "，过期时间 5 分钟。" },
          { kind: "assistant", before: "读路径先查", after: "，没命中再查库并回填；更新商品时主动删掉对应的键。" },
          { kind: "assistant", before: "压测对比：加", after: "之后 p95 从 180ms 降到 22ms。" },
        ],
      },
      { sid: hid(2), title: "退款金额精度", excerpt: "部分退款算出来差一分钱", agoMin: 60 * 26, messages: 31 },
      { sid: hid(3), title: "升级 express 到 5", excerpt: "express 升到 5.x，看看哪些中间件要改", agoMin: 60 * 72, messages: 57 },
      {
        sid: hid(9),
        title: "会话存储迁到 Redis",
        excerpt: "用户会话现在放在进程内存里",
        agoMin: 60 * 27,
        messages: 35,
        hits: [
          { kind: "user", before: "用户会话现在放在进程内存里，一重启就掉登录。挪到 Redis，本地那份", after: "也一起删掉。" },
          { kind: "assistant", before: "进程内的会话", after: "已经删掉，改成 Redis 存储，滑动过期 7 天。" },
        ],
      },
    ],
  },
  {
    origin: LOCAL,
    path: "/home/dev/projects/web-app",
    sessions: [
      { sid: WEB, title: "登录表单校验", excerpt: "登录页的邮箱输入框没有格式校验", agoMin: 5, messages: 3, live: true },
      {
        sid: hid(4),
        title: "首页图片懒加载",
        excerpt: "首页图片太多，首屏加载慢",
        agoMin: 60 * 5,
        messages: 24,
        hits: [{ kind: "assistant", before: "图片走 CDN，浏览器会按响应头", after: "，这次只改加载时机，不动图片地址。" }],
      },
      { sid: hid(5), title: "暗色模式切换", excerpt: "加一个跟随系统的暗色模式", agoMin: 60 * 49, messages: 38 },
      {
        sid: hid(10),
        title: "构建产物瘦身",
        excerpt: "打包出来的 main.js 有 2.1 MB",
        agoMin: 60 * 50,
        messages: 27,
        hits: [{ kind: "assistant", before: "把第三方库单独拆成 vendor 包，它们很少变，浏览器", after: "的命中率会高很多。" }],
      },
    ],
  },
  {
    origin: REMOTE.label,
    path: "/srv/infra",
    sessions: [
      { sid: INFRA, title: "开启 gzip 压缩", excerpt: "给 nginx/site.conf 打开 gzip", agoMin: 16, messages: 3, live: true },
      {
        sid: hid(6),
        title: "静态资源缓存头",
        excerpt: "给 /static 下的文件加上 Cache-Control",
        agoMin: 60 * 3,
        messages: 18,
        hits: [
          { kind: "user", before: "给 /static 下的文件加上 Cache-Control，让浏览器", after: " 30 天。" },
          { kind: "assistant", before: "文件名带了内容哈希，长期", after: "是安全的；index.html 单独设成 no-cache。" },
        ],
      },
      { sid: hid(7), title: "证书自动续期", excerpt: "证书下个月到期，改成自动续期", agoMin: 60 * 30, messages: 22 },
    ],
  },
  {
    origin: REMOTE.label,
    path: "/srv/jobs",
    sessions: [
      { sid: WORKER, title: "重试改指数退避", excerpt: "扣款失败的重试现在是固定 5 秒一次", agoMin: 2, messages: 3, live: true },
      {
        sid: hid(8),
        title: "日报任务提速",
        excerpt: "日报任务每次都重算全部数据",
        agoMin: 60 * 8,
        messages: 29,
        hits: [
          { kind: "user", before: "日报任务每次都重算全部数据，把中间结果", after: "到磁盘上。" },
          { kind: "assistant", before: "按日期分片", after: "，只重算当天那一片；旧分片带校验和，源数据变了就作废。" },
        ],
      },
    ],
  },
];

// ─── 代码全景：api-server 这个小仓（六个目录）＋ 以刚改过的路由为中心的调用子图（几条确定的边，一条派发） ───

export interface ShotModule {
  dir: string;
  files: { name: string; symbols: number }[];
}
export const PANO_MODULES: ShotModule[] = [
  { dir: "src/routes", files: [{ name: "index.ts", symbols: 3 }, { name: "orders.ts", symbols: 9 }, { name: "products.ts", symbols: 11 }, { name: "users.ts", symbols: 8 }] },
  { dir: "src/services", files: [{ name: "orderService.ts", symbols: 14 }, { name: "orderDto.ts", symbols: 4 }, { name: "productCache.ts", symbols: 9 }, { name: "billing.ts", symbols: 12 }] },
  { dir: "src/repo", files: [{ name: "client.ts", symbols: 5 }, { name: "orderRepo.ts", symbols: 7 }, { name: "productRepo.ts", symbols: 6 }] },
  { dir: "src/middleware", files: [{ name: "auth.ts", symbols: 6 }, { name: "session.ts", symbols: 5 }, { name: "rateLimit.ts", symbols: 4 }] },
  { dir: "src/lib", files: [{ name: "http.ts", symbols: 6 }, { name: "paging.ts", symbols: 3 }, { name: "logger.ts", symbols: 5 }, { name: "config.ts", symbols: 4 }] },
  { dir: "src/jobs", files: [{ name: "dailyReport.ts", symbols: 6 }, { name: "retryQueue.ts", symbols: 8 }] },
];
/** 索引读数（全景打开时的覆盖信号）：这个小仓没有解析不了的调用 ⇒ 顶上不挂「覆盖不全」。 */
export const PANO_UNRESOLVED = 0;

/** 调用子图：以刚改过的那个路由处理函数为中心（`[id, 行号]`；id ＝ `文件#名字`）。 */
export const PANO_CENTER = "src/routes/orders.ts#getOrders";
export const PANO_SYMBOLS: [string, number][] = [
  ["src/routes/index.ts#mountRoutes", 8],
  [PANO_CENTER, 7],
  ["src/middleware/auth.ts#requireUser", 11],
  ["src/lib/paging.ts#parsePage", 4],
  ["src/services/orderService.ts#listOrders", 21],
  ["src/lib/http.ts#sendJson", 9],
  ["src/middleware/session.ts#loadSession", 17],
  ["src/lib/paging.ts#clampInt", 15],
  ["src/repo/orderRepo.ts#countOrders", 6],
  ["src/repo/orderRepo.ts#findOrders", 14],
  ["src/services/orderDto.ts#toOrderDto", 3],
  ["src/lib/http.ts#logResponse", 22],
];
/** `[调用方, 被调方, 可信度]`。 */
export const PANO_CALLS: [string, string, "Exact" | "Dispatch"][] = [
  ["src/routes/index.ts#mountRoutes", PANO_CENTER, "Exact"],
  [PANO_CENTER, "src/middleware/auth.ts#requireUser", "Exact"],
  [PANO_CENTER, "src/lib/paging.ts#parsePage", "Exact"],
  [PANO_CENTER, "src/services/orderService.ts#listOrders", "Exact"],
  [PANO_CENTER, "src/lib/http.ts#sendJson", "Exact"],
  ["src/middleware/auth.ts#requireUser", "src/middleware/session.ts#loadSession", "Exact"],
  ["src/lib/paging.ts#parsePage", "src/lib/paging.ts#clampInt", "Exact"],
  ["src/services/orderService.ts#listOrders", "src/repo/orderRepo.ts#countOrders", "Exact"],
  ["src/services/orderService.ts#listOrders", "src/repo/orderRepo.ts#findOrders", "Exact"],
  ["src/services/orderService.ts#listOrders", "src/services/orderDto.ts#toOrderDto", "Dispatch"],
  ["src/lib/http.ts#sendJson", "src/lib/http.ts#logResponse", "Exact"],
];
/** 调用子图下面那行完整度读数（这种图不认「排除测试」⇒ 那一格是「不适用」）。 */
export const PANO_CALLS_HONESTY = {
  unresolved_calls: 0,
  ambiguous_calls: 0,
  filtered_guess_links: 0,
  excluded_test_symbols: null,
  omitted: null,
  db_errors: [] as string[],
};

// ─── 账号与机器健康 ───

/** 两台机器上登记的账号（同一份：cc-acct-iso 的账号库在两台都装着）。第一个是默认。 */
export const ACCOUNTS = [
  { name: "work", email: "dev@example.com" },
  { name: "personal", email: "me@example.org" },
];
/** 两台后端各自的进程号（设置 → 机器里「已连上（pid …）」那一格）。 */
export const BACKEND_PID: Record<string, number> = { [LOCAL]: 2817, [REMOTE.label]: 40213 };
