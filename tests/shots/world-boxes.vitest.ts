/**
 * 截图台架起一个带远端的世界：远端各台的沙箱（它们的常驻 · 跑腿口）由台架自己起，交给无头壳的只有本机那一台。
 *
 * 起因（A17-47）：10-26 起无头壳照产品那样只起本机那一台（别台的流归本机后端），台架照旧把各台的整条命令交给壳 ⇒
 * 远端各台没人起，本机沙箱开场等远端常驻问得到等满 30 秒、退出，本机后端从没起，每个带远端的场景都报「<local> 的后端 30 秒没握上手」。
 *
 * 人群：`real/pool.mjs` 的 `/__ccm/world`（造世界）与收场（`close`）。
 * 替身：无头壳只认 `machines` · `offer` 两问（照产品只起本机；本机那一台这里不起，不在这条判据里）；后端是一段 sh：
 *   常驻那一形（`CCM_RESIDENT=1 … --stream`）在它的家里落一个记号、守着，问常驻（`--resident-call`）看记号在不在。
 * 买不到：真后端 · 真无头壳 · 本机那一跳握手（截图那一趟本身判）。要真 bwrap：起不来的机器（Windows · 不许建命名空间的）这一格让开。
 */
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";
import { UNTIL_CEILING, WALL_CEILING_MS } from "../test-support/wall-ceiling";

const POOL = "./real/pool.mjs";
type Pool = { plugin: { configureServer: (s: { middlewares: { use: (f: Serve) => void } }) => void }; close: () => void };
type Serve = (req: unknown, res: unknown, next: () => void) => Promise<void>;
const { backendPool, settledAs } = (await import(POOL)) as {
  backendPool: (o: { repo: string; sandbox: string }) => Pool;
  settledAs: (frame: unknown) => { sid: string; fate: string } | null;
};

/**
 * 收场那一段（`/__ccm/end`）认的是会话流上裁出来的那一帧：10-26 起离开「活」只一帧 `left`，去向在 `face.life.is`（A1-3-2）；
 * 从前认的 `state` 帧没了 ⇒ 每个带收场的场景等满上限。帧照壳会话簿金样里那几条离开的会话（真线上那一形）。
 */
describe("收场认得出裁出来的那一帧", () => {
  const book = JSON.parse(readFileSync(path.join(__dirname, "../comms/resident-client/book.golden.json"), "utf8")) as { left: Record<string, unknown>; live: Record<string, unknown> };
  it("left 帧 ⇒ 那条会话 · 去向", () => {
    expect(Object.values(book.left).map((left) => settledAs({ left }))).toEqual([
      { sid: "b", fate: "reconnectable" },
      { sid: "c", fate: "ended" },
    ]);
  });
  it("宣告 · 别的帧不算收场", () => {
    expect(Object.values(book.live).map((added) => settledAs({ added }))).toEqual([null, null]);
  });
});

const boxWorks =
  process.platform !== "win32" &&
  spawnSync("bwrap", ["--ro-bind", "/", "/", "--dev", "/dev", "--unshare-pid", "--proc", "/proc", "--unshare-net", "--", "/bin/true"]).status === 0;

/** 后端替身：常驻那一形落记号并守着；问常驻 ⇒ 记号在才算问到。 */
const BACKEND = `#!/bin/sh
d=\${CCM_DATA_DIR:-$HOME/.cc-monitor}
case " $* " in
  *" --resident-call "*) cat >/dev/null; [ -e "$d/run/up" ]; exit ;;
  *" --stream "*) [ "$CCM_RESIDENT" = 1 ] || exit 1; mkdir -p "$d/run"; : > "$d/run/up"; exec sleep 3600 ;;
esac
exit 2
`;

/** 无头壳替身：记下交来的机器表，各台都答握上手。 */
const SHELL = `#!/usr/bin/python3
import json, sys
for line in sys.stdin:
    f = json.loads(line)
    if f.get("t") == "machines":
        open("machines.json", "w").write(json.dumps(f["machines"]))
    elif f.get("t") == "offer":
        print(json.dumps({"t": "offer", "n": f["n"], "up": True}), flush=True)
`;

