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
    "apikey-read": () => ({ configured: true, masked: "sk-ant-…a1b2", notice: null, path: `${HOME}/.cc-monitor/apikey-credentials.json`, problem: null }),
    "apikey-routing": () => ({ routed: [`${HOME}/.cc-monitor/accounts/api`], running: true }),
    "accounts-mcp-read": () => ({ enabled: true, servers: ["docs-search", "issue-tracker"], conflicts: [], changed: [], notes: [] }),
    "drift-report": () => ({ faces: [] }),
    "session-fork": () => ({
      sessionId: "5e55f0f0-0000-4000-8000-00000000f0f0",
      jsonlPath: `${HOME}/.claude/projects/-home-user-work-orders/5e55f0f0-0000-4000-8000-00000000f0f0.jsonl`,
    }),
    "aliases-render": () => JSON.parse(JSON.stringify(ALIASES_GOLDEN.renderReply).split("<HOME>").join(HOME)),
    "forward-list": () => ({
      forwards: [
        { id: "fwd-1", origin: "devbox", localPort: 15432, remoteHost: "localhost", remotePort: 5432, state: "running", connCount: 2 },
        { id: "fwd-2", origin: "gpu-01", localPort: 16006, remoteHost: "localhost", remotePort: 6006, state: "running", connCount: 0 },
      ],
    }),
    "ssh-config-import": () => ({
      groups: [
        { label: "devbox", host: "10.0.0.11", port: 22, user: "user", keyPath: null, addresses: ["devbox.example.com:22"], jump: null, members: [{ alias: "devbox", host: "10.0.0.11", port: 22, proxyJump: null }, { alias: "devbox-wan", host: "devbox.example.com", port: 22, proxyJump: null }] },
        { label: "build-02", host: "10.0.0.31", port: 2222, user: "ci", keyPath: null, addresses: [], jump: "bastion", members: [{ alias: "build-02", host: "10.0.0.31", port: 2222, proxyJump: "bastion" }] },
        { label: "bastion", host: "bastion.example.com", port: 22, user: "user", keyPath: null, addresses: [], jump: null, members: [{ alias: "bastion", host: "bastion.example.com", port: 22, proxyJump: null }] },
      ],
    }),
    "aliases-read": () => JSON.parse(JSON.stringify(ALIASES_GOLDEN.readReply).split("<HOME>").join(HOME)),
    "assets-sync": (_o, _r, w) => ({
      self: null,
      synced: w.machines.slice(1).map((origin) => ({ origin, peer: null, changed: false, pushed: 0, error: null })),
      reach: w.machines.slice(1).map((origin) => ({ origin, machine: null })),
    }),
    "capture-pane": (_o, req) => ({
      name: String(req.name),
      screen: [
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
      ].join("\n"),
    }),
    "ssh-config-aliases": () => ({ aliases: ["devbox", "gpu-01", "win-laptop", "bastion"] }),
    "exit-policy-read": () => ({ state: "absent", killOnExit: false, reason: null, path: null, said: copyText("backendPolicy.exit.unattended") }),
    "footprint-report": () => footprint(),
    "tmux-list": (origin, _r, w) => ({
      installed: true,
      sessions: w.sessions
        .filter((s) => s.origin === origin && !s.ended)
        .map((s, i) => ({ name: `cc-${s.cwd.split(/[/\\]/).pop()}`, path: s.cwd, command: s.idle ? "zsh" : "claude", attached: i === 0, windows: 1, sid: s.sid, agent: !s.idle })),
    }),
    "sessions-tmux": (_o, req, w) => ({
      results: (req.sids as string[]).map((sid) => {
        const s = w.sessions.find((x) => x.sid === sid);
        if (!s || s.ended) return { sid, standing: "none", names: [] };
        return { sid, standing: s.idle ? "idle" : "running", names: [`cc-${s.cwd.split(/[/\\]/).pop()}`] };
      }),
    }),
    "history-find": (_o, req) => {
      const q = String(req.query ?? "");
      return {
        total: 3,
        hits: [
          { uuid: "00000000-0000-4000-8000-000000000001", kind: "user", before: "帮我加上", matched: q, after: "（指数退避）和整体超时，顺便补测试。" },
          { uuid: "00000000-0000-4000-8000-000000000006", kind: "assistant", before: "统一做**", matched: q, after: " ＋ 退避 ＋ 整体超时**；" },
          { uuid: "00000000-0000-4000-8000-000000000007", kind: "assistant", before: "为 InventoryClient._call 写单元测试：成功、", matched: q, after: "后成功、重试耗尽三种。" },
        ],
      };
    },
  };
}

export function machineCommands(): Record<string, CommandHandler> {
  return {
    list_remote_mcp_origins: (_a, w) => w.machines.slice(1).filter((m) => !w.unseenMachines.includes(m)),
    backend_status: (a, w) => ({
      channel: !w.unseenMachines.includes(String(a.origin)),
      pid: pidOf(String(a.origin), w),
      health: {
        state: "clean",
        summary: copyText("rsBackendPolicy.health.clean"),
        why: null,
        detail: copyText("rsBackendPolicy.health.detail", { crashed: 0, refused: 0, neverStarted: 0, misread: 0 }),
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
