/**
 * 截图台架那个按口转发的假 ssh（`real/pool.mjs::forwarderSh`，本机沙箱里 sshd 的登录壳）与它交本机后端的那张机器表（`reachTable`）。
 *
 * - 按连进来的口认台：第 k 台 ⇒ 那台常驻的套接字目录（`CCM_DATA_DIR`），产品单引号过的那几格原样切回来、stdin 那一行原样交过去。
 * - 只转问常驻那一形：别的命令 · 不认的口一律拒（台架那几台之外谁也够不着）。
 * - 场景演「那一跳断开」的那几条（`refuse`）：一个字不回、退 255，别的命令照转。
 * - 机器表照场景整张交（`fake/world.ts` 写的那份）：问不到的那台（从没见过 · 会话流被关）照样在表里、停用开关原样带过去；
 *   问得到的那几台拨的口就是假 ssh 听的口，口取自场景表那一行（A10-17：从前只交问得到的那几台、口现编 ⇒ 从没见过的那台掉出设置页列表）。
 * - 场景 `~/.ssh/config` 里 devbox 与表里那台同址（后端据地址 · 用户 · 口标「已在列表」；不同址 ⇒ 导入框里 devbox 可勾可改名，撞名判错）。
 *
 * 买不到：sshd 本身与沙箱里那一跳（`--resident-call` 连常驻）不在这里起；台架里每台问没问到由截图那一趟的问题表判（`fake/backend.ts::judgeAsked`）。
 */
