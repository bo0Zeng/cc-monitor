/**
 * 性能台架几只脚本共用的那几样：进程树 CPU 时长 · 找浏览器 · 调试口 · 空闲端口 · 分位数。
 * `bench.mjs`（主窗口 · 查看窗）与 `settings-bench.mjs`（设置窗 · 长开内存）都用它。
 */
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { sleep } from "../cdp.mjs";

export function pct(xs, q, floor = 0) {
  if (xs.length === 0) return floor;
  const s = [...xs].sort((a, b) => a - b);
  return Math.max(floor, s[Math.min(s.length - 1, Math.floor(q * s.length))]);
}

/**
 * 一棵进程树（浏览器连同它起的渲染 / GPU 进程）到此刻一共用了多少 CPU 毫秒：每个线程 `/proc/<pid>/task/<tid>/schedstat`
 * 第一格（纳秒，在 CPU 上跑的时长）相加。机器忙时墙钟会被别的活拉长，CPU 时长基本不受影响 ⇒ 两样都记。
 */
export function cpuMs(root) {
  const cpuSeen = (globalThis.__cpuSeen ??= new Map());
  const kids = new Map();
  for (const d of readdirSync("/proc")) {
    if (!/^\d+$/.test(d)) continue;
    try {
      const st = readFileSync(`/proc/${d}/stat`, "utf8");
      const ppid = Number(st.slice(st.lastIndexOf(")") + 2).split(" ")[1]);
      if (!kids.has(ppid)) kids.set(ppid, []);
      kids.get(ppid).push(Number(d));
    } catch {
      // 进程刚退
    }
  }
  const stack = [root];
  while (stack.length) {
    const pid = stack.pop();
    stack.push(...(kids.get(pid) ?? []));
    try {
      for (const t of readdirSync(`/proc/${pid}/task`)) {
        try {
          // 按线程记最后一次读数、只往上加：中途退掉的进程（关页时它的渲染进程）留着最后的读数，合计不倒着走
          const v = Number(readFileSync(`/proc/${pid}/task/${t}/schedstat`, "utf8").split(" ")[0]);
          const key = `${pid}/${t}`;
          cpuSeen.set(key, Math.max(cpuSeen.get(key) ?? 0, v));
        } catch {
          // 线程刚退
        }
      }
    } catch {
      // 进程刚退
    }
  }
  let ns = 0;
  for (const v of cpuSeen.values()) ns += v;
  return ns / 1e6;
}

export function findChrome() {
  const own = process.env.CCM_SHOTS_CHROME;
  if (own) return own;
  const cache = path.join(os.homedir(), ".cache/ms-playwright");
  const found = existsSync(cache)
    ? readdirSync(cache)
        .filter((d) => /^chromium-\d+$/.test(d))
        .map((d) => path.join(cache, d, "chrome-linux64/chrome"))
        .filter((p) => existsSync(p))
        .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)
    : [];
  if (found.length === 0) throw new Error("找不到 Chromium：设 CCM_SHOTS_CHROME");
  return found[0];
}

export async function devtoolsUrl(dir) {
  const file = path.join(dir, "DevToolsActivePort");
  // 机器忙时浏览器起来要好一阵 ⇒ 最多等 60 s
  for (let i = 0; i < 600; i++) {
    if (existsSync(file)) {
      const [p, q] = readFileSync(file, "utf8").split("\n");
      if (p && q) return `ws://127.0.0.1:${p}${q}`;
    }
    await sleep(100);
  }
  throw new Error("浏览器没报出调试口");
}

export function freePort() {
  return new Promise((resolve, reject) => {
    const s = net.createServer();
    s.listen(0, "127.0.0.1", () => {
      const { port: p } = s.address();
      s.close(() => resolve(p));
    });
    s.on("error", reject);
  });
}
