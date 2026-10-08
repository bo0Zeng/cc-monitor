/**
 * 机器与设置那一族的合成答法：后端状态、ccm 入口、ssh 配置、tmux、足迹、数据位置、日志 …
 * 后端说的那几句话一律取文案表（与真后端出的成品同一份字），形状照各跨语言金样。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import FOOTPRINT_GOLDEN from "../../__fixtures__/footprint-report.golden.json";
import EXT_GOLDEN from "../../__fixtures__/ext-flow.golden.json";
import type { CommandHandler } from "./commands";
import type { OpHandler, World } from "./types";

const HOME = "/home/user";

const pidOf = (origin: string, w: World): number => 3200 + w.machines.indexOf(origin) * 17;

/** 足迹：金样的家目录换成合成那台；用户装过的那几行改成「在」。 */
function footprint(): unknown {
  const g = JSON.parse(JSON.stringify(FOOTPRINT_GOLDEN).split("/nonexistent-footprint-golden").join(HOME)) as {
    rows: { path_declared: string; tier: string; host_label: string; state: { kind: string; detail?: string } }[];
    settings_scopes: { state: unknown; has_cc_bus_hooks: boolean | null }[];
  };
  for (const r of g.rows) {
    const p = r.path_declared;
    if (r.state.kind === "expected_absent") continue;
    if (p.startsWith("$") || r.host_label === copyText("rsConfigSurface.host.projectDir")) continue;
    const seen = p.endsWith("/") ? copyText("rsConfigSurface.observe.dirCount", { count: 6 }) : copyText("rsConfigSurface.observe.fileSize", { bytes: 2048 + p.length * 37 });
    r.state = { kind: "present", detail: seen };
  }
  if (g.settings_scopes[0]) {
    g.settings_scopes[0].state = { kind: "present", detail: copyText("rsConfigSurface.observe.fileSize", { bytes: 2140 }) };
    g.settings_scopes[0].has_cc_bus_hooks = true;
  }
  return g;
}

/** 扩展页那张表：金样的临时目录换成合成那台的家，两台机器换成本机与 devbox。 */
function extList(): unknown {
  const g = JSON.parse(JSON.stringify(EXT_GOLDEN.list).split("<BASE>/a").join(HOME).split("<BASE>/b").join("/home/dev")) as {
    machines: { name: string; key: string | null }[];
  };
  g.machines[0].name = "workstation";
  g.machines[1].name = "devbox";
  g.machines[1].key = "devbox";
  // 再两台：gpu-01 连不上（那一列是上次同步的样子）· win-laptop；外加一条 MCP「github」，本机装了、devbox 内容不同、另两台没装。
  const rows = (g as unknown as { rows: { cells: unknown[] }[] }).rows;
  const more = [
    { here: false, key: "gpu-01", name: "gpu-01", projects: [], reachable: false },
    { here: false, key: "win-laptop", name: "win-laptop", projects: [], reachable: true },
  ];
  g.machines.push(...(more as never[]));
  const missingCell = (bring: unknown) => ({ bring, note: null, places: [{ at: { level: "user" }, dir: null, note: null, state: "missing", uninstall: false }], state: "missing" });
  for (const r of rows) r.cells.push(missingCell(null), missingCell(null));
  const bring = { from: null, fromName: "本机", scope: { from: { level: "user" }, to: { level: "user" } }, targets: [{ at: { level: "user" }, note: null, ok: true }] };
  rows.push({
    about: "读写 GitHub 上的仓库与 issue",
    builtin: null,
    cells: [
      { bring: null, note: null, places: [{ at: { level: "user" }, dir: null, note: null, state: "same", uninstall: true }], state: "same" },
      { bring, note: null, places: [{ at: { level: "user" }, dir: null, note: null, state: "differs", uninstall: true }], state: "differs" },
      missingCell(bring),
      missingCell(bring),
    ],
    detail: [],
    kind: "mcp",
    name: "github",
    new: false,
    note: null,
  } as never);
  return g;
}

