/**
 * 截图台架的真后端池：每个场景一屋子盘上原始格式的家目录（页里交来的 `DiskWorld`），每台机器一个真后端进程（stdio 载体），
 * 页里的帧命令经 vite 的这两个口原样问它。成品（额度行 · 时刻字 · 状态字 …）一律是真后端算的，台架不写。
 *
 *   POST /__ccm/world  {key, machines: {<origin>: {files: {<相对家目录的路径>: <内容>}}}}  ⇒ 造好家目录、起后端、读到 hello 再答
 *   POST /__ccm/call   {key, origin, op, args}                                             ⇒ 那台后端的应答帧原样
 *
 *   世界里的 `warm`（quota-warm 在跑）：同样配一个替身进程，写 `.cc-monitor/quota-warm.json` 带它的 pid。
 *   世界里的 `live`（活会话）：每个配一个 `sleep` 进程当 claude 的替身，写 `.claude/sessions/<pid>.json`（带 `procStart`，判活照真的那一套）。
 *
 * 隔离：每台后端跑在 bwrap 里 —— 家目录挂成 `/home/user`（界面上看到的路径与机器无关）、断网（`--unshare-net`）、主机名 `shots`、
 * `/tmp` 是空的；白名单环境（PATH · HOME · LANG · TZ · TMUX_TMPDIR · XDG_RUNTIME_DIR 都指沙箱）；不起 claude、不碰用户的 tmux。
 * 后端二进制：`CCM_SHOTS_BACKEND`，不给就用本树 `.build/backend/debug/cc-monitor-backend`（`run.mjs` 起 vite 前先编好）。
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";

/** 同时留着的世界数（截图是一页一页串着截的；多留几份给预热页与下一页交接）。 */
const KEEP = 4;

export function backendPool({ repo, sandbox }) {
  const bin = process.env.CCM_SHOTS_BACKEND ?? path.join(repo, ".build/backend/debug/cc-monitor-backend");
  const worlds = new Map();

  function kill(key) {
    const w = worlds.get(key);
    if (!w) return;
    worlds.delete(key);
    for (const m of w.machines.values()) {
      for (const pid of [-m.child.pid, ...m.child.sleepers.map((c) => c.pid)]) {
        try {
          process.kill(pid, "SIGKILL");
        } catch {
          // 已经退了
        }
      }
    }
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
      ...(process.env.TZ ? { TZ: process.env.TZ } : {}),
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
    // 自成一个进程组：收场时整组杀（bwrap 先死时 `--die-with-parent` 不一定带得走后端）。
    const child = spawn("bwrap", box, { detached: true, env: { PATH: "/usr/bin:/bin" }, stdio: ["pipe", "pipe", process.env.CCM_SHOTS_DEBUG ? "inherit" : "ignore"] });
    child.sleepers = sleepers;
    const waiting = new Map();
    let nid = 0;
    const hello = new Promise((resolve, reject) => {
      child.on("error", reject);
      child.on("exit", (code) => {
        reject(new Error(`${origin} 的后端退了（${code}）`));
        for (const w of waiting.values()) w({ kind: "reply", ok: false, code: "shots_backend_gone", message: `后端退了（${code}）`, detail: "" });
        waiting.clear();
      });
      createInterface({ input: child.stdout, crlfDelay: Infinity }).on("line", (line) => {
        let f;
        try {
          f = JSON.parse(line);
        } catch {
          return;
        }
        if (f.kind === "hello") resolve(f);
        if (f.kind === "reply" && waiting.has(f.id)) {
          const w = waiting.get(f.id);
          waiting.delete(f.id);
          w(f);
        }
      });
    });
    const call = (cmd, args) =>
      new Promise((resolve) => {
        const id = `s${++nid}`;
        waiting.set(id, resolve);
        child.stdin.write(JSON.stringify({ id, cmd, args: args ?? null, within_ms: 30_000 }) + "\n");
      });
    return { child, hello, call };
  }

  async function makeWorld(body) {
    if (!existsSync(bin)) throw new Error(`真后端二进制不在：${bin}（先 cd src/backend && cargo build）`);
    const { key, machines } = body;
    kill(key);
    const dir = path.join(sandbox, "worlds", key.replace(/[^A-Za-z0-9_.-]/g, "_"));
    rmSync(dir, { recursive: true, force: true });
    const w = { dir, machines: new Map() };
    worlds.set(key, w);
    Object.entries(machines).forEach(([origin, m], i) => {
      w.machines.set(origin, start(path.join(dir, `m${i}`), origin, m));
    });
    await Promise.all([...w.machines.values()].map((m) => m.hello));
    while (worlds.size > KEEP) kill(worlds.keys().next().value);
  }

  async function call(body) {
    const w = worlds.get(body.key);
    const m = w?.machines.get(body.origin);
    if (!m) return { kind: "reply", ok: false, code: "shots_no_world", message: `没有这台：${body.origin}`, detail: "" };
    return m.call(body.op, body.args);
  }

  const readBody = (req) =>
    new Promise((resolve, reject) => {
      let s = "";
      req.setEncoding("utf8");
      req.on("data", (d) => (s += d));
      req.on("end", () => resolve(s));
      req.on("error", reject);
    });

  async function serve(req, res, next) {
    if (req.method !== "POST" || !(req.url === "/__ccm/world" || req.url === "/__ccm/call")) return next();
    try {
      const body = JSON.parse(await readBody(req));
      const out = req.url === "/__ccm/world" ? (await makeWorld(body), { ok: true }) : await call(body);
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
