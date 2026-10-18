/**
 * 截图台架那个按口转发的假 ssh（`real/pool.mjs::forwarderSh`，本机沙箱里 sshd 的登录壳）与它交本机后端登记的那一份（`reachArgs`）。
 *
 * - 按连进来的口认台：第 k 台 ⇒ 那台常驻的套接字目录（`CCM_DATA_DIR`），产品单引号过的那几格原样切回来、stdin 那一行原样交过去。
 * - 只转问常驻那一形：别的命令 · 不认的口一律拒（台架那几台之外谁也够不着）。
 * - 场景演「那一跳断开」的那几条（`refuse`）：一个字不回、退 255，别的命令照转。
 * - 登记的拨号拨的就是转发认的那个口。
 *
 * 买不到：sshd 本身与沙箱里那一跳（`--resident-call` 连常驻）不在这里起；台架里每台问没问到由截图那一趟的问题表判（`fake/backend.ts::judgeAsked`）。
 */
import { spawnSync } from "node:child_process";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";

const POOL = "./real/pool.mjs";
type Peer = { port: number; k: number; refuse?: string[] };
const { forwarderSh, reachArgs, PEER_PORT_BASE } = (await import(POOL)) as {
  forwarderSh: (peers: Peer[], backend?: string) => string;
  reachArgs: (origin: string, k: number) => { origin: string; dial: { machine: { host: string; port: number } } };
  PEER_PORT_BASE: number;
};

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

  it("不认的口 · 不是问常驻那一形 ⇒ 拒，不交后端", () => {
    const port = run(peers, PEER_PORT_BASE + 7, line, ask("history-search"));
    expect([port.status, port.stdout]).toEqual([255, ""]);
    const other = run(peers, PEER_PORT_BASE, productLine("--history-search", "--stdin-line"), ask("history-search"));
    expect([other.status, other.stdout]).toEqual([127, ""]);
    const injected = run(peers, PEER_PORT_BASE, "id; " + line, ask("history-search"));
    expect([injected.status, injected.stdout]).toEqual([127, ""]);
  });

  it("登记的拨号拨的就是转发认的那个口", () => {
    const r = reachArgs("gpu-01", 1);
    expect(r.origin).toBe("gpu-01");
    expect([r.dial.machine.host, r.dial.machine.port]).toEqual(["127.0.0.1", PEER_PORT_BASE + 1]);
  });
});