export function machineOps(): Record<string, OpHandler> {
  return {
    "ext-list": () => extList(),
    // 「装到 N 台」那一张卡：每台一张（MCP 那一条要一个 token，win-laptop 上已经有了）。
    "ext-hub-preview": (_o, req) => ({
      kind: req.kind,
      name: req.name,
      path: req.to === "win-laptop" ? "C:\\Users\\user\\.claude.json" : `${HOME}/.claude.json`,
      writes: [".claude.json · mcpServers"],
      unchanged: false,
      suspects: [],
      stop: null,
      config: null,
      slots: [{ field: "env", key: "GITHUB_TOKEN", kept: req.to === "win-laptop" }],
      tokens: { source: "s1", target: "t1" },
    }),
    // 停 / 重启 / 更新 / 卸载之前「会打断什么」（形状同 `tests/__fixtures__/machine-interrupts.golden.json`）：
    //   devbox 上两个会话的请求经它、一个活着；本机账上通往 devbox 的转发一条；别的台什么都不断。
    "machine-interrupts": (origin, req) => ({
      relayedSessions: origin === "devbox" ? 2 : 0,
      relayedMaybe: 0,
      liveStreams: origin === "devbox" ? 2 : 0,
      forwards: origin === "<local>" && req.machine === "devbox" ? 1 : 0,
    }),
    "apikey-read": () => ({ configured: true, masked: "sk-ant-…a1b2", notice: null, path: `${HOME}/.cc-monitor/apikey-credentials.json`, problem: null }),
    "apikey-routing": () => ({ routed: [`${HOME}/.cc-monitor/accounts/api`], running: true }),
    // 共用 MCP：两条共用；issue-tracker 在 personal 号里也改了一版（两边都改，等人选）。
    "accounts-mcp-read": () => ({
      enabled: true,
      sync: true,
      servers: ["docs-search", "issue-tracker"],
      conflicts: [{ name: "issue-tracker", choices: [{ from: null, holders: ["work", "api"], gone: false }, { from: "personal", holders: ["personal"], gone: false }] }],
      changed: [],
      notes: [],
    }),
    "accounts-mcp-sync": (_o, req) => ({ enabled: true, sync: req.on === true, servers: ["docs-search", "issue-tracker"], conflicts: [], changed: [], notes: [] }),
    // devbox 上记下了三种认不出的会话流（结构夹具：键名是造的）。
    "drift-report": (origin) => ({
      faces:
        origin === "devbox"
          ? [{ face: "record_type", consequence: "不显示", overflowed: false, entries: ["tool_progress", "ghost_note", "x_marker"].map((key) => ({ key, count: 2, first_sample: null })) }]
          : [],
    }),
    // 起新会话要的三格那台推：源会话在跑 ⇒ 号与终端都知道；已结束 ⇒ 号与终端说不出（码同真后端）。
    "session-fork": (_o, req, w) => {
      const s = w.sessions.find((x) => x.sid === req.sid);
      const live = s !== undefined && !s.ended;
      return {
        sessionId: "5e55f0f0-0000-4000-8000-00000000f0f0",
        jsonlPath: `${HOME}/.claude/projects/-home-user-work-orders/5e55f0f0-0000-4000-8000-00000000f0f0.jsonl`,
        launch: {
          cwd: s ? { kind: "known", value: s.cwd, from: "record" } : { kind: "unknown", why: "no_cwd" },
          account: live ? { kind: "known", value: null, from: "process" } : { kind: "unknown", why: "exited" },
          terminal: live
            ? { kind: "known", value: { host: "tmux", terminal: `tmux-cc-${s.cwd.split(/[/\\]/).pop()}` }, from: "terminal_list" }
            : { kind: "unknown", why: "exited" },
        },
      };
    },
    "forward-list": () => ({
      forwards: [
        { id: "fwd-1", origin: "devbox", localPort: 15432, remoteHost: "localhost", remotePort: 5432, state: "running", connCount: 2 },
        { id: "fwd-2", origin: "gpu-01", localPort: 16006, remoteHost: "localhost", remotePort: 6006, state: "running", connCount: 0 },
      ],
    }),
    "ssh-config-import": () => ({
      groups: [
        { label: "devbox", host: "10.0.0.11", port: 22, user: "user", keyPath: null, addresses: ["devbox.example.com:22"], jump: null, members: [{ alias: "devbox", host: "10.0.0.11", port: 22, proxyJump: null }, { alias: "devbox-wan", host: "devbox.example.com", port: 22, proxyJump: null }], inList: true },
        { label: "build-02", host: "10.0.0.31", port: 2222, user: "ci", keyPath: null, addresses: [], jump: "bastion", members: [{ alias: "build-02", host: "10.0.0.31", port: 2222, proxyJump: "bastion" }], inList: false },
        { label: "bastion", host: "bastion.example.com", port: 22, user: "user", keyPath: null, addresses: [], jump: null, members: [{ alias: "bastion", host: "bastion.example.com", port: 22, proxyJump: null }], inList: false },
      ],
    }),
    // 接入那一格：~/.bashrc 已接上（第 40 行），你在第 125 / 129 行自己写了 cc · cct（链接那一条恒是你写的生效）。
    "aliases-read": () => ({
      home: HOME,
      otherRc: null,
      rcCandidates: [
        {
          path: `${HOME}/.bashrc`,
          sourced: false,
          exists: true,
          block: {
            present: true,
            version: "v2",
            outdated: false,
            conflictingFunctions: [
              { name: "cc", line: 125, wins: "yours" },
              { name: "cct", line: 129, wins: "yours" },
            ],
            manualCleanupHint: "",
          },
          unreadable: null,
          policy: null,
          blockLines: 4,
        },
      ],
    }),
    "aliases-block-render": () => ({
      text: '# === cc-monitor remote ccm BEGIN v2 ===\nexport PATH="$HOME/.cc-monitor/bin:$PATH"\nif [ -r "$HOME/.cc-monitor/aliases.sh" ]; then . "$HOME/.cc-monitor/aliases.sh"; fi\n# === cc-monitor remote ccm END ===',
    }),
    "ccm-print": () => ({ line: "cd ~ && exec claude" }),
    "assets-sync": (_o, _r, w) => ({
      self: null,
      synced: w.machines.slice(1).map((origin) => ({ origin, peer: null, changed: false, pushed: 0, error: null })),
      reach: w.machines.slice(1).map((origin) => ({ origin, machine: null })),
    }),
    "terminal-preview": () => ({
      lines: [
        "╭──────────────────────────────────────────────╮",
        "│ ✻ Welcome to Claude Code!                    │",
        "╰──────────────────────────────────────────────╯",
        "",
        "> 订单服务调用库存接口时偶尔超时，帮我加上重试…",
        "",
        "● 我先看一下现在的调用点和配置。",
        "",
        "● Bash(pytest -q)",
        "  ⎿  213 passed in 9.41s",
        "",
        "✻ Thinking… (esc to interrupt)",
        "",
        "─────────────────────────────────────────────────",
        "> ",
        "─────────────────────────────────────────────────",
        "  ⏵⏵ accept edits on (shift+tab to cycle)",
      ].map((text) => ({ text })),
      screen: "00000000000000a1",
      captured_at: Math.floor(Date.parse("2026-10-06T10:42:05") / 1000),
    }),
    // 终端名单：每个没结束的会话一个 tmux 终端；第一个会话有一个终端窗口连着，别的在后台。
    "terminals-list": (o, _r, w) => ({
      complete: true,
      terminals: w.sessions
        .filter((x) => !x.ended && x.origin === o)
        .map((x, i) => {
          const name = `cc-${x.cwd.split(/[/\\]/).pop()}`;
          return {
            terminal: `tmux-${name}`,
            host: "tmux",
            tmux_name: name,
            title: name,
            program: "claude",
            cwd: x.cwd,
            session: { sid: x.sid, agent: "claude" },
            purpose: "normal",
            started_by: { client: "ccm", mine: true },
            clients: i === 0 ? [{ kind: "terminal-window", since: 1_700_000_000, last_activity: 1_700_000_100 }] : [],
            input: "shared",
            state: "running",
            last_activity: 1_700_000_100,
            can: { preview: true, input: true, end: true },
          };
        }),
    }),
    "terminal-input": () => ({ result: "delivered" }),
    "ssh-config-aliases": () => ({ aliases: ["devbox", "gpu-01", "win-laptop", "bastion"] }),
    "exit-policy-read": () => ({ state: "absent", killOnExit: false, reason: null, path: null, said: copyText("backendPolicy.exit.unattended") }),
    "footprint-report": () => footprint(),
    // 文件与数据：devbox 照稿 25 那六件（旧 ccm · 两条重名 · 失效行 · 实时显示 · 收信）；本机缺一个可选的开终端工具、实时显示已做；改过 ~/.bashrc 与扩展装的 skill。
    // 离线那台的上次值（本机后端记着的）：gpu-01 两天前读成的那一份「文件与数据」；记下一律答好。
    "last-seen-write": () => ({ atMs: Date.now() }),
    "last-seen-read": (_o, r) => ({
      accounts: null,
      data:
        r.origin === "gpu-01"
          ? {
              atMs: Date.now() - 2 * 24 * 3600 * 1000,
              value: {
                home: HOME,
                changedFiles: [BASHRC_CHANGED],
                todo: [],
                tmux: true,
                chores: 0,
                own: [
                  { id: "bin", path: "~/.cc-monitor/bin", dir: true, class: "cache", exists: true, size: null },
                  { id: "profiles", path: "~/.cc-monitor/profiles.toml", dir: false, class: "truth", exists: true, size: 920 },
                  { id: "accounts", path: "~/.cc-monitor/accounts", dir: true, class: "truth", exists: true, size: null },
                ],
              },
            }
          : null,
    }),
    "data-report": (o, _r, w) => {
      if (w.unseenMachines.includes(o)) throw new Error(`${o} unreachable`);
      const todo = o === "devbox" ? DEVBOX_CHORES : o === "<local>" ? LOCAL_CHORES : [];
      return {
        home: HOME,
        changedFiles: [
          BASHRC_CHANGED,
          { path: "~/.claude/skills/cc-bus", what: "cc-bus", undo: { page: "ext" } },
        ],
        todo,
        tmux: o !== "win-laptop",
        chores: todo.filter((c) => ["must", "install", "decide"].includes(c.kind) && (c.state === "todo" || c.state === "expired")).length,
        own: [
          { id: "bin", path: "~/.cc-monitor/bin", dir: true, class: "cache", exists: true, size: null },
          { id: "relayKey", path: "~/.cc-monitor/relay-key", dir: false, class: "truth", exists: true, size: 64 },
          { id: "policy", path: "~/.cc-monitor/backend.json", dir: false, class: "truth", exists: false, size: null },
          { id: "profiles", path: "~/.cc-monitor/profiles.toml", dir: false, class: "truth", exists: true, size: 1840 },
          { id: "aliasesPosix", path: "~/.cc-monitor/aliases.sh", dir: false, class: "truth", exists: true, size: 512 },
          { id: "chores", path: "~/.cc-monitor/chores.json", dir: false, class: "truth", exists: true, size: 96 },
          { id: "quota", path: "~/.cc-monitor/quota.json", dir: false, class: "truth", exists: true, size: 3172 },
          { id: "accounts", path: "~/.cc-monitor/accounts", dir: true, class: "truth", exists: true, size: null },
          { id: "knownHosts", path: "~/.cc-monitor/known_hosts", dir: false, class: "cache", exists: true, size: 802 },
        ],
      };
    },
    "sessions-where": (_o, req, w) => ({
      results: (req.sids as string[]).map((sid) => {
        const s = w.sessions.find((x) => x.sid === sid);
        if (!s || s.ended) return { sid, standing: "none", names: [], terminals: [] };
        const name = `cc-${s.cwd.split(/[/\\]/).pop()}`;
        return { sid, standing: s.idle ? "idle" : "running", names: [name], terminals: [{ host: "tmux", terminal: `tmux-${name}` }] };
      }),
    }),
    "history-find": (_o, req) => {
      const q = String(req.query ?? "");
      const at = (min: number): number => Date.now() - min * 60_000;
      return {
        total: 4,
        hits: [
          { uuid: "00000000-0000-4000-8000-000000000001", kind: "user", before: "帮我加上", matched: q, after: "（指数退避）和整体超时，顺便补测试。", turn: 1, tsMs: at(14) },
          { uuid: "00000000-0000-4000-8000-000000000006", kind: "assistant", before: "统一做**", matched: q, after: " ＋ 退避 ＋ 整体超时**；", turn: 1, tsMs: at(12) },
          { uuid: "00000000-0000-4000-8000-000000000007", kind: "assistant", before: "为 InventoryClient._call 写单元测试：成功、", matched: q, after: "后成功、重试耗尽三种。", turn: 1, tsMs: at(11) },
          { uuid: "00000000-0000-4000-8000-0000000000ff", kind: "assistant", before: "上次讨论过要不要做", matched: q, after: "，当时的结论是先不做", turn: 1, tsMs: at(60 * 26) },
        ],
      };
    },
  };
}

