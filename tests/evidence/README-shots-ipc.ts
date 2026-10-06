/**
 * 用户 09-29「readme全面搞一下 … 看看有没有什么截图 … 突出这个app是干嘛的」—— README 截图的**页里替身后端**。
 *
 * `README-shots.ts` 把本文件注进 index.html / settings.html 的最前面（先于入口模块求值）：Tauri 官方的
 * `@tauri-apps/api/mocks` 接住全部 `invoke`，通道那一跳（`chan_call` / `chan_subscribe`）按后端成品的线型答夹具里的合成数据。
 * 回包的编码借 `test-support/chan-fake.ts`（与各判据同一份）；后端出的那几句话（健康 · 退出策略 · 足迹）一律取文案表。
 * 答不上的帧命令回「对端不认」（界面按旧后端处理），名字记进 `window.__SHOTS_UNANSWERED` 由跑者报出来。
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { copyText } from "../../src/frontend/ui/copy-table";
import { LS_KEYS } from "../../src/frontend/ui/local-storage";
import { LOCAL_MACHINE_KEY, MACHINE_FACETS, recordFacet } from "../../src/frontend/ui/settings/machine-status";
import { chanArgsJson, chanReply, linesReply, UNSUPPORTED, type ChanCallArgs } from "../test-support/chan-fake";
import FOOTPRINT_GOLDEN from "../__fixtures__/footprint-report.golden.json";
import {
  ACCOUNTS,
  ACTIVE_SID,
  BACKEND_PID,
  HISTORY,
  LOCAL,
  MACHINES,
  NOW_ISO,
  REMOTE,
  SEARCH_WORD,
  SESSIONS,
  type ShotProject,
  type ShotSession,
} from "./README-shots-fixture";

type Json = Record<string, unknown>;
const w = window as unknown as {
  __SHOTS_UNANSWERED: string[];
  __TAURI_INTERNALS__: { invoke(cmd: string, args?: unknown): Promise<unknown> };
};
w.__SHOTS_UNANSWERED = [];
const NOW = Date.parse(NOW_ISO);

// ─── 界面偏好（用户自己会留下的那几格）＋ 机器健康账本（测过、都通过） ───
try {
  localStorage.setItem(LS_KEYS.lastActiveSid, ACTIVE_SID);
  localStorage.setItem(LS_KEYS.tabBarWidth, "300");
  localStorage.setItem(LS_KEYS.cmdkHintSeen, "1");
} catch {
  /* 存不进也照拍，只是 tab 栏窄一点 */
}
for (const machine of [LOCAL_MACHINE_KEY, REMOTE.label])
  for (const facet of MACHINE_FACETS) recordFacet(machine, facet, { kind: "ok", at: NOW - 5 * 60_000 });

// ─── 会话流：每台机器一条 `session-lines` 订阅，交一批「活会话 ＋ 记录行 ＋ 红绿灯 ＋ 清单报完」 ───
const recordPath = (s: ShotSession): string => `${s.cwd}/.claude/${s.sid}.jsonl`;
const withOrigin = (origin: string): Json => (origin === LOCAL ? {} : { origin });

function sessionFrames(origin: string): { t: "frame"; seq: number; body: string }[] {
  const mine = SESSIONS.filter((s) => s.origin === origin);
  const frames: unknown[] = [
    ...mine.map((s) => ({ live: { session_id: s.sid, origin, kind: "interactive", attachable: true, cwd: s.cwd, name: null } })),
    { batch: "start" },
    ...mine.flatMap((s) =>
      s.records.map((message, seq) => ({ line: { session_id: s.sid, cwd: s.cwd, path: recordPath(s), seq, ...withOrigin(origin), message } })),
    ),
    { batch: "end" },
    ...mine.map((s) => ({ activity: { session_id: s.sid, status: s.status, waiting_for: null } })),
    { listed: { origin } },
  ];
  return frames.map((f, seq) => ({ t: "frame", seq, body: JSON.stringify(f) }));
}

const sessionAt = (path: unknown): ShotSession | undefined => SESSIONS.find((s) => recordPath(s) === path);

