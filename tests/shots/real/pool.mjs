/**
 * 截图台架的真后端池：每个场景一屋子盘上原始格式的家目录（页里交来的 `DiskWorld`），每台机器一个真后端进程（stdio 载体），
 * 前面站着壳的真代码（无头壳 `ccm-shots-shell`：`src/frontend/shell/src/shots_shell.rs`，本机那条读循环 → 会话账 → 重放缓冲 → 通道交格）。
 * 页里的帧命令与订阅经 vite 的这几个口原样交给无头壳。成品（额度行 · 时刻字 · 状态字 · 拒答的复制详情 …）一律是真后端与壳算的，台架不写。
 *
 *   POST /__ccm/world  {key, machines: {<origin>: {files: {<相对家目录的路径>: <内容>}}}}  ⇒ 造好家目录、起无头壳与各台后端、全部握上手再答
 *   POST /__ccm/call   {key, origin, op, args}                ⇒ 壳 `chan_call` 那一跳的结局：`{ok:true, body}` / `{ok:false, fail:{err, body, detail}}`
 *   POST /__ccm/sub    {key, id, origin, kind, want} · /__ccm/want {key, id, more} · /__ccm/stop {key, id} · /__ccm/ready {key, priority_sid}
 *   POST /__ccm/write  {key, origin, rel, text}             ⇒ 场景在开页之后改那台盘上的一份文件（相对家目录）
 *   GET  /__ccm/items?key=…                                  ⇒ 事件流（SSE）：一格一条 `{sub, items}`，同壳交给页的 `chan-items` 事件体
 *
 *   世界里的 `warm`（quota-warm 在跑）：同样配一个替身进程，写 `.cc-monitor/quota-warm.json` 带它的 pid。
 *   世界里的 `live`（活会话）：每个配一个 `sleep` 进程当 claude 的替身，写 `.claude/sessions/<pid>.json`（带 `procStart`，判活照真的那一套）。
 *
 * 隔离：每台后端跑在 bwrap 里 —— 家目录挂成 `/home/user`（界面上看到的路径与机器无关）、断网（`--unshare-net`）、主机名 `shots`、
 * `/tmp` 是空的；白名单环境（PATH · HOME · LANG · TZ · TMUX_TMPDIR · XDG_RUNTIME_DIR 都指沙箱）；不起 claude、不碰用户的 tmux。
 * 后端二进制：`CCM_SHOTS_BACKEND`，不给就用本树 `.build/backend/debug/cc-monitor-backend`；无头壳：本树 `.build/shell/debug/examples/ccm-shots-shell`
 * （两样都由 [`buildBins`] 编：`run.mjs` 起 vite 前调；无头壳只在特性 `shots` 下编，发版构建里没有它）。
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";

/** 同时留着的世界数（截图是一页一页串着截的；多留几份给预热页与下一页交接）。 */
const KEEP = 4;

/**
 * 截图里每一台的时区（IANA 名）：各台后端 · 无头壳 · 浏览器（`run.mjs` 起它时同样给）一律按它，不随跑截图那台机器的设置走。
 * 「现在」还没给死：后端那一侧没有假钟的入口（要核心补），页面单方面拨钟会和后端算的「距今」对不上。
 */
export const SHOTS_TZ = "Asia/Shanghai";

/** 无头壳那一份：壳的包里一个只在特性 `shots` 下编的例子（判据 `tests/frontend/shell/shots_feature_guard_tests.rs`）。 */
const SHELL_EXAMPLE = "ccm-shots-shell";

/**
 * 编真后端与无头壳（增量编）。`env` 是编译用的环境（要真 HOME 找工具链，别的已摘）。回失败的那一句，成了回 `null`。
 * 外面给了 `CCM_SHOTS_BACKEND` 就不编后端。
 */
export function buildBins({ repo, env }) {
  const jobs = { ...env, CARGO_BUILD_JOBS: env.CARGO_BUILD_JOBS ?? "4" };
  if (!process.env.CCM_SHOTS_BACKEND) {
    const b = spawnSync("cargo", ["build", "--quiet"], { cwd: path.join(repo, "src/backend"), env: jobs, stdio: ["ignore", "inherit", "inherit"] });
    if (b.status !== 0) return `真后端没编出来（cargo 退出码 ${b.status}）`;
  }
  const s = spawnSync("cargo", ["build", "--quiet", "--features", "shots", "--example", SHELL_EXAMPLE], { cwd: path.join(repo, "src/frontend/shell"), env: jobs, stdio: ["ignore", "inherit", "inherit"] });
  if (s.status !== 0) return `无头壳没编出来（cargo 退出码 ${s.status}）`;
  return null;
}