/** 「改过你的文件」里那一行 ~/.bashrc（data-report 与离线那台的上次值共用）。 */
const BASHRC_CHANGED = { path: "~/.bashrc", what: copyText("rsToolRegistry.tools.ccmName"), undo: { page: "machine", tab: "config", anchor: "connect-terminal" } };

/** 那台的状态成品（形状同 `tests/__fixtures__/machine-state.golden.json`）：看不见的那台 ＝ 密钥被拒。 */
function machineOf(origin: string, w: World): Record<string, unknown> {
  const base = { stage: null, os: origin === "win-laptop" ? "Windows" : "Linux", version: "4.1.1" as string | null, versionRelation: "same" as string | null, seenHostKey: null as string | null };
  if (w.installingMachines?.includes(origin)) return { ...base, state: "installing", reason: null, version: null, versionRelation: null, fixes: [] };
  if (w.hostKeyChanged?.includes(origin))
    return { ...base, state: "host_key_changed", reason: "host_key", version: null, versionRelation: null, fixes: ["compare_fingerprint"], seenHostKey: "SHA256:Zq81nVb0cR2yT6wXe4uLm9kPp3sHd7fJg5aQiO1tYw8" };
  if (w.staleMachines.includes(origin)) return { ...base, state: "needs_update", reason: null, version: null, versionRelation: "older", fixes: ["update"] };
  if (!w.unseenMachines.includes(origin)) return { ...base, state: "up", reason: null, fixes: [] };
  return { ...base, state: "down", reason: "auth", version: null, versionRelation: null, fixes: ["push_key", "conn_settings"] };
}

