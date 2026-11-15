/**
 * 机器与设置那一族的合成答法：后端状态、ccm 入口、ssh 配置、tmux、足迹、数据位置、日志 …
 * 后端说的那几句话一律取文案表（与真后端出的成品同一份字），形状照各跨语言金样。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import EXT_GOLDEN from "../../__fixtures__/ext-flow.golden.json";
import type { AliasListing } from "../../../src/frontend/ui/generated/AliasListing";
import type { BackendStatus } from "../../../src/frontend/ui/generated/BackendStatus";
import type { ExtBring } from "../../../src/frontend/ui/generated/ExtBring";
import type { ExtCell } from "../../../src/frontend/ui/generated/ExtCell";
import type { FootprintClient } from "../../../src/frontend/ui/generated/FootprintClient";
import type { ForwardRow } from "../../../src/frontend/ui/generated/ForwardRow";
import type { Forwards } from "../../../src/frontend/ui/generated/Forwards";
import type { CommandHandler } from "./commands";
import type { OpHandler, World } from "./types";

const HOME = "/home/user";

const pidOf = (origin: string, w: World): number => 3200 + w.machines.indexOf(origin) * 17;

/** 那台的漂移账（`drift-report` 那一形）：devbox 上记下了三种认不出的会话流（结构夹具：键名是造的）。 */
const driftOf = (origin: string) => ({
  faces:
    origin === "devbox"
      ? [{ face: "record_type", consequence: "不显示", overflowed: false, entries: ["tool_progress", "ghost_note", "x_marker"].map((key) => ({ key, count: 2, first_sample: null })) }]
      : [],
});

