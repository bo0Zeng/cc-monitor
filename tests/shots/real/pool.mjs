/**
 * 截图台架的真后端池：每个场景一屋子盘上原始格式的家目录（页里交来的 `DiskWorld`），每台机器一个真后端进程（stdio 载体），
 * 前面站着壳的真代码（无头壳 `ccm-shots-shell`：`src/frontend/shell/src/shots_shell.rs`，本机那条读循环 → 会话账 → 重放缓冲 → 通道交格）。
 * 页里的帧命令与订阅经 vite 的这几个口原样交给无头壳。成品（额度行 · 时刻字 · 状态字 · 拒答的复制详情 …）一律是真后端与壳算的，台架不写。
 *
 *   POST /__ccm/world  {key, machines: {<origin>: {files: {<相对家目录的路径>: <内容>}}}}  ⇒ 造好家目录、起无头壳与各台后端、全部握上手再答
 *   POST /__ccm/call   {key, origin, op, args, view?}         ⇒ 壳 `chan_call` 那一跳的结局：`{ok:true, body}` / `{ok:false, fail:{err, body, detail}}`
 *   POST /__ccm/sub    {key, id, origin, kind, want} · /__ccm/want {key, id, more} · /__ccm/stop {key, id} · /__ccm/ready {key, priority_sid}
 *   POST /__ccm/watch  {key, sessions: [{origin, sessionId}]}  ⇒ 主窗口在看哪几个（同 Tauri 命令 `watch_sessions`）
 *   POST /__ccm/write  {key, origin, rel, text, append?}    ⇒ 场景在开页之后改那台盘上的一份文件（相对家目录；`append` ⇒ 接在末尾写）
 *   POST /__ccm/status {key, origin, sid, status, waitingFor?} ⇒ 场景演那一家改了自己 pidfile 里的状态（答了框 ⇒ busy）
 *   POST /__ccm/end    {key, origin}                         ⇒ 那台已结束 / 可重连的那几个这时收场（页里收到那台的清单之后发），见到裁出来的那一格才答
 *   POST /__ccm/kill   {key, origin}                         ⇒ 那台后端当场杀掉（演「断了」）
 *   GET  /__ccm/items?key=…                                  ⇒ 事件流（SSE）：一格一条 `{sub, items}`，同壳交给页的 `chan-items` 事件体
 *   POST /__ccm/trace  {scene}                               ⇒ 那个场景的世界里台架与无头壳之间的来往（最近 [`TRACE_KEEP`] 条，带毫秒）落进
 *                                                              `<沙箱>/trace/<场景>.log`，答那几份的路径（一张图没截成时 `run.mjs` 来要，问题表里点名它）
 *
 *   世界里的 `warm`（quota-warm 在跑）：同样配一个替身进程，写 `.cc-monitor/quota-warm.json` 带它的 pid。
 *   世界里的 `live`（活会话）：每个配一个 `sleep` 进程当 claude 的替身（缺省住在那台私有 tmux 的一个窗格里），写 `.claude/sessions/<pid>.json`
 *   （带 `procStart`，判活照真的那一套）；替身 · tmux · 后端同在一个沙箱里（见 `start`）。
 *
 * 隔离：每台后端跑在 bwrap 里 —— 家目录挂成 `/home/user`（界面上看到的路径与机器无关）、断网（`--unshare-net`）、主机名 `shots`、
 * `/tmp` 是空的；白名单环境（PATH · HOME · LANG · TZ · TMUX_TMPDIR · XDG_RUNTIME_DIR 都指沙箱）；不起 claude、不碰用户的 tmux。
 * 后端二进制：`CCM_SHOTS_BACKEND`，不给就用本树 `.build/backend-shots/debug/cc-monitor-backend`（带特性 `shots` 编）；无头壳：本树 `.build/shell/debug/examples/ccm-shots-shell`
 * （两样都由 [`buildBins`] 编：`run.mjs` 起 vite 前调；无头壳只在特性 `shots` 下编，发版构建里没有它）。
 */
import { spawn, spawnSync } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";

/** 同时留着的世界数（截图是一页一页串着截的；多留几份给预热页与下一页交接）。 */
/** 等一件事落定的上限：兜底的那一个数（与 `run.mjs` · `tests/test-support/wall-ceiling.ts` 同值）；等的是条件，到了就走。 */
export const WALL_CEILING_MS = 300_000;