// ─── 历史 ───
const projectDir = (path: string): string => path.replace(/\//g, "-");
const baseName = (path: string): string => path.slice(path.lastIndexOf("/") + 1);
const minutesAgo = (min: number): number => NOW - min * 60_000;
const historyPath = (p: ShotProject, sid: string): string => `/home/dev/.claude/projects/${projectDir(p.path)}/${sid}.jsonl`;

function searchHits(p: ShotProject): Json[] {
  return p.sessions.flatMap((s) =>
    s.hits
      ? [
          {
            sessionId: s.sid,
            projectPath: p.path,
            projectName: baseName(p.path),
            jsonlPath: historyPath(p, s.sid),
            title: s.title,
            updatedAt: minutesAgo(s.agoMin),
            hitCount: s.hits.length,
            hits: s.hits.map((h, i) => ({ uuid: `${s.sid}-h${i}`, tsMs: minutesAgo(s.agoMin + 3 - i), kind: h.kind, before: h.before, matched: SEARCH_WORD, after: h.after })),
            hitsTruncated: false,
          },
        ]
      : [],
  );
}

/**
 * 足迹：行的**结构与文字**取后端现产的跨语言金样（`tests/__fixtures__/footprint-report.golden.json`，每一句都是文案表里的成品），
 * 家目录换成夹具那台；这台上装着的改成「在」（在的那句话同样取文案表）；旧版遗留那几行后端本来就答「该不在、确实不在」、
 * 项目目录 / 由配置决定的那几行后端本来就答「未确定」—— 这两类原样留着。
 */
function footprint(): unknown {
  const g = JSON.parse(JSON.stringify(FOOTPRINT_GOLDEN).split("/nonexistent-footprint-golden").join("/home/dev")) as {
    rows: { path_declared: string; tier: string; host_label: string; state: { kind: string; detail?: string } }[];
    settings_scopes: { state: unknown; has_cc_bus_hooks: boolean | null }[];
  };
  const sizeOf = (path: string): number => 900 + ([...path].reduce((n, c) => n * 31 + c.charCodeAt(0), 7) % 38_000);
  for (const r of g.rows) {
    const p = r.path_declared;
    if (r.state.kind === "expected_absent") continue;
    if (p.startsWith("$") || r.host_label === copyText("rsConfigSurface.host.projectDir")) continue;
    const seen =
      r.tier === "UserInstallsWePrompt"
        ? copyText("rsConfigSurface.onPath.present", { named: p })
        : p.endsWith("*")
          ? copyText("rsConfigSurface.observe.globCount", { count: 3 })
          : p.endsWith("/") || p.includes("/skills")
            ? copyText("rsConfigSurface.observe.dirCount", { count: 3 + (sizeOf(p) % 12) })
            : copyText("rsConfigSurface.observe.fileSize", { bytes: sizeOf(p) });
    const either = r.host_label === copyText("rsConfigSurface.host.either") && r.tier !== "UserInstallsWePrompt";
    r.state = { kind: "present", detail: either ? copyText("rsConfigSurface.observe.eitherPresent", { detail: seen }) : seen };
  }
  g.settings_scopes[0].state = { kind: "present", detail: copyText("rsConfigSurface.observe.fileSize", { bytes: 2140 }) };
  g.settings_scopes[0].has_cc_bus_hooks = true;
  return g;
}

// ─── 帧命令表：op ⇒ 成品（线型照各读面的解码器 / 跨语言金样） ───
const acctDir = (name: string): string => `/home/dev/.cc-accts/${name}`;
const OPS: Record<string, (origin: string, body: Json) => unknown> = {
  "accounts-list": () => ({
    meta: { enabled: true, acctsDir: "/home/dev/.cc-accts", manifestPath: "/home/dev/.cc-accts/accounts.json", updatedAt: "2026-09-20T02:00:00Z", sharedStore: null, count: ACCOUNTS.length, error: null, unsupported: null },
    accounts: ACCOUNTS.map((a, i) => ({ ...a, configDir: acctDir(a.name), isDefault: i === 0, mode: "isolated", exists: true, loggedIn: true, authKind: "subscription", authReady: true })),
    notice: null,
  }),
  "accounts-sessions": (origin) =>
    SESSIONS.filter((s) => s.origin === origin).map((s, i) => ({ pid: 4100 + i, sessionId: s.sid, cwd: s.cwd, configDir: acctDir(ACCOUNTS[0].name), account: ACCOUNTS[0].name, bare: false, alive: true, launchId: null, viaRelay: null })),
  "history-last-accounts": () => ({ accounts: Object.fromEntries(SESSIONS.map((s) => [s.sid, ACCOUNTS[0].name])) }),
  "tasks-list": () => ({ tasks: [] }),
  "history-facts": (_o, body) => {
    const last = [...(sessionAt(body.path)?.records ?? [])].reverse().find((r) => r.type === "assistant");
    const usage = last?.type === "assistant" && last.message.usage ? { model: last.message.model, promptTokens: last.message.usage.input_tokens } : null;
    return { agents: [], end: 4096, forkedFrom: null, touchedFiles: [], usage };
  },
  "history-user-inputs": (_o, body) => ({
    from: 0,
    end: 4096,
    entries: (sessionAt(body.path)?.records ?? []).flatMap((r) =>
      r.type === "user" && r.userText.speaker.kind === "human" && r.userText.text ? [{ uuid: r.uuid, excerpt: r.userText.text.slice(0, 80), timestamp: r.timestamp }] : [],
    ),
  }),
  "history-search": (origin) => HISTORY.filter((p) => p.origin === origin).flatMap(searchHits),
  "history-search-merge": (_o, body) => {
    const rows = [...(body.sessions as { updatedAt: number; hitCount: number }[])].sort((a, b) => b.updatedAt - a.updatedAt);
    return { totalHits: rows.reduce((n, r) => n + r.hitCount, 0), sessionCount: rows.length, truncated: false, sessions: rows };
  },
  "exit-policy-read": () => ({ state: "absent", killOnExit: false, reason: null, path: null, said: copyText("backendPolicy.exit.unattended") }),
  "ssh-config-aliases": () => ({ aliases: [REMOTE.label] }),
  "footprint-report": () => footprint(),
};
/** 按行那一族（`{"lines": [...]}`）。 */
const LINE_OPS = new Set(["accounts-sessions", "history-search"]);

function chanCall(a: ChanCallArgs): unknown {
  const answer = OPS[a.op]?.(a.origin, chanArgsJson(a) as Json);
  if (answer === undefined) {
    w.__SHOTS_UNANSWERED.push(`${a.origin} ${a.op}`);
    return Promise.reject(UNSUPPORTED);
  }
  return LINE_OPS.has(a.op) ? linesReply(answer as unknown[]) : chanReply(answer);
}

// ─── Tauri 命令 ───
const emit = (event: string, payload: unknown): void => {
  void w.__TAURI_INTERNALS__.invoke("plugin:event|emit", { event, payload });
};

mockWindows(location.pathname.includes("settings") ? "settings" : "main");
mockIPC(
  (cmd, raw) => {
    const args = (raw ?? {}) as Json;
    switch (cmd) {
      case "chan_call":
        return chanCall(args as unknown as ChanCallArgs);
      case "chan_subscribe":
        // 登记那一跳先回来（`chan.subscribe` 要先拿到句柄），格随后到
        if (args.kind === "session-lines") setTimeout(() => emit("chan-items", { sub: args.id, items: sessionFrames(String(args.origin)) }), 30);
        return null;
      case "load_config":
        return {
          remote: { enabled: true, hosts: [{ label: REMOTE.label, host: REMOTE.host, user: REMOTE.user, port: REMOTE.port }] },
          accounts: { defaultName: ACCOUNTS[0].name },
        };
      case "backend_machines":
        return [...MACHINES];
      case "list_remote_mcp_origins":
        return [REMOTE.label];
      case "backend_status":
        return {
          channel: true,
          pid: BACKEND_PID[String(args.origin)],
          health: {
            state: "clean",
            summary: copyText("rsBackendPolicy.health.clean"),
            why: null,
            detail: copyText("rsBackendPolicy.health.detail", { crashed: 0, refused: 0, neverStarted: 0, misread: 0 }),
          },
        };
      default:
        return null; // 窗口 / 开目录 / 落日志那一类：没有要画的回包
    }
  },
  { shouldMockEvents: true },
);