/** 扩展页那张表：金样的临时目录换成合成那台的家，两台机器换成本机与 devbox。 */
function extList(w: World): unknown {
  const g = JSON.parse(JSON.stringify(EXT_GOLDEN.list).split("<BASE>/a").join(HOME).split("<BASE>/b").join("/home/dev")) as {
    machines: { name: string; key: string | null }[];
  };
  g.machines[0].name = "workstation";
  g.machines[1].name = "devbox";
  g.machines[1].key = "devbox";
  // 再两台：gpu-01 断开（那一列是上次同步的样子）· win-laptop；外加一条 MCP「github」，本机装了、devbox 内容不同、另两台没装。
  const rows = (g as unknown as { rows: { cells: unknown[] }[] }).rows;
  const more = [
    { here: false, key: "gpu-01", name: "gpu-01", reachable: !w.unseenMachines.includes("gpu-01") },
    { here: false, key: "win-laptop", name: "win-laptop", reachable: !w.unseenMachines.includes("win-laptop") },
  ];
  g.machines.push(...(more as never[]));
  // 照线上类型 `ExtCell` / `ExtPlace`：点与每一处那一句（`said`）由那台写好（`assets/ext.rs::state_said`）。
  const SAID: Record<"same" | "differs" | "missing", string> = {
    same: copyText("beExt.legend.same"),
    differs: copyText("beExt.state.differs"),
    missing: copyText("reason.spawn.notInstalled"),
  };
  const cell = (state: "same" | "differs" | "missing", bring: ExtBring | null): ExtCell => ({
    bring,
    note: null,
    said: SAID[state],
    places: [{ at: { level: "user" }, dir: null, note: null, state, said: SAID[state], uninstall: state !== "missing" }],
    state,
  });
  for (const r of rows) r.cells.push(cell("missing", null), cell("missing", null));
  const bring: ExtBring = { from: null, scope: { from: { level: "user" }, to: { level: "user" } }, targets: [{ at: { level: "user" }, note: null, ok: true }] };
  rows.push({
    about: "读写 GitHub 上的仓库与 issue",
    known: null,
    cells: [cell("same", null), cell("differs", bring), cell("missing", bring), cell("missing", bring)],
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
    "ext-list": (_o, _r, w) => extList(w),
    // 「装到 N 台」那一张卡（本机后端并好的）：MCP 那一条要一个 token，win-laptop 上已经有了。
    "ext-hub-preview": (_o, req) => {
      const to = req.to as (string | null)[];
      const path = (t: string | null) => (t === "win-laptop" ? "C:\\Users\\user\\.claude.json" : `${t === null ? HOME : "/home/user"}/.claude.json`);
      return {
        places: [{ at: { level: "user" }, ok: true, note: null }],
        place: { level: "user" },
        slots: [{ field: "env", key: "GITHUB_TOKEN", kept: to.includes("win-laptop") ? ["win-laptop"] : [] }],
        machines: to.map((t) => ({
          to: t,
          name: t ?? "workstation",
          card: { kind: req.kind, name: req.name, path: path(t), writes: [path(t)], unchanged: false, suspects: [], stop: null, config: null, slots: [{ field: "env", key: "GITHUB_TOKEN", kept: t === "win-laptop" }], tokens: { source: "s1", target: "t1" } },
          files: [`${path(t)} · mcpServers`],
          error: null,
        })),
      };
    },
    // 共用 MCP：两条共用；issue-tracker 在 personal 号里也改了一版（两边都改，等人选）。
    "accounts-mcp-read": () => ({
      enabled: true,
      sync: true,
      servers: [{ name: "docs-search" }, { name: "issue-tracker" }],
      conflicts: [{ name: "issue-tracker", choices: [{ from: null, holders: ["work", "api"], gone: false }, { from: "personal", holders: ["personal"], gone: false }] }],
      changed: [],
      notes: [],
    }),
    "accounts-mcp-sync": (_o, req) => ({ enabled: true, sync: req.on === true, servers: [{ name: "docs-search" }, { name: "issue-tracker" }], conflicts: [], changed: [], notes: [] }),
    "drift-report": (origin) => driftOf(origin),
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
    // 照线上类型 `Forwards`：那一行给人看的字（`said`）由那台写好（`dial/forwards.rs::list_with`）。
    "forward-list": (): Forwards => {
      const row = (id: string, origin: string, localPort: number, remotePort: number, connCount: number): ForwardRow => ({
        id,
        origin,
        localPort,
        remoteHost: "localhost",
        remotePort,
        state: "running",
        said: copyText("portForward.reload.row", { origin, localPort, remoteHost: "localhost", remotePort, connCount }),
      });
      return { forwards: [row("fwd-1", "devbox", 15432, 5432, 2), row("fwd-2", "gpu-01", 16006, 6006, 0)] };
    },
    // 接入那一格：~/.bashrc 已接上（第 40 行），你在第 125 / 129 行自己写了 cc · cct（链接那一条恒是你写的生效）。
    "aliases-read": () => ({
      shell: "posix",
      otherRc: null,
      rcCandidates: [
        {
          path: `${HOME}/.bashrc`,
          shown: "~/.bashrc",
          sourced: false,
          exists: true,
          block: {
            present: true,
            conflictingFunctions: [
              { name: "cc", line: 125, wins: "yours" },
              { name: "cct", line: 129, wins: "yours" },
            ],
          },
          unreadable: null,
          policy: null,
          blockLines: 4,
        },
      ],
    }) satisfies AliasListing,
    "ccm-print": () => ({ line: "cd ~ && exec claude" }),
    "assets-sync": (_o, _r, w) => ({
      self: null,
      synced: w.machines.slice(1).map((origin) => ({ origin, peer: null, changed: false, pushed: 0, error: null })),
      reach: w.machines.slice(1).map((origin) => ({ origin, machine: null })),
    }),
    // 文件与数据：devbox 那六件（旧 ccm · 两条重名 · 失效行 · 实时显示 · 收信）；本机缺一个可选的开终端工具、实时显示已做；改过 ~/.bashrc 与扩展装的 skill。
    // 离线那台的上次值（本机后端记着的）：gpu-01 两天前读成的那一份「文件与数据」；记下一律答好。
    "data-report": (o, _r, w) => {
      if (w.unseenMachines.includes(o)) throw new Error(`${o} unreachable`);
      const todo = o === "devbox" ? DEVBOX_CHORES : o === "<local>" ? LOCAL_CHORES : [];
      return {
        changedFiles: [
          BASHRC_CHANGED,
          { path: "~/.claude/skills/cc-bus", what: "cc-bus", undo: { page: "ext" } },
        ],
        todo,
        tmux: o !== "win-laptop",
        counts: choreCounts(todo),
        rank: ((n) => (n.badge > 0 ? 0 : n.open > 0 ? 1 : 2))(choreCounts(todo)),
        // 显示名与绝对路径是那台后端写好的两格（`footprint/data.rs`）：台架照它的写法补上。
        own: [
          { id: "bin", what: "台架编的一句 · bin", path: "~/.cc-monitor/bin", dir: true, class: "cache", exists: true, absent_said: null, size_text: null },
          { id: "relayKey", what: "台架编的一句 · relayKey", path: "~/.cc-monitor/relay-key", dir: false, class: "truth", exists: true, absent_said: null, size_text: "64 B" },
          { id: "policy", what: "台架编的一句 · policy", path: "~/.cc-monitor/backend.json", dir: false, class: "truth", exists: false, absent_said: "台架编的一句 · 未创建", size_text: null },
          { id: "profiles", what: "台架编的一句 · profiles", path: "~/.cc-monitor/profiles.toml", dir: false, class: "truth", exists: true, absent_said: null, size_text: "1.8 KB" },
          { id: "aliasesPosix", what: "台架编的一句 · aliasesPosix", path: "~/.cc-monitor/aliases.sh", dir: false, class: "truth", exists: true, absent_said: null, size_text: "512 B" },
          { id: "chores", what: "台架编的一句 · chores", path: "~/.cc-monitor/chores.json", dir: false, class: "truth", exists: true, absent_said: null, size_text: "96 B" },
          { id: "quota", what: "台架编的一句 · quota", path: "~/.cc-monitor/quota.json", dir: false, class: "truth", exists: true, absent_said: null, size_text: "3.1 KB" },
          { id: "accounts", what: "台架编的一句 · accounts", path: "~/.cc-monitor/accounts", dir: true, class: "truth", exists: true, absent_said: null, size_text: null },
          { id: "knownHosts", what: "台架编的一句 · knownHosts", path: "~/.cc-monitor/known_hosts", dir: false, class: "cache", exists: true, absent_said: null, size_text: "802 B" },
          // 本机那一问：本机后端连界面自己那几样一起答。
          ...(o === "<local>"
            ? [
                { id: "config", what: "台架编的一句 · config", path: "~/.cc-monitor/config.json", dir: false, class: "truth", exists: true, absent_said: null, size_text: "4.0 KB" },
                { id: "logs", what: "台架编的一句 · logs", path: "~/.cc-monitor/logs", dir: true, class: "cache", exists: true, absent_said: null, size_text: null },
              ]
            : []),
        ].map(shownAt),
      };
    },
  };
}

/** `data-report` 的 `own` 一行补上后端写好的显示名（`.cc-monitor/` 底下那一段，目录带尾 `/`）与绝对路径（台架家目录 `HOME`）。 */
function shownAt<T extends { path: string; dir: boolean }>(o: T): T & { label: string; abs: string } {
  const tail = o.path.replace(/^~\/\.cc-monitor\/?/, "");
  return { ...o, label: o.dir ? `${tail}/` : tail, abs: o.path.replace(/^~/, HOME) };
}

/** 一台各类的件数（照 `footprint/chores::counts` 的分法；`data-report` 还是合成的那几条之一）。 */
function choreCounts(todo: readonly { kind: string; state: string }[]): Record<string, number> {
  const n = { badge: 0, open: 0, must: 0, decide: 0, install: 0, optional: 0, done: 0 };
  for (const c of todo) {
    if (c.state === "done") n.done++;
    if (c.state === "done" || c.state === "declined") continue;
    n.open++;
    const k = c.kind === "installOptional" ? "optional" : (c.kind as "must" | "decide" | "install" | "optional");
    n[k]++;
    if (c.state !== "blocked" && k !== "optional") n.badge++;
  }
  return n;
}

/** 「改过你的文件」里那一行 ~/.bashrc（data-report 与离线那台的上次值共用）。 */
const BASHRC_CHANGED = { path: "~/.bashrc", what: copyText("rsToolRegistry.tools.ccmName"), undo: { page: "machine", tab: "config", anchor: "connect-terminal" } };

/**
 * 本机后端那份主机钥匙库里每台记下的那一把（`.cc-monitor/known_hosts`，`world.ts::diskOf` 写；机器表的指纹格由真后端从它现算）
 * 与它的指纹（指纹变了的那台那一轮带的「库里记下的那一枚」）。
 */
export const HOST_KEYS: Record<string, { key: string; fingerprint: string }> = {
  devbox: { key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpaWlpa", fingerprint: "SHA256:jysidH8sUN9+mB/5Wx25m7gT4aa66R2kbn4o2cy0Vio" },
  "gpu-01": { key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFtbW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tbW1tb", fingerprint: "SHA256:5aZuCW8wYjCRfhDeCMdhjgQfFrWpWCmOoF8dWVzJ1pI" },
  "win-laptop": { key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFxcXFxcXFxcXFxcXFxcXFxcXFxcXFxcXFxcXFxcXFxc", fingerprint: "SHA256:yj4Kqj4VGoZpFeQmmY0rmphvdZDJOAFJ1pYW3ezkwPc" },
};

export function machineCommands(): Record<string, CommandHandler> {
  return {
    // 形状照壳那一口（`BackendStatus`）：本机后端起没起来是壳的事实（`local`）；各台（含本机）的状态成品与名字都在本机后端（`machine-read` · `changed {machine}`），这一口不带。
    backend_status: (a, w): BackendStatus => ({
      origin: String(a.origin),
      local: a.origin === "<local>" ? { state: "up" } : null,
      channel: !w.unseenMachines.includes(String(a.origin)),
      pid: pidOf(String(a.origin), w),
      attempts: a.origin === "<local>" ? 1 : null,
      detached: a.origin === "<local>" ? false : null,
      health: {
        state: "clean",
        summary: copyText("rsBackendPolicy.health.clean"),
        why: null,
        detail: null,
      },
    }),
    get_diagnostics_config: () => ({ log_enabled: true, log_level: "info", error_toast: true, max_files: 7 }),
    set_diagnostics_config: () => "needs_restart",
    get_log_file_info: () => ({
      dir: `${HOME}/.cc-monitor/logs`,
      dir_shown: "~/.cc-monitor/logs",
      current_file: `${HOME}/.cc-monitor/logs/monitor.2026-10-01.log`,
      current_file_shown: "~/.cc-monitor/logs/monitor.2026-10-01.log",
      current_size_text: "470.8 KB",
      all_files: [
        { path: `${HOME}/.cc-monitor/logs/monitor.2026-10-01.log`, shown: "~/.cc-monitor/logs/monitor.2026-10-01.log", size_text: "470.8 KB", modified_ms: Date.parse("2026-10-01T11:58:00Z") },
        { path: `${HOME}/.cc-monitor/logs/monitor.2026-09-30.log`, shown: "~/.cc-monitor/logs/monitor.2026-09-30.log", size_text: "1.1 MB", modified_ms: Date.parse("2026-09-30T23:59:00Z") },
      ],
      backend_stderr: [],
    }),
    cc_get_auto_launch: () => ({ auto_launch_enabled: false, monitor_exe_path: "/opt/cc-monitor/cc-monitor" }),
    // `diagnostics_report` 不在这里：整段由本机真后端 `diagnostics-report` 出（`backend.ts` 的 `invoke` 那一臂交过去）。
    // 形状照壳那一口（`footprint_client.rs`）：界面原样带进本机后端的 `{client}`，后端照 `FootprintClient` 收；家是沙箱里后端那一份。
    footprint_client_facts: (): FootprintClient => ({ home: HOME, path: `${HOME}/.cc-monitor/bin:/usr/bin:/bin` }),
  };
}

/** 「待办」一件（假后端照 `footprint/chores` 的成品形状）。 */
function chore(over: Record<string, unknown>): Record<string, unknown> & { kind: string; state: string } {
  return { id: "", kind: "optional", state: "todo", name: "", loc: "", said: "", why: "", steps: [], diff: [], copy: null, whole: null, wholeCovers: [], file: null, go: null, howUrl: null, action: "copySnippet", tag: { text: copyText("dataPage.kind.optional"), kind: "optional" }, tone: "muted", ...over } as Record<string, unknown> & { kind: string; state: string };
}

const SETTINGS = `${HOME}/.claude/settings.json`;
/** 待定那一类的小标签（同名两件共用）。 */
const DECIDE_TAG = () => ({ tag: { text: copyText("dataPage.kind.decide"), kind: "decide" }, tone: "warn" });
const WHOLE = '{\n  "env": {\n    "ANTHROPIC_BASE_URL": "http://127.0.0.1:8788/k/9f3c2a71/claude",\n    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "32000"\n  }\n}\n';
const DEVBOX_CHORES = [
  chore({ id: "stale-ccm", kind: "must", name: copyText("beChore.staleCcm.name"), loc: "/usr/local/bin/ccm", said: copyText("beChore.staleCcm.said"), why: "敲 ccm 和别名先找到的是 /usr/local/bin/ccm：不是 cc-monitor 放的，起的会话 cc-monitor 无法管理", steps: [copyText("beChore.staleCcm.step")], copy: "sudo rm '/usr/local/bin/ccm'", action: "copyCommand", tag: { text: copyText("dataPage.kind.must"), kind: "must" }, tone: "bad" }),
  chore({ id: "clash:cc", kind: "decide", name: copyText("beChore.clash.name", { name: "cc" }), loc: "~/.bashrc 第 125 行", said: copyText("beChore.clash.winsYours"), why: "现在敲 cc 起的是后定义的那一个；没生效的那一条白放着", copy: `${HOME}/.bashrc:125`, file: `${HOME}/.bashrc`, go: { page: "machine", tab: "config", anchor: "clash" }, action: "decide", ...DECIDE_TAG() }),
  chore({ id: "clash:cct", kind: "decide", name: copyText("beChore.clash.name", { name: "cct" }), loc: "~/.bashrc 第 129 行", said: copyText("beChore.clash.winsYours"), why: "现在敲 cct 起的是后定义的那一个；没生效的那一条白放着", copy: `${HOME}/.bashrc:129`, file: `${HOME}/.bashrc`, go: { page: "machine", tab: "config", anchor: "clash" }, action: "decide", ...DECIDE_TAG() }),
  chore({ id: `dead:${HOME}/.bashrc`, name: ".bashrc · 2 行失效", loc: "~/.bashrc 第 118, 119 行", said: copyText("beChore.dead.said"), why: copyText("beChore.dead.why"), steps: ["第 118 行：source ~/.old-ccm.sh", "第 119 行：. ~/bin/ccm-env"], copy: `${HOME}/.bashrc:118`, file: `${HOME}/.bashrc`, action: "locate" }),
  chore({
    id: "relay:claude-code",
    name: copyText("beChore.relay.name", { agent: "Claude Code" }),
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
    wholeCovers: ["relay:claude-code", "cc-bus-hooks:claude"],
    file: SETTINGS,
  }),
  chore({
    id: "relay:codex",
    name: copyText("beChore.relay.name", { agent: "Codex" }),
    loc: "~/.codex/config.toml · openai_base_url",
    said: copyText("beChore.relay.saidTodo"),
    why: copyText("beChore.relay.why"),
    steps: ["打开 ~/.codex/config.toml", copyText("beChore.step.insertTop", { n: "1" }), copyText("beChore.step.save")],
    diff: [
      { n: null, op: "add", text: 'openai_base_url = "http://127.0.0.1:8788/9f3c2a71/t/codex/_"' },
      { n: 1, op: "same", text: 'model = "gpt-5"' },
    ],
    copy: 'openai_base_url = "http://127.0.0.1:8788/9f3c2a71/t/codex/_"',
    whole: 'openai_base_url = "http://127.0.0.1:8788/9f3c2a71/t/codex/_"\nmodel = "gpt-5"\n',
    wholeCovers: ["relay:codex"],
    file: `${HOME}/.codex/config.toml`,
  }),
  chore({ id: "cc-bus-hooks:claude", name: copyText("beChore.hooks.name"), loc: "~/.claude/settings.json · hooks", said: copyText("beChore.hooks.said"), why: copyText("beChore.hooks.why"), steps: ["打开 ~/.claude/settings.json", "在第 5 行后面加下面 12 行", copyText("beChore.step.save")], copy: '"hooks": {}', whole: WHOLE, wholeCovers: ["relay:claude-code", "cc-bus-hooks:claude"], file: SETTINGS }),
];
const LOCAL_CHORES = [
  chore({ id: "install:xdg-terminal-exec", kind: "installOptional", name: "xdg-terminal-exec", loc: "xdg-terminal-exec", said: copyText("beChore.install.saidOptional"), action: "how", howUrl: "https://gitlab.freedesktop.org/terminal-wg/specifications", tag: { text: copyText("dataPage.install.kindOptional"), kind: "installOptional" } }),
  chore({ id: "relay:claude-code", state: "done", name: copyText("beChore.relay.name", { agent: "Claude Code" }), loc: "~/.claude/settings.json · env", said: copyText("beChore.relay.saidDone"), file: SETTINGS, tone: "ok" }),
];