/** 走一趟插件的口（同 vite 那一层：POST 一个 JSON 体，回状态码与回包）。 */
async function post(serve: Serve, url: string, body: unknown): Promise<{ status: number; text: string }> {
  return new Promise((resolve) => {
    const data = JSON.stringify(body);
    const req = {
      method: "POST",
      url,
      setEncoding() {},
      on(ev: string, f: (d?: string) => void) {
        if (ev === "data") queueMicrotask(() => f(data));
        if (ev === "end") setTimeout(() => f(), 0);
      },
    };
    const res = { statusCode: 200, setHeader() {}, end: (text: string) => resolve({ status: res.statusCode, text }) };
    void serve(req, res, () => resolve({ status: 404, text: "" }));
  });
}

/** 命令行里带着这个目录的进程（看收场收干净没有）。 */
function procsUnder(dir: string): string[] {
  return spawnSync("pgrep", ["-f", dir], { encoding: "utf8" }).stdout.split("\n").filter(Boolean);
}

describe.skipIf(!boxWorks)("截图台架起带远端的世界", () => {
  it(
    "远端各台的常驻由台架起，收场整组收掉",
    async () => {
      const repo = mkdtempSync(path.join(os.tmpdir(), "shots-boxes-"));
      const saved = process.env.CCM_SHOTS_BACKEND;
      let pool: Pool | null = null;
      try {
        const backend = path.join(repo, "backend");
        writeFileSync(backend, BACKEND);
        chmodSync(backend, 0o755);
        const shellDir = path.join(repo, ".build/shell/debug/examples");
        mkdirSync(shellDir, { recursive: true });
        writeFileSync(path.join(shellDir, "ccm-shots-shell"), SHELL);
        chmodSync(path.join(shellDir, "ccm-shots-shell"), 0o755);
        process.env.CCM_SHOTS_BACKEND = backend;
        const sandbox = path.join(repo, "sandbox");
        pool = backendPool({ repo, sandbox });
        let serve: Serve | null = null;
        pool.plugin.configureServer({ middlewares: { use: (f) => (serve = f) } });
        const hosts = ["devbox", "gpu-01"].map((label, k) => ({ label, host: "127.0.0.1", port: 2201 + k, user: "user", keyPath: "/x", connect: true }));
        const machines = {
          "<local>": { files: { ".cc-monitor/machines.json": JSON.stringify({ version: 1, hosts }) } },
          devbox: { files: {} },
          "gpu-01": { files: {} },
        };
        const got = await post(serve!, "/__ccm/world", { key: "boxes", machines, reach: { devbox: { refuse: [] }, "gpu-01": { refuse: [] } }, scene: "boxes" });
        expect(got.status, got.text).toBe(200);
        const world = path.join(sandbox, "worlds", "boxes");
        // 远端两台：常驻都起过（记号落在各自的家里）。
        expect(["m1", "m2"].filter((m) => !existsSync(path.join(world, m, "home/.cc-monitor/run/up")))).toEqual([]);
        // 交给无头壳的只有本机那一台。
        await vi.waitFor(() => expect(existsSync(path.join(world, "machines.json"))).toBe(true), UNTIL_CEILING);
        const handed = JSON.parse(readFileSync(path.join(world, "machines.json"), "utf8")) as { origin: string }[];
        expect(handed.map((m) => m.origin)).toEqual(["<local>"]);
        // 收场：远端各台的沙箱连同里面的常驻一起没了。
        pool.close();
        pool = null;
        await vi.waitFor(() => expect(procsUnder(world)).toEqual([]), UNTIL_CEILING);
      } finally {
        pool?.close();
        if (saved === undefined) delete process.env.CCM_SHOTS_BACKEND;
        else process.env.CCM_SHOTS_BACKEND = saved;
        rmSync(repo, { recursive: true, force: true });
      }
    },
    WALL_CEILING_MS,
  );
});