import { spawn, spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";
import { defaultWorld, diskOf, LOCAL, REMOTES } from "./fake/world";
import { reachOf } from "./fake/backend";
import { HOST_KEYS } from "./fake/machine";
import { UNTIL_CEILING } from "../test-support/wall-ceiling";

const POOL = "./real/pool.mjs";
type Peer = { port: number; k: number; refuse?: string[]; fake?: "older" | "hang" | "install" };
type Row = { label: string; host: string; port: number; user: string; keyPath: string; connect: boolean };
const { forwarderSh, reachTable, DOER_PY } = (await import(POOL)) as {
  DOER_PY: string;
  forwarderSh: (peers: Peer[], backend?: string, slowSftp?: string, doer?: string) => string;
  reachTable: (table: string | undefined, reach: [string, { refuse: string[]; fake?: Peer["fake"] }][]) => { hosts: Row[]; peers: (Peer & { origin: string })[] };
};
/** 转发那几格用的口（任意，只要转发表里写的是它）。 */
const PEER_PORT_BASE = 2201;

/** 产品那一形的命令行（`remote_ask::command_line`：落点 ＋ 逐格单引号）。 */
const sq = (a: string): string => "'" + a.split("'").join("'\\''") + "'";
const productLine = (...argv: string[]): string => '"$HOME"/.cc-monitor/bin/ccm -- ' + argv.map(sq).join(" ");

describe.skipIf(process.platform === "win32")("按口转发的假 ssh", () => {
  const run = (peers: Peer[], port: number, command: string, stdin: string) => {
    const dir = mkdtempSync(path.join(os.tmpdir(), "shots-forwarder-"));
    try {
      // 后端的替身：印它拿到的 CCM_DATA_DIR · 参数（一格一行）· stdin。
      const backend = path.join(dir, "backend");
      writeFileSync(backend, '#!/bin/sh\necho "dir=$CCM_DATA_DIR"\nfor a in "$@"; do echo "arg=$a"; done\necho "in=$(cat)"\n');
      chmodSync(backend, 0o755);
      const fwd = path.join(dir, "forward.sh");
      writeFileSync(fwd, forwarderSh(peers, backend));
      chmodSync(fwd, 0o755);
      return spawnSync(fwd, [], {
        input: stdin,
        encoding: "utf8",
        env: { PATH: "/usr/bin:/bin", HOME: "/home/user", SSH_CONNECTION: `127.0.0.1 40000 127.0.0.1 ${port}`, SSH_ORIGINAL_COMMAND: command },
      });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };
  const peers: Peer[] = [
    { port: PEER_PORT_BASE, k: 0 },
    { port: PEER_PORT_BASE + 1, k: 1, refuse: ["sessions-needs"] },
  ];
  const ask = (cmd: string) => `${JSON.stringify({ cmd, args: {} })}\n`;
  const line = productLine("--resident-call", "--stdin-line", "--within-ms", "2900", "--tz", "Asia/Shanghai");

  it("按口认台、换成那台常驻的目录，参数原样切回、stdin 原样交", () => {
    const r = run(peers, PEER_PORT_BASE + 1, line, ask("history-search"));
    expect(r.status).toBe(0);
    expect(r.stdout.trim().split("\n")).toEqual([
      "dir=/tmp/shots-peers/1",
      "arg=--",
      "arg=--resident-call",
      "arg=--stdin-line",
      "arg=--within-ms",
      "arg=2900",
      "arg=--tz",
      "arg=Asia/Shanghai",
      `in=${ask("history-search").trim()}`,
    ]);
  });

  it("那台演断开的那一条：一个字不回、退 255；别台同一条照转", () => {
    const down = run(peers, PEER_PORT_BASE + 1, line, ask("sessions-needs"));
    expect([down.status, down.stdout]).toEqual([255, ""]);
    expect(run(peers, PEER_PORT_BASE, line, ask("sessions-needs")).stdout).toContain("dir=/tmp/shots-peers/0");
  });

  /** 常驻那几问的产品写法（`resident_client::feed::backend_command`：落点 ＋ 一个常量词，不加引号）。 */
  const word = (w: string): string => `"$HOME"/.cc-monitor/bin/ccm -- ${w}`;
  /** 接常驻那一跳的替身：同常驻那一形，先 hello、再 attach 回话（同一版），再把 stdin 原样回过去。 */
  const attaching = `#!/bin/sh\necho "dir=$CCM_DATA_DIR $*" >&2\necho '{"hello":{"relation":"same"}}'\necho '{"attach":"ok","relation":"same"}'\ncat\n`;
  const runWith = (peers: Peer[], port: number, command: string, stdin: string, backendSh: string, timeout?: number) => {
    const dir = mkdtempSync(path.join(os.tmpdir(), "shots-forwarder-"));
    try {
      const backend = path.join(dir, "backend");
      writeFileSync(backend, backendSh);
      chmodSync(backend, 0o755);
      const slow = path.join(dir, "slow-sftp");
      writeFileSync(slow, '#!/bin/sh\necho "slow-sftp $*"\n');
      chmodSync(slow, 0o755);
      const doer = path.join(dir, "doer");
      writeFileSync(doer, '#!/bin/sh\necho "doer $*"\n');
      chmodSync(doer, 0o755);
      const fwd = path.join(dir, "forward.sh");
      writeFileSync(fwd, forwarderSh(peers, backend, slow, doer));
      chmodSync(fwd, 0o755);
      return spawnSync(fwd, [], {
        input: stdin,
        encoding: "utf8",
        timeout,
        env: { PATH: "/usr/bin:/bin", HOME: "/home/user", SSH_CONNECTION: `127.0.0.1 40000 127.0.0.1 ${port}`, SSH_ORIGINAL_COMMAND: command },
      });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };
  const fakes: Peer[] = [
    { port: PEER_PORT_BASE, k: 0 },
    { port: PEER_PORT_BASE + 1, k: 1, fake: "older" },
    { port: PEER_PORT_BASE + 2, k: 2, fake: "hang" },
    { port: PEER_PORT_BASE + 3, k: 3, fake: "install" },
  ];

  it("接常驻（本机后端那条常驻流）：按口换成那台常驻的目录，回话 · 流原样交", () => {
    const r = runWith(fakes, PEER_PORT_BASE, word("--resident-attach"), '{"attach":true}\n', attaching);
    expect(r.status).toBe(0);
    expect(r.stderr).toContain("dir=/tmp/shots-peers/0 -- --resident-attach");
    expect(r.stdout.split("\n")).toEqual(['{"hello":{"relation":"same"}}', '{"attach":"ok","relation":"same"}', '{"attach":true}', ""]);
  });

  it("旧了的那台：attach 回话里的新旧换成 older，后面的流一个字不动", () => {
    const r = runWith(fakes, PEER_PORT_BASE + 1, word("--resident-attach"), '{"relation":"same"}\n', attaching);
    expect(r.stdout.split("\n")).toEqual(['{"hello":{"relation":"same"}}', '{"attach":"ok","relation":"older"}', '{"relation":"same"}', ""]);
  });

  it("正在连的那台：接常驻那一跳拖住不回", () => {
    const r = runWith(fakes, PEER_PORT_BASE + 2, word("--resident-attach"), "", attaching, 600);
    expect(r.error?.message ?? "").toMatch(/ETIMEDOUT/);
    expect(r.stdout).toBe("");
  });

  it("确保常驻（换不换由那台自己判）：在那台自己的沙箱里跑（经它那个跑腿口），两形原样交、不换词", () => {
    // ensure 要认在听的那一位（`SO_PEERCRED` 的 pid），跨进程命名空间认不出 ⇒ 必须在那台自己的沙箱里跑
    // （从前在本机沙箱里跑：回「连接在听的后端失败 · 路径不存在」，本机后端丢了那条流、按退避一轮轮重来）。
    const echo = '#!/bin/sh\necho "dir=$CCM_DATA_DIR $*"\n';
    for (const port of [PEER_PORT_BASE, PEER_PORT_BASE + 1, PEER_PORT_BASE + 3]) {
      for (const w of ["--resident-ensure", "--resident-ensure --replace"]) {
        const r = runWith(fakes, port, word(w), "", echo);
        expect([r.status, r.stdout.trim()]).toEqual([0, `doer ask /tmp/shots-peers/${port - PEER_PORT_BASE}/run/shots-do.sock ${w}`]);
      }
    }
  });

  it("正在装的那台：回话说旧了、问系统照实答、SFTP 交那份慢的；别台不认这两样", () => {
    const att = runWith(fakes, PEER_PORT_BASE + 3, word("--resident-attach"), "", attaching);
    expect(att.stdout.split("\n")[1]).toBe('{"attach":"ok","relation":"older"}');
    const uname = runWith(fakes, PEER_PORT_BASE + 3, "uname -s -m", "", attaching);
    expect([uname.status, uname.stdout.trim()]).toEqual([0, spawnSync("uname", ["-s", "-m"], { encoding: "utf8" }).stdout.trim()]);
    const sftp = runWith(fakes, PEER_PORT_BASE + 3, "shots-sftp", "", attaching);
    expect([sftp.status, sftp.stdout.trim()]).toEqual([0, "slow-sftp /tmp/shots-install/3"]);
    for (const c of ["uname -s -m", "shots-sftp"]) expect(runWith(fakes, PEER_PORT_BASE, c, "", attaching).status).toBe(127);
  });

  it("那台的跑腿口：照那台自己的后端跑确保常驻，stdout · stderr · 退出码原样回；别的参数不跑", async () => {
    const dir = mkdtempSync(path.join(os.tmpdir(), "shots-doer-"));
    const doer = path.join(dir, "shots-do");
    writeFileSync(doer, DOER_PY);
    chmodSync(doer, 0o755);
    const backend = path.join(dir, "backend");
    writeFileSync(backend, '#!/bin/sh\necho "ran $*"\necho "said $*" >&2\nexit 3\n');
    chmodSync(backend, 0o755);
    const sock = path.join(dir, "do.sock");
    const srv = spawn(doer, ["serve", sock, backend], { stdio: "ignore" });
    try {
      await vi.waitFor(() => expect(existsSync(sock)).toBe(true), UNTIL_CEILING);
      const ask = (...a: string[]) => spawnSync(doer, ["ask", sock, ...a], { encoding: "utf8" });
      const r = ask("--resident-ensure", "--replace");
      expect([r.status, r.stdout, r.stderr]).toEqual([3, "ran -- --resident-ensure --replace\n", "said -- --resident-ensure --replace\n"]);
      const no = ask("--resident-stop");
      expect([no.status, no.stdout]).toEqual([127, ""]);
    } finally {
      srv.kill();
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("不认的口 · 不是问常驻那一形 ⇒ 拒，不交后端", () => {
    const port = run(peers, PEER_PORT_BASE + 7, line, ask("history-search"));
    expect([port.status, port.stdout]).toEqual([255, ""]);
    const other = run(peers, PEER_PORT_BASE, productLine("--history-search", "--stdin-line"), ask("history-search"));
    expect([other.status, other.stdout]).toEqual([127, ""]);
    const injected = run(peers, PEER_PORT_BASE, "id; " + line, ask("history-search"));
    expect([injected.status, injected.stdout]).toEqual([127, ""]);
  });

});

describe("交本机后端的机器表", () => {
  /** 一个世界交本机后端的那张表与假 ssh 听的那几个口（同 `pool.mjs` 起世界那一段）。 */
  const tableOf = (w: ReturnType<typeof defaultWorld>) => {
    const files = diskOf(w)[LOCAL].files;
    return { files, ...reachTable(files[".cc-monitor/machines.json"], Object.entries(reachOf(w))) };
  };

  it("从没见过的那台照样在表里、停用开关原样带过去；问得到的那几台拨的就是假 ssh 听的口", () => {
    const w = defaultWorld();
    w.unseenMachines = ["gpu-01"];
    for (const h of (w.config.remote as { hosts: Row[] }).hosts) if (h.label === "gpu-01") h.connect = false;
    const { hosts, peers } = tableOf(w);
    expect(hosts.map((h) => h.label)).toEqual(REMOTES);
    expect(hosts.find((h) => h.label === "gpu-01")?.connect).toBe(false);
    expect(peers.map((p) => p.origin)).toEqual(["devbox", "win-laptop"]);
    for (const p of peers) {
      const row = hosts.find((h) => h.label === p.origin);
      expect([row?.host, row?.port]).toEqual(["127.0.0.1", p.port]);
    }
    expect(new Set(hosts.map((h) => h.port)).size).toBe(hosts.length);
  });

  it("场景 ~/.ssh/config 里 devbox 与表里那台同址（地址 · 用户 · 口）", () => {
    const { files, hosts } = tableOf(defaultWorld());
    const block = files[".ssh/config"].split(/^Host /m).find((b) => b.split("\n")[0].trim() === "devbox") ?? "";
    const opt = (k: string): string | undefined => block.match(new RegExp(`^\\s+${k}\\s+(\\S+)`, "m"))?.[1];
    const row = hosts.find((h) => h.label === "devbox");
    expect([opt("HostName"), opt("User"), Number(opt("Port") ?? 22)]).toEqual([row?.host, row?.user, row?.port]);
  });

  it("每个场景的世界：表照场景整张交、问得到的那几台各有自己的口", async () => {
    const { SCENES } = await import("./scenes");
    const bad: string[] = [];
    for (const s of SCENES) {
      const w = s.world();
      try {
        const { hosts, peers } = tableOf(w);
        const labels = ((w.config.remote as { hosts?: Row[] } | undefined)?.hosts ?? []).map((h) => h.label);
        if (hosts.map((h) => h.label).join() !== labels.join()) bad.push(`${s.id}：表里 ${hosts.map((h) => h.label).join()}，场景写的 ${labels.join()}`);
        if (new Set(peers.map((p) => p.port)).size !== peers.length) bad.push(`${s.id}：两台问得到的同一个口`);
      } catch (e) {
        bad.push(`${s.id}：${(e as Error).message}`);
      }
    }
    expect(bad).toEqual([]);
  });

  it("旧了 · 正在连 · 正在装的那几台交假 ssh 演哪一样；指纹变了的那台本机库里记的是另一把钥", () => {
    const w = defaultWorld();
    w.staleMachines = ["devbox"];
    w.connectingMachines = ["gpu-01"];
    w.installingMachines = ["win-laptop"];
    expect(Object.fromEntries(Object.entries(reachOf(w)).map(([m, r]) => [m, r.fake]))).toEqual({ devbox: "older", "gpu-01": "hang", "win-laptop": "install" });
    expect(tableOf(w).peers.map((p) => p.fake)).toEqual(["older", "hang", "install"]);
    const k = defaultWorld();
    k.hostKeyChanged = ["gpu-01"];
    const { files, hosts } = tableOf(k);
    const row = hosts.find((h) => h.label === "gpu-01");
    expect(files[".cc-monitor/known_hosts"].split("\n")).toContain(`[${row?.host}]:${row?.port} ${HOST_KEYS["gpu-01"].key}`);
    expect(tableOf(defaultWorld()).files[".cc-monitor/known_hosts"]).not.toMatch(/^\[/m);
  });

  it("问得到的那台在场景表里没有那一行 ⇒ 拒（不现编一台）", () => {
    expect(() => reachTable(JSON.stringify({ version: 1, hosts: [] }), [["devbox", { refuse: [] }]])).toThrow(/devbox/);
  });
});