const KEEP = 4;

/** 每个世界在内存里留最近几条来往（台架 ⇄ 无头壳：收发的每一行、页里交来的每一问），没截成时落盘。 */
const TRACE_KEEP = 4000;

/**
 * 截图里每一台的时区（IANA 名）：各台后端 · 无头壳 · 浏览器（`run.mjs` 起它时同样给）一律按它，不随跑截图那台机器的设置走。
 */
export const SHOTS_TZ = "Asia/Shanghai";

/**
 * 截图里的「现在」：每一页开头拨到这一刻（[`pinDateSource`]，经调试口在页里任何脚本之前装上），之后照真钟接着走；
 * 页里起世界时把它那一刻的 `Date.now()` 交来（`/__ccm/world` 的 `now`），各台后端从同一刻接着走
 * （带特性 `shots` 编的后端认 `CCM_SHOTS_NOW_MS`：`src/backend/common/time.rs::shots_clock`），盘上写的文件修改时刻也落在这一刻。
 * 取整分（14:30:00）：一张图从开页到截下来不到一分钟，钟面那一格不会跨分。
 */
export const SHOTS_NOW_MS = Date.parse("2026-10-08T14:30:00+08:00");

/** 页里拨钟的那一段（装在每一页的开头）：`Date.now()` 与无参的 `new Date()` 从 `now` 起照真钟接着走，别的照旧。 */
export function pinDateSource(now = SHOTS_NOW_MS) {
  return `(() => {
  const Real = Date;
  const shift = ${now} - Real.now();
  function D(...a) {
    if (!new.target) return new Real(Real.now() + shift).toString();
    return a.length === 0 ? new Real(Real.now() + shift) : new Real(...a);
  }
  D.prototype = Real.prototype;
  D.now = () => Real.now() + shift;
  D.parse = Real.parse;
  D.UTC = Real.UTC;
  Object.setPrototypeOf(D, Real);
  globalThis.Date = D;
})();`;
}

/** 台架那一份后端的构建目录（带特性 `shots`）。 */
const BACKEND_TARGET = (repo) => path.join(repo, ".build/backend-shots");

/** 无头壳那一份：壳的包里一个只在特性 `shots` 下编的例子（判据 `tests/frontend/shell/shots_feature_guard_tests.rs`）。 */
const SHELL_EXAMPLE = "ccm-shots-shell";

/**
 * 编真后端与无头壳（增量编）。`env` 是编译用的环境（要真 HOME 找工具链，别的已摘）。回失败的那一句，成了回 `null`。
 * 外面给了 `CCM_SHOTS_BACKEND` 就不编后端。
 */
export function buildBins({ repo, env }) {
  const jobs = { ...env, CARGO_BUILD_JOBS: env.CARGO_BUILD_JOBS ?? "4" };
  if (!process.env.CCM_SHOTS_BACKEND) {
    // 带特性 `shots`（拨钟那一段只在这一档里编），另落一个目录：不和判据 / e2e 用的那份默认构建互相覆盖。
    const b = spawnSync("cargo", ["build", "--quiet", "--features", "shots", "--target-dir", BACKEND_TARGET(repo)], { cwd: path.join(repo, "src/backend"), env: jobs, stdio: ["ignore", "inherit", "inherit"] });
    if (b.status !== 0) return `真后端没编出来（cargo 退出码 ${b.status}）`;
  }
  const s = spawnSync("cargo", ["build", "--quiet", "--features", "shots", "--example", SHELL_EXAMPLE], { cwd: path.join(repo, "src/frontend/shell"), env: jobs, stdio: ["ignore", "inherit", "inherit"] });
  if (s.status !== 0) return `无头壳没编出来（cargo 退出码 ${s.status}）`;
  return null;
}

/**
 * 沙箱外够那一台私有 tmux 的手（server 起在那台后端的沙箱里，见 `start`）：socket 在 `<dir>/tmux/tmux-<uid>/default`
 * （沙箱里挂成 `$TMUX_TMPDIR`，后端照常 `tmux` 就连到它，碰不到用户的 tmux）。每条命令显式 `-S` ＋ 摘掉 `TMUX` / `TMUX_PANE`、不读配置文件。
 */