export function backendPool({ repo, sandbox }) {
  const bin = process.env.CCM_SHOTS_BACKEND ?? path.join(repo, ".build/backend/debug/cc-monitor-backend");
  const shellBin = path.join(repo, ".build/shell/debug/examples", SHELL_EXAMPLE);
  const worlds = new Map();

  function kill(key) {
    const w = worlds.get(key);
    if (!w) return;
    worlds.delete(key);
    for (const pid of [-w.shell.pid, ...w.sleepers.map((c) => c.pid)]) {
      try {
        process.kill(pid, "SIGKILL");
      } catch {
        // 已经退了
      }
    }
    for (const res of w.listeners) res.end();
    rmSync(w.dir, { recursive: true, force: true });
  }

  function start(dir, origin, m) {
    const home = path.join(dir, "home");
    mkdirSync(home, { recursive: true });
    for (const [rel, text] of Object.entries(m.files ?? {})) {
      if (rel.startsWith("/") || rel.split("/").includes("..")) throw new Error(`世界里的路径要相对家目录：${rel}`);
      const p = path.join(home, rel);
      mkdirSync(path.dirname(p), { recursive: true });
      writeFileSync(p, text);
    }
    // 活会话的替身进程：判活看 pid 在不在 ＋ 进程起始时刻对不对得上（`procStart`，/proc/<pid>/stat 第 22 格）。
    const sleepers = [];
    for (const l of m.live ?? []) {
      const s = spawn("sleep", ["3600"], { stdio: "ignore", env: { PATH: "/usr/bin:/bin" } });
      sleepers.push(s);
      const st = readFileSync(`/proc/${s.pid}/stat`, "utf8");
      const procStart = st.slice(st.lastIndexOf(")") + 2).split(" ")[19];
      const p = path.join(home, ".claude", "sessions", `${s.pid}.json`);
      mkdirSync(path.dirname(p), { recursive: true });
      writeFileSync(p, JSON.stringify({ pid: s.pid, sessionId: l.sid, cwd: l.cwd, startedAt: l.startedAt ?? Date.now(), kind: l.kind ?? "interactive", procStart, ...(l.status ? { status: l.status } : {}), ...(l.waitingFor ? { waitingFor: l.waitingFor } : {}), ...(l.statusUpdatedAt ? { statusUpdatedAt: l.statusUpdatedAt } : {}) }));
    }
    if (m.warm) {
      const s = spawn("sleep", ["3600"], { stdio: "ignore", env: { PATH: "/usr/bin:/bin" } });
      sleepers.push(s);
      mkdirSync(path.join(home, ".cc-monitor"), { recursive: true });
      writeFileSync(path.join(home, ".cc-monitor", "quota-warm.json"), JSON.stringify({ pid: s.pid, next: m.warm }));
    }
    mkdirSync(path.join(dir, "tmux"), { recursive: true, mode: 0o700 });
    mkdirSync(path.join(dir, "run"), { recursive: true, mode: 0o700 });
    const H = "/home/user";
    const env = {
      PATH: "/usr/bin:/bin",
      HOME: H,
      LANG: "C.UTF-8",
      TMUX_TMPDIR: `${H}/.shots-tmux`,
      XDG_RUNTIME_DIR: `${H}/.shots-run`,
      CCM_BACKEND_STDERR_LOG: "/tmp/stderr.log",
      TZ: SHOTS_TZ,
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
      "--unshare-net", "--unshare-uts", "--hostname", "shots", "--die-with-parent",
      "--chdir", H,
      "--clearenv",
      ...Object.entries(env).flatMap(([k, v]) => ["--setenv", k, v]),
      "/tmp/ccm-shots/cc-monitor-backend", "--", "--stream", "--tail-only", "--with-bg", "--with-pid",
    ];
    // 每台一条整命令（沙箱 ＋ 后端），交给无头壳去起（它拿着各台的标准输入输出，就像壳拿着本机后端那一条）。
    return { argv: ["bwrap", ...box], sleepers };
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
        if ((f.t === "reply" || f.t === "offer") && replies.has(f.n)) {
          const r = replies.get(f.n);
          replies.delete(f.n);
          r(f);
        } else if (f.t === "items") {
          const msg = `data: ${JSON.stringify({ sub: f.sub, items: f.items })}\n\n`;
          if (w.listeners.length === 0) w.backlog.push(msg);
          for (const res of w.listeners) res.write(msg);
        }
      });
    });
    const send = (cmd) => child.stdin.write(JSON.stringify(cmd) + "\n");
    const ask = (cmd) =>
      new Promise((resolve) => {
        const id = ++n;
        replies.set(id, resolve);
        send({ ...cmd, n: id });
      });
    const call = (origin, op, args) => ask({ t: "call", origin, op, payload: JSON.stringify(args ?? {}), left_ms: 30_000 });
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

  async function makeWorld(body) {
    if (!existsSync(bin)) throw new Error(`真后端二进制不在：${bin}（先 cd src/backend && cargo build）`);
    if (!existsSync(shellBin)) throw new Error(`无头壳不在：${shellBin}（先 cd src/frontend/shell && cargo build --features shots --example ${SHELL_EXAMPLE}）`);
    const { key, machines } = body;
    kill(key);
    const dir = path.join(sandbox, "worlds", key.replace(/[^A-Za-z0-9_.-]/g, "_"));
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(path.join(dir, "shell-home"), { recursive: true });
    const w = { dir, homes: new Map(), sleepers: [], listeners: [], backlog: [], shell: null };
    worlds.set(key, w);
    const list = Object.entries(machines).map(([origin, m], i) => {
      w.homes.set(origin, path.join(dir, `m${i}`, "home"));
      const { argv, sleepers } = start(path.join(dir, `m${i}`), origin, m);
      w.sleepers.push(...sleepers);
      return { origin, argv };
    });
    const sh = startShell(dir, w);
    w.shell = sh.child;
    w.sh = sh;
    sh.send({ t: "machines", machines: list });
    await sh.up(list.map((m) => m.origin));
    while (worlds.size > KEEP) kill(worlds.keys().next().value);
  }

  async function call(body) {
    const w = worlds.get(body.key);
    if (!w) return { ok: false, fail: { err: { Ours: "Misuse" }, body: [], detail: `没有这个世界：${body.key}` } };
    return w.sh.call(body.origin, body.op, body.args);
  }

  /** 页里的订阅那几样原样交给无头壳（窗口只有一扇）。 */
  function tell(url, body) {
    const w = worlds.get(body.key);
    if (!w) return;
    if (url === "/__ccm/sub") w.sh.send({ t: "sub", id: body.id, origin: body.origin, kind: body.kind, want: body.want });
    else if (url === "/__ccm/want") w.sh.send({ t: "want", id: body.id, more: body.more });
    else if (url === "/__ccm/stop") w.sh.send({ t: "stop", id: body.id });
    else if (url === "/__ccm/ready") w.sh.send({ t: "ready", priority_sid: body.priority_sid ?? null });
    else if (url === "/__ccm/kill") w.sh.send({ t: "kill", origin: body.origin });
    else if (url === "/__ccm/write") {
      const home = w.homes.get(body.origin);
      const rel = String(body.rel);
      if (!home || rel.startsWith("/") || rel.split("/").includes("..")) throw new Error(`改不了：${body.origin} · ${rel}`);
      mkdirSync(path.dirname(path.join(home, rel)), { recursive: true });
      writeFileSync(path.join(home, rel), String(body.text));
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

  const TELL = new Set(["/__ccm/sub", "/__ccm/want", "/__ccm/stop", "/__ccm/ready", "/__ccm/kill", "/__ccm/write"]);

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
    if (req.method !== "POST" || !(req.url === "/__ccm/world" || req.url === "/__ccm/call" || TELL.has(req.url))) return next();
    try {
      const body = JSON.parse(await readBody(req));
      let out = { ok: true };
      if (req.url === "/__ccm/world") await makeWorld(body);
      else if (req.url === "/__ccm/call") out = await call(body);
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