export function machineCommands(): Record<string, CommandHandler> {
  return {
    // 机器表试算口：同名 · 端口 · 地址 / 用户空（与真写口同一张规则的合成版，按盘上那张表 ＋ 这一批新增的算）。
    machine_table_try: (a, w) => tryMachineTable(a.edits as Record<string, unknown>[], w),
    list_remote_mcp_origins: (_a, w) => w.machines.slice(1).filter((m) => !w.unseenMachines.includes(m)),
    backend_status: (a, w) => ({
      channel: !w.unseenMachines.includes(String(a.origin)),
      pid: pidOf(String(a.origin), w),
      machine: machineOf(String(a.origin), w),
      health: {
        state: "clean",
        summary: copyText("rsBackendPolicy.health.clean"),
        why: null,
        detail: null,
      },
    }),
    local_ccm_entry_status: () => ({
      entry: "$HOME/.cc-monitor/bin/ccm",
      ours: { installed: true, version: "4.0.6", capabilities: ["tmux", "launcher"], build: "shots" },
      on_path: { installed: true, version: "4.0.6", capabilities: ["tmux", "launcher"], build: "shots" },
      verdict: "ours",
      message: "",
      ok: true,
      summary: "",
    }),
    bound_terminal_count: () => 2,
    get_data_paths: () => ({
      monitorDataDir: `${HOME}/.cc-monitor`,
      entries: [
        { label: "配置", class: "truth", path: `${HOME}/.cc-monitor/config.json`, kind: "file", description: "界面的偏好与机器列表", exists: true, sizeBytes: 4120 },
        { label: "日志", class: "cache", path: `${HOME}/.cc-monitor/logs`, kind: "dir", description: "运行日志（按天滚动）", exists: true, sizeBytes: 3_400_000 },
      ],
      backendHome: `${HOME}/.cc-monitor`,
      backendEntries: [
        { label: "账号", class: "truth", path: `${HOME}/.cc-monitor/accounts`, kind: "dir", description: "各账号的登录与配置", exists: true, sizeBytes: 820_000 },
      ],
      webviewUserDataDir: null,
    }),
    get_diagnostics_config: () => ({ log_enabled: true, log_level: "info", error_toast: true, max_files: 7 }),
    set_diagnostics_config: () => "needs_restart",
    get_log_file_info: () => ({
      dir: `${HOME}/.cc-monitor/logs`,
      current_file: `${HOME}/.cc-monitor/logs/monitor.2026-10-01.log`,
      current_size_bytes: 482_113,
      all_files: [
        { path: `${HOME}/.cc-monitor/logs/monitor.2026-10-01.log`, size_bytes: 482_113, modified_ms: Date.parse("2026-10-01T11:58:00Z") },
        { path: `${HOME}/.cc-monitor/logs/monitor.2026-09-30.log`, size_bytes: 1_204_551, modified_ms: Date.parse("2026-09-30T23:59:00Z") },
      ],
      backend_stderr: [],
    }),
    cc_get_auto_launch: () => ({ auto_launch_enabled: false, monitor_exe_path: "/opt/cc-monitor/cc-monitor" }),
    ccm_user_path_status: () => ({ supported: false, dir: null, onUserPath: false, addCommand: null, removeCommand: null, error: null }),
    drift_ledger_report: (a) => ({ origin: String(a.origin ?? "<local>"), faces: [] }),
    diagnostics_report: (a) => {
      const keys = (a.configUnknown as string[] | undefined) ?? [];
      return {
        text: [copyText("rsDiagReport.text.head"), "版本 4.1.1 · 构建 p8n-mcp-sync · linux", copyText("rsDiagReport.text.machines"), "  本机 · up · 版本 4.1.1 · 构建 p8n-mcp-sync", "  devbox · up · 版本 4.1.1 · 构建 p8n-mcp-sync", copyText("rsDiagReport.text.unknown"), "  devbox · 会话记录 3 条", `  config.json · ${keys.length} 项`, `日志：${HOME}/.cc-monitor/logs/monitor.2026-10-01.log`].join("\n"),
        unknown: [
          { machine: "本机", records: 0 },
          { machine: "devbox", records: 3 },
          { machine: "gpu-01", records: 0 },
          { machine: "win-laptop", records: 0 },
        ],
        configUnknown: keys.length,
      };
    },
    footprint_client_facts: () => ({}),
  };
}

