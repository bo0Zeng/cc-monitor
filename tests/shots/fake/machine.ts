/**
 * 机器与设置那一族的合成答法：后端状态、ccm 入口、ssh 配置、tmux、足迹、数据位置、日志 …
 * 后端说的那几句话一律取文案表（与真后端出的成品同一份字），形状照各跨语言金样。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import FOOTPRINT_GOLDEN from "../../__fixtures__/footprint-report.golden.json";
import EXT_GOLDEN from "../../__fixtures__/ext-flow.golden.json";
import ALIASES_GOLDEN from "../../__fixtures__/aliases.golden.json";
import type { CommandHandler } from "./commands";
import type { OpHandler, World } from "./types";
import { ACCOUNTS } from "./ops";

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
  return g;
}

export function machineOps(): Record<string, OpHandler> {
  return {
    "ext-list": () => extList(),
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
    "aliases-render": () => ({ ...JSON.parse(JSON.stringify(ALIASES_GOLDEN.renderReply).split("<HOME>").join(HOME)), problems: [] }),
    "aliases-from-form": (_o, req) => ({ alias: { name: (req.form as { name: string }).name || "new", args: ["--"], restTo: "agent" } }),
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
    // 别名清单：默认那三条 ＋ 每个号两条（`{名}cc` · `{名}cct`，api 缺 tmux 那条）＋ 自己加的几条；
    //   ~/.bashrc 已接上（第 40 行），你在第 125 / 129 行自己写了 cc · cct（在块后面 ⇒ 你写的生效）。
    "aliases-read": () => {
      const r = JSON.parse(JSON.stringify(ALIASES_GOLDEN.readReply).split("<HOME>").join(HOME)) as Record<string, unknown>;
      const acct = ACCOUNTS.flatMap((a) =>
        [false, true]
          .filter((tmux) => !(tmux && a.name === "api"))
          .map((tmux) => ({ name: `${a.name}cc${tmux ? "t" : ""}`, args: ["--", "--account", a.name, ...(tmux ? ["--ccm-tmux"] : [])], restTo: "agent", group: { account: a.name, tmux }, said: `${a.name} 账号 · ${tmux ? "tmux · 自动取名" : "当前终端"}` })),
      );
      const other = [
        { name: "cc", args: [], restTo: "agent", group: null, said: "默认账号 · 当前终端" },
        { name: "cct", args: ["--", "--ccm-tmux"], restTo: "agent", group: null, said: "默认账号 · tmux · 自动取名" },
        { name: "cca", args: ["--", "--attach"], restTo: "ccm", group: null, said: "默认账号 · 接回已有 tmux 会话 · 敲 cca 会话名" },
        { name: "conv", args: ["--", "--account", "work", "--cwd-if", "~", "~/projects/notes"], restTo: "agent", group: null, said: "work 账号 · 当前终端 · 在 ~ 敲就进 ~/projects/notes" },
        { name: "fe", args: ["--", "--account", "personal", "--cwd", "~/文档/frontend", "--ccm-tmux=fe"], restTo: "agent", group: null, said: "personal 账号 · tmux 会话 fe · 进 ~/文档/frontend" },
        { name: "opus", args: ["--model", "opus"], restTo: "agent", group: null, said: "默认账号 · 当前终端 · 模型 opus" },
      ];
      const rows = [...other.slice(0, 3), ...acct, ...other.slice(3)];
      const bashrc = {
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
      };
      return {
        ...r,
        aliases: rows.map(({ group: _g, said: _s, ...x }) => x),
        groups: rows.map((x) => x.group),
        said: rows.map((x) => x.said),
        accounts: ACCOUNTS.map((a) => a.name),
        missing: [{ account: "api", tmux: true, alias: { name: "apicct", args: ["--", "--account", "api", "--ccm-tmux"], restTo: "agent" } }],
        rcCandidates: [bashrc],
      };
    },
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