function tmuxOf(dir) {
  const sock = path.join(dir, "tmux", `tmux-${process.getuid()}`, "default");
  return (args) => spawnSync("env", ["-u", "TMUX", "-u", "TMUX_PANE", "tmux", "-S", sock, "-f", "/dev/null", ...args], { stdio: "ignore", env: { PATH: "/usr/bin:/bin", LANG: "C.UTF-8" } }).status === 0;
}

export function backendPool({ repo, sandbox }) {
  const bin = process.env.CCM_SHOTS_BACKEND ?? path.join(BACKEND_TARGET(repo), "debug/cc-monitor-backend");
  const shellBin = path.join(repo, ".build/shell/debug/examples", SHELL_EXAMPLE);
  const worlds = new Map();

  function kill(key) {
    const w = worlds.get(key);
    if (!w) return;
    worlds.delete(key);
    for (const pid of [-w.shell.pid]) {
      try {
        process.kill(pid, "SIGKILL");
      } catch {
        // 已经退了
      }
    }
    for (const res of w.listeners) res.end();
    rmSync(w.dir, { recursive: true, force: true });
  }

  function start(dir, origin, m, now) {
    const home = path.join(dir, "home");
    mkdirSync(home, { recursive: true });
    for (const [rel, text] of Object.entries(m.files ?? {})) {
      if (rel.startsWith("/") || rel.split("/").includes("..")) throw new Error(`世界里的路径要相对家目录：${rel}`);
      const p = path.join(home, rel);
      mkdirSync(path.dirname(p), { recursive: true });
      writeFileSync(p, text);
      const at = m.mtimes?.[rel] ?? now;
      utimesSync(p, at / 1000, at / 1000);
    }
    // 替身进程（活会话的 claude · quota-warm）与这台的私有 tmux 都起在后端那一个沙箱里（同一个用户 / 进程命名空间）：
    // 后端要读替身的 `/proc/<pid>/environ` 取 `TMUX_PANE`（打标 `@ccm_sid`、报容器），沙箱外的进程它读不到；pid 也只在这一个命名空间里算数。
    // 沙箱里先跑一段起替身、写 pidfile 的开场（`.shots-setup/run.sh`），再 exec 后端。判活照真的那一套：pid 在不在 ＋ 进程起始时刻（`procStart`，/proc/<pid>/stat 第 22 格）。
    const H = "/home/user";
    const setup = path.join(home, ".shots-setup");
    mkdirSync(setup, { recursive: true });
    mkdirSync(path.join(home, ".claude", "sessions"), { recursive: true });
    mkdirSync(path.join(dir, "tmux", `tmux-${process.getuid()}`), { recursive: true, mode: 0o700 });
    mkdirSync(path.join(dir, "run"), { recursive: true, mode: 0o700 });
    const q = (v) => `'${String(v).replace(/'/g, `'\\''`)}'`;
    const sh = ["set -e", `T() { env -u TMUX -u TMUX_PANE tmux -S ${H}/.shots-tmux/tmux-${process.getuid()}/default -f /dev/null "$@"; }`];
    /**
     * 开页之后才收场的那几个（台架见到那台的清单之后收）：已结束 ⇒ 关掉它那个 tmux 会话（替身跟着没了）；
     * 可重连 ⇒ 只杀替身（claude 退了），窗格接着跑一个 shell、后端打过的标签还挂着。
     */
    const doomed = [];
    (m.live ?? []).forEach((l, k) => {
      const pane = `T new-session -d -s s${k} -x 120 -y 32 -P -F`;
      if (l.tmux === false && l.fate) throw new Error(`${origin} 上 ${l.sid}：不住 tmux 的替身演不了收场（台架从沙箱外够不着它）`);
      if (l.tmux === false) sh.push("sleep 3600 & pid=$!");
      else if (l.fate === "reconnectable") {
        // 替身是窗格里那个 shell 的子进程：它被杀之后窗格还在（shell 接着睡）。
        const pf = `${H}/.shots-setup/${k}.pid`;
        sh.push(`${pane} '#{pane_id}' ${q(`sleep 3600 & echo $! > ${pf}; wait; exec sleep 3600`)} >/dev/null`, `while [ ! -s ${pf} ]; do sleep 0.02; done`, `pid=$(cat ${pf})`);
        doomed.push({ sid: l.sid, fate: "reconnectable", act: ["run-shell", `kill $(cat ${pf})`] });
      } else sh.push(`pid=$(${pane} '#{pane_pid}' 'exec sleep 3600')`);
      if (l.fate === "ended") doomed.push({ sid: l.sid, fate: "ended", act: ["kill-session", "-t", `s${k}`] });
      const body = { pid: "__PID__", sessionId: l.sid, cwd: l.cwd, startedAt: l.startedAt ?? now, kind: l.kind ?? "interactive", procStart: "__PS__", ...(l.name ? { name: l.name } : {}), ...(l.status ? { status: l.status } : {}), ...(l.waitingFor ? { waitingFor: l.waitingFor } : {}), ...(l.statusUpdatedAt ? { statusUpdatedAt: l.statusUpdatedAt } : {}) };
      writeFileSync(path.join(setup, `${k}.json`), JSON.stringify(body).replace('"__PID__"', "__PID__"));
      sh.push(`ps=$(awk '{print $22}' /proc/$pid/stat)`, `sed "s/__PID__/$pid/; s/__PS__/$ps/" ${H}/.shots-setup/${k}.json > ${H}/.claude/sessions/$pid.json`);
    });
    if (m.warm) {
      mkdirSync(path.join(home, ".cc-monitor"), { recursive: true });
      writeFileSync(path.join(setup, "warm.json"), JSON.stringify({ pid: "__PID__", next: m.warm }).replace('"__PID__"', "__PID__"));
      sh.push("sleep 3600 & pid=$!", `sed "s/__PID__/$pid/" ${H}/.shots-setup/warm.json > ${H}/.cc-monitor/quota-warm.json`);
    }
    // 看的那一台的时区（同壳 attach 行带的 `tz`）：推来的钟面（记录的时刻字 · 子运行的起始字）按它写。
    sh.push(`exec /tmp/ccm-shots/cc-monitor-backend -- --stream --tz ${SHOTS_TZ}`);
    writeFileSync(path.join(setup, "run.sh"), sh.join("\n") + "\n");
    const env = {
      PATH: "/usr/bin:/bin",
      HOME: H,
      LANG: "C.UTF-8",
      TMUX_TMPDIR: `${H}/.shots-tmux`,
      XDG_RUNTIME_DIR: `${H}/.shots-run`,
      CCM_BACKEND_STDERR_LOG: process.env.CCM_SHOTS_DEBUG ? `${H}/.shots-stderr.log` : "/tmp/stderr.log",
      TZ: SHOTS_TZ,
      CCM_SHOTS_NOW_MS: String(Math.round(now)),
    };
    const box = [
      "--ro-bind", "/", "/",
      "--tmpfs", "/home",
      "--bind", home, H,
      "--bind", path.join(dir, "tmux"), `${H}/.shots-tmux`,
      "--bind", path.join(dir, "run"), `${H}/.shots-run`,
      "--tmpfs", "/tmp",
      "--ro-bind", bin, "/tmp/ccm-shots/cc-monitor-backend",
      "--dev", "/dev",
      // 自成一个进程命名空间：沙箱的头一个进程一退（壳杀那一台 · 台架收场），替身与 tmux 跟着全没。
      "--unshare-pid", "--proc", "/proc",
      "--unshare-net", "--unshare-uts", "--hostname", "shots", "--die-with-parent",
      "--chdir", H,
      "--clearenv",
      ...Object.entries(env).flatMap(([k, v]) => ["--setenv", k, v]),
      "/bin/sh", `${H}/.shots-setup/run.sh`,
    ];
    // 每台一条整命令（沙箱 ＋ 后端），交给无头壳去起（它拿着各台的标准输入输出，就像壳拿着本机后端那一条）。
    return { argv: ["bwrap", ...box], doomed, tmux: tmuxOf(dir) };
  }

  /** 无头壳：一行一个 JSON 进出（`shots_shell.rs` 头注）。 */
  function startShell(dir, w) {
    // 自成一个进程组：收场时整组杀（各台后端是它的子进程）。
    const child = spawn(shellBin, [], {
      detached: true,
      cwd: dir,
      env: { PATH: "/usr/bin:/bin", HOME: path.join(dir, "shell-home"), LANG: "C.UTF-8", TZ: SHOTS_TZ },
      stdio: ["pipe", "pipe", process.env.CCM_SHOTS_DEBUG ? "inherit" : "ignore"],
    });
    // 世界被收掉（`kill`）之后还有晚到的一格要回一句（如收场那一订的 `stop`）⇒ 往已经没了的壳写会 EPIPE：
    // 壳没了由下面 `exit` 那一支照实说（在等的问一律答「无头壳退了」），这里不让它变成整个 vite 进程的未处理异常。
    child.stdin.on("error", (e) => w.trace(`无头壳的标准输入写不进去：${e.message}`));
    const replies = new Map();
    let n = 0;
    let gone = null;
    const exited = new Promise((resolve) => {
      child.on("error", (e) => resolve((gone = e)));
      child.on("exit", (code) => {
        resolve((gone = new Error(`无头壳退了（${code}）`)));
        for (const r of replies.values()) r({ ok: false, fail: { err: { Ours: "Broken" }, body: [], detail: `无头壳退了（${code}）` } });
        replies.clear();
      });
      createInterface({ input: child.stdout, crlfDelay: Infinity }).on("line", (line) => {
        let f;
        try {
          f = JSON.parse(line);
        } catch {
          return;
        }
        if (f.t === "items") w.trace(`< sub=${f.sub} ${f.items.map((it) => (it.t === "frame" ? String(it.body ?? "").slice(0, 60) : it.t)).join(" | ").slice(0, 600)}`);
        else if (f.t !== "offer") w.trace(`< ${line.slice(0, 240)}`);
        if ((f.t === "reply" || f.t === "offer") && replies.has(f.n)) {
          const r = replies.get(f.n);
          replies.delete(f.n);
          r(f);
        } else if (f.t === "items" && w.inner.has(f.sub)) {
          w.inner.get(f.sub)(f.items);
        } else if (f.t === "items") {
          const msg = `data: ${JSON.stringify({ sub: f.sub, items: f.items })}\n\n`;
          if (w.listeners.length === 0) w.backlog.push(msg);
          for (const res of w.listeners) res.write(msg);
        }
      });
    });
    const send = (cmd) => {
      if (cmd.t !== "offer") w.trace(`> ${JSON.stringify(cmd).slice(0, 240)}`);
      child.stdin.write(JSON.stringify(cmd) + "\n");
    };
    const ask = (cmd) =>
      new Promise((resolve) => {
        const id = ++n;
        replies.set(id, resolve);
        send({ ...cmd, n: id });
      });
    const call = (origin, op, args, view) => ask({ t: "call", origin, op, payload: JSON.stringify(args ?? {}), view: view ?? null, left_ms: 30_000 });
    /** 各台都握上手（壳那一侧的入方向通道登记了）：隔一会儿问一次，30 秒上限。 */
    const up = async (origins) => {
      const until = Date.now() + 30_000;
      for (const origin of origins) {
        for (;;) {
          if (gone) throw gone;
          if ((await Promise.race([ask({ t: "offer", origin }), exited])).up === true) break;
          if (Date.now() > until) throw new Error(`${origin} 的后端 30 秒没握上手`);
          await new Promise((r) => setTimeout(r, 20));
        }
      }
    };
    return { child, up, send, call };
  }

  /** 台架自己订的那几条（编号从这里往上，页里的订阅编号小得多）。 */
  let innerId = 1 << 30;

  /**
   * 收场的那几个（`/__ccm/end {origin}`：页里收到那台的清单之后发）：开页时它们还活着，这时才收场 ⇒ 后端照真的那一套裁「已结束」/「可重连」
   * （会话账本：摘除 ＋ 之后那一份 tmux 观测里挂不挂它），页上那张卡照真的那样变。台架自己订一条只看它的会话流（`session-lines/<sid>`，订上当场重放）：
   * 见到宣告才收场（关它那个 tmux 会话 / 只杀替身），见到裁出来的那一格才答。
   */
  async function endAll(body) {
    const w = worlds.get(body.key);
    if (!w) return;
    const mine = w.doomed.filter((d) => d.origin === body.origin);
    w.doomed = w.doomed.filter((d) => d.origin !== body.origin);
    await Promise.all(mine.map((d) => endOne(w, d)));
  }

  /** 一个收场的会话：等它的宣告 ⇒ 收场 ⇒ 等裁出来的那一格（上限是兜底的那个数）。 */
  function endOne(w, d) {
    const id = innerId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`${d.origin} 上 ${d.sid} 等到上限还没收场`)), WALL_CEILING_MS);
      let killed = false;
      w.inner.set(id, (items) => {
        for (const it of items) {
          if (it.t !== "frame") continue;
          const f = JSON.parse(it.body);
          if (!killed && f.live?.session_id === d.sid) {
            killed = true;
            if (!d.act()) reject(new Error(`${d.origin} 上 ${d.sid} 的替身关不掉`));
          } else if (killed && (d.fate === "ended" ? f.ended : f.idle)?.session_id === d.sid) {
            clearTimeout(timer);
            w.inner.delete(id);
            w.sh.send({ t: "stop", id });
            resolve();
          }
        }
      });
      w.sh.send({ t: "sub", id, origin: d.origin, kind: `session-lines/${d.sid}`, want: 1 << 20 });
    });
  }

  async function makeWorld(body) {
    if (!existsSync(bin)) throw new Error(`真后端二进制不在：${bin}（先 cd src/backend && cargo build）`);
    if (!existsSync(shellBin)) throw new Error(`无头壳不在：${shellBin}（先 cd src/frontend/shell && cargo build --features shots --example ${SHELL_EXAMPLE}）`);
    const { key, machines } = body;
    // 页里那一刻（页开头拨过钟）；没给 ⇒ 台架那一刻。
    const now = Number.isFinite(body.now) ? body.now : SHOTS_NOW_MS;
    kill(key);
    const dir = path.join(sandbox, "worlds", key.replace(/[^A-Za-z0-9_.-]/g, "_"));
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(path.join(dir, "shell-home"), { recursive: true });
    // 这屋子的钟比台架这一侧快多少（开页之后改的文件，修改时刻照这屋子的钟落）。
    const t0 = Date.now();
    const lines = [];
    const trace = (t) => {
      lines.push(`${String(Date.now() - t0).padStart(7)} ${t}`);
      if (lines.length > TRACE_KEEP) lines.splice(0, lines.length - TRACE_KEEP);
    };
    const w = { dir, homes: new Map(), doomed: [], inner: new Map(), listeners: [], backlog: [], shell: null, shift: now - Date.now(), scene: String(body.scene ?? ""), lines, trace };
    worlds.set(key, w);
    const list = Object.entries(machines).map(([origin, m], i) => {
      w.homes.set(origin, path.join(dir, `m${i}`, "home"));
      const { argv, doomed, tmux } = start(path.join(dir, `m${i}`), origin, m, now);
      w.doomed.push(...doomed.map((d) => ({ ...d, origin, act: () => tmux(d.act) })));
      return { origin, argv };
    });
    const sh = startShell(dir, w);
    w.shell = sh.child;
    w.sh = sh;
    sh.send({ t: "machines", machines: list });
    await sh.up(list.map((m) => m.origin));
    while (worlds.size > KEEP) kill(worlds.keys().next().value);
  }

  /** 那个场景还留着的世界：各自的来往落盘（同一场景几个世界 ⇒ 几份，按起的先后编号）。 */
  function dumpTrace(scene) {
    const mine = [...worlds.values()].filter((w) => w.scene === scene);
    const dir = path.join(sandbox, "trace");
    mkdirSync(dir, { recursive: true });
    return mine.map((w, i) => {
      const file = path.join(dir, `${scene.replace(/[^A-Za-z0-9_.-]/g, "_")}${mine.length > 1 ? `.${i}` : ""}.log`);
      writeFileSync(file, w.lines.join("\n") + "\n");
      return file;
    });
  }

  async function call(body) {
    const w = worlds.get(body.key);
    if (!w) return { ok: false, fail: { err: { Ours: "Misuse" }, body: [], detail: `没有这个世界：${body.key}` } };
    return w.sh.call(body.origin, body.op, body.args, body.view);
  }

  /** 页里的订阅那几样原样交给无头壳（窗口只有一扇）。 */
  function tell(url, body) {
    const w = worlds.get(body.key);
    if (!w) return;
    w.trace(`http ${url}`);
    if (url === "/__ccm/sub") w.sh.send({ t: "sub", id: body.id, origin: body.origin, kind: body.kind, want: body.want });
    else if (url === "/__ccm/want") w.sh.send({ t: "want", id: body.id, more: body.more });
    else if (url === "/__ccm/stop") w.sh.send({ t: "stop", id: body.id });
    else if (url === "/__ccm/ready") w.sh.send({ t: "ready", priority_sid: body.priority_sid ?? null });
    else if (url === "/__ccm/kill") w.sh.send({ t: "kill", origin: body.origin });
    else if (url === "/__ccm/watch") w.sh.send({ t: "watch", sessions: body.sessions });
    else if (url === "/__ccm/status") {
      // 那一家改了自己 pidfile 里的状态（答了框 ⇒ 接着跑）：找到那个会话的那一份，改状态那几格（原词）。
      const dir = path.join(w.homes.get(body.origin) ?? "", ".claude", "sessions");
      for (const f of existsSync(dir) ? readdirSync(dir) : []) {
        const p = path.join(dir, f);
        const v = JSON.parse(readFileSync(p, "utf8"));
        if (v.sessionId !== body.sid) continue;
        v.status = String(body.status);
        if (body.waitingFor) v.waitingFor = String(body.waitingFor);
        else delete v.waitingFor;
        v.statusUpdatedAt = Date.now() + w.shift;
        writeFileSync(p, JSON.stringify(v));
      }
    } else if (url === "/__ccm/write") {
      const home = w.homes.get(body.origin);
      const rel = String(body.rel);
      if (!home || rel.startsWith("/") || rel.split("/").includes("..")) throw new Error(`改不了：${body.origin} · ${rel}`);
      mkdirSync(path.dirname(path.join(home, rel)), { recursive: true });
      // 记录文件往后长（会话又写了几行）⇒ 接在末尾写，不截断重写（watcher 读到的是一份变短又变长的文件，会从头再读）。
      if (body.append) appendFileSync(path.join(home, rel), String(body.text));
      else writeFileSync(path.join(home, rel), String(body.text));
      const at = (Date.now() + w.shift) / 1000;
      utimesSync(path.join(home, rel), at, at);
    }
  }

  const readBody = (req) =>
    new Promise((resolve, reject) => {
      let s = "";
      req.setEncoding("utf8");
      req.on("data", (d) => (s += d));
      req.on("end", () => resolve(s));
      req.on("error", reject);
    });

  const TELL = new Set(["/__ccm/sub", "/__ccm/want", "/__ccm/stop", "/__ccm/ready", "/__ccm/kill", "/__ccm/watch", "/__ccm/write", "/__ccm/status"]);

  async function serve(req, res, next) {
    if (req.method === "GET" && req.url?.startsWith("/__ccm/items?")) {
      const w = worlds.get(new URL(req.url, "http://x").searchParams.get("key"));
      if (!w) {
        res.statusCode = 404;
        return res.end();
      }
      res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
      res.write(":\n\n");
      for (const msg of w.backlog.splice(0)) res.write(msg);
      w.listeners.push(res);
      req.on("close", () => (w.listeners = w.listeners.filter((x) => x !== res)));
      return;
    }
    if (req.method !== "POST" || !(req.url === "/__ccm/world" || req.url === "/__ccm/call" || req.url === "/__ccm/end" || req.url === "/__ccm/trace" || TELL.has(req.url))) return next();
    try {
      const body = JSON.parse(await readBody(req));
      let out = { ok: true };
      if (req.url === "/__ccm/world") await makeWorld(body);
      else if (req.url === "/__ccm/call") out = await call(body);
      else if (req.url === "/__ccm/end") {
        worlds.get(body.key)?.trace(`http /__ccm/end ${body.origin}`);
        await endAll(body);
        worlds.get(body.key)?.trace(`http /__ccm/end ${body.origin} 收完了`);
      } else if (req.url === "/__ccm/trace") out = { files: dumpTrace(String(body.scene ?? "")) };
      else tell(req.url, body);
      res.setHeader("content-type", "application/json");
      res.end(JSON.stringify(out));
    } catch (e) {
      res.statusCode = 500;
      res.end(JSON.stringify({ error: e instanceof Error ? e.message : String(e) }));
    }
  }

  return {
    /** vite 插件：两个口挂在开发服务器与预览服务器上（截图工具用前者，性能台架用后者）。 */
    plugin: {
      name: "ccm-shots-real-backend",
      configureServer(server) {
        server.middlewares.use(serve);
      },
      configurePreviewServer(server) {
        server.middlewares.use(serve);
      },
    },
    close() {
      for (const k of [...worlds.keys()]) kill(k);
    },
  };
}