/** 合成的机器表试算：只认新增（`insertin`）与改名 / 改端口（`setin`），够场景用。 */
function tryMachineTable(edits: Record<string, unknown>[], w: World): Record<string, unknown> | null {
  const remote = (w.config.remote ?? {}) as { hosts?: Record<string, unknown>[] };
  const hosts = (remote.hosts ?? []).map((h) => ({ ...h }));
  const keyOf = (h: Record<string, unknown>): string => String(h.label ?? "").trim() || String(h.host ?? "").trim();
  for (const e of edits) {
    if (e.op === "insertin") hosts.push({ ...(e.value as Record<string, unknown>) });
    if (e.op === "setin") {
      const where = (e.where as { equals: string }[])[0]?.equals;
      const h = hosts.find((x) => keyOf(x) === where);
      if (h) h[String(e.field)] = e.value;
    }
  }
  const seen = new Set<string>();
  for (const h of hosts) {
    const name = keyOf(h);
    if (String(h.host ?? "").trim() === "") return { code: "no_host", name };
    if (String(h.user ?? "").trim() === "") return { code: "no_user", name };
    const port = h.port === undefined || h.port === null ? 22 : Number(h.port);
    if (!Number.isInteger(port) || port < 1 || port > 65535) return { code: "port", name };
    if (seen.has(name)) return { code: "name_taken", name };
    seen.add(name);
  }
  return null;
}

/** 「要你动手」一件（假后端照 `footprint/chores` 的成品形状）。 */
function chore(over: Record<string, unknown>): Record<string, unknown> & { kind: string; state: string } {
  return { id: "", kind: "optional", state: "todo", name: "", loc: "", said: "", why: "", steps: [], diff: [], copy: null, whole: null, wholeCovers: [], file: null, go: null, howUrl: null, mask: null, action: "copySnippet", ...over } as Record<string, unknown> & { kind: string; state: string };
}

const SETTINGS = `${HOME}/.claude/settings.json`;
const WHOLE = '{\n  "env": {\n    "ANTHROPIC_BASE_URL": "http://127.0.0.1:8788/k/9f3c2a71/claude",\n    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "32000"\n  }\n}\n';
const DEVBOX_CHORES = [
  chore({ id: "stale-ccm", kind: "must", name: copyText("beChore.staleCcm.name"), loc: "/usr/local/bin/ccm", said: copyText("beChore.staleCcm.said"), why: "敲 ccm 和别名先找到的是 /usr/local/bin/ccm：不是 cc-monitor 放的，起的会话 cc-monitor 管不到", steps: [copyText("beChore.staleCcm.step")], copy: "sudo rm '/usr/local/bin/ccm'", action: "copyCommand" }),
  chore({ id: "clash:cc", kind: "decide", name: "cc 重名 · 你写的 / 清单", loc: "~/.bashrc 第 125 行", said: copyText("beChore.clash.winsYours"), why: "现在敲 cc 起的是后定义的那一个；没生效的那一条白放着", copy: `${HOME}/.bashrc:125`, file: `${HOME}/.bashrc`, go: { page: "machine", tab: "config", anchor: "clash" }, action: "decide" }),
  chore({ id: "clash:cct", kind: "decide", name: "cct 重名 · 你写的 / 清单", loc: "~/.bashrc 第 129 行", said: copyText("beChore.clash.winsYours"), why: "现在敲 cct 起的是后定义的那一个；没生效的那一条白放着", copy: `${HOME}/.bashrc:129`, file: `${HOME}/.bashrc`, go: { page: "machine", tab: "config", anchor: "clash" }, action: "decide" }),
  chore({ id: `dead:${HOME}/.bashrc`, name: ".bashrc · 2 行失效", loc: "~/.bashrc 第 118, 119 行", said: copyText("beChore.dead.said"), why: copyText("beChore.dead.why"), steps: ["第 118 行：source ~/.old-ccm.sh", "第 119 行：. ~/bin/ccm-env"], copy: `${HOME}/.bashrc:118`, file: `${HOME}/.bashrc`, action: "locate" }),
  chore({
    id: "relay",
    name: copyText("beChore.relay.name"),
    loc: "~/.claude/settings.json · env",
    said: copyText("beChore.relay.saidTodo"),
    why: copyText("beChore.relay.why"),
    steps: ["打开 ~/.claude/settings.json", "在第 2 行后面加下面 1 行", copyText("beChore.step.save")],
    diff: [
      { n: 2, op: "same", text: '  "env": {' },
      { n: null, op: "add", text: '    "ANTHROPIC_BASE_URL": "http://127.0.0.1:8788/k/9f3c2a71/claude",' },
      { n: 3, op: "same", text: '    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "32000"' },
    ],
    copy: '    "ANTHROPIC_BASE_URL": "http://127.0.0.1:8788/k/9f3c2a71/claude",',
    whole: WHOLE,
    wholeCovers: ["relay", "cc-bus-hooks"],
    file: SETTINGS,
    mask: "9f3c2a71",
  }),
  chore({ id: "cc-bus-hooks", name: copyText("beChore.hooks.name"), loc: "~/.claude/settings.json · hooks", said: copyText("beChore.hooks.said"), why: copyText("beChore.hooks.why"), steps: ["打开 ~/.claude/settings.json", "在第 5 行后面加下面 12 行", copyText("beChore.step.save")], copy: '"hooks": {}', whole: WHOLE, wholeCovers: ["relay", "cc-bus-hooks"], file: SETTINGS }),
];
const LOCAL_CHORES = [
  chore({ id: "install:xdg-terminal-exec", kind: "installOptional", name: "xdg-terminal-exec", loc: "xdg-terminal-exec", said: copyText("beChore.install.saidOptional"), action: "how", howUrl: "https://gitlab.freedesktop.org/terminal-wg/specifications" }),
  chore({ id: "relay", state: "done", name: copyText("beChore.relay.name"), loc: "~/.claude/settings.json · env", said: copyText("beChore.relay.saidDone"), file: SETTINGS }),
];
