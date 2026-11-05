/**
 * 网页那三扇窗的 WebKitGTK 那一份（`run.mjs --webkit`）：同一个 `serve.mjs`（vite ＋ 真后端池 ＋ 假适配层）、同一份拨钟脚本，
 * 浏览器换成 python-gi 的 WebKit2（`webkit-shoot.py`，在私有 Xvfb 上开一扇窗、从 Xvfb 截那一块）。Linux 桌面版的壳就是 WebKitGTK，
 * Chromium 那一份截出来对、这一份不对的，就是用户在 Linux 上看到的那样。
 *
 * 系统没有 python-gi WebKit2（或 libwebkit2gtk-4.1 · libcairo）⇒ 照实跳过、说缺哪一样，不往用户环境装。
 * 和 Chromium 那一份不一样的：没有调试口 ⇒ 真按键（`__shotsKey`）· 真鼠标拖 / 停（`drag` · `pointer`）做不了，那几幕记成跳过、说为什么；
 * 减少动画是把过渡与动画时长压成 0（没有媒体查询那一句模拟）；页里的控制台不收。
 */
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";
import { privateXvfb } from "./filewin.mjs";

const SHOOTER = "tests/shots/webkit-shoot.py";

/**
 * 这台缺哪一样（python3 · gi 的 Gtk 3 与 WebKit2 4.1 · 截那一张要直接调的两个库 `libwebkit2gtk-4.1.so.0` 与 `libcairo.so.2`，
 * 同 `webkit-shoot.py` 开头 `ctypes.CDLL` 的那两个）；都在 ⇒ `null`。
 */
export function webkitMissing(env) {
  const probe = [
    "import ctypes",
    "import gi",
    "gi.require_version('Gtk', '3.0')",
    "gi.require_version('WebKit2', '4.1')",
    "from gi.repository import Gtk, WebKit2",
    "ctypes.CDLL('libwebkit2gtk-4.1.so.0')",
    "ctypes.CDLL('libcairo.so.2')",
  ].join("\n");
  const r = spawnSync("python3", ["-I", "-c", probe], { env, encoding: "utf8" });
  if (r.error) return `没有 python3（${r.error.message}）`;
  if (r.status !== 0) return (r.stderr ?? "").trim().split("\n").pop() || `探测退出码 ${r.status}`;
  return null;
}

/** 那一幕没做成的原因若是「要调试口那一套真输入」⇒ 跳过的说法；否则 `null`。 */
function needsDevtools(row) {
  if (row.real?.length) return `要真鼠标（${row.real.join(" · ")}）`;
  if (/没有真按键通道/.test(row.error ?? "")) return "要真按键";
  return null;
}

/**
 * 逐幕截（`only` 是清单 id 的正则串）。`boot` 是每一页开头要先跑的脚本（拨钟 ＋ 不画光标，同 Chromium 那一份）。
 * 回 `{ results, problems, skipped }`：`results` 同 Chromium 那一份的行（图集照它出），`skipped` 一幕一句。
 */
export async function shootWebkit({ repo, base, out, sandbox, only, boot, env, children, ceilingMs }) {
  const results = [];
  const problems = [];
  const skipped = [];
  const x = await privateXvfb({ repo, env, children });
  if (x === null) return { results, problems: ["WebKitGTK：Xvfb 没报号就退了（起不来）"], skipped };
  const bootFile = path.join(sandbox, "webkit-boot.js");
  mkdirSync(sandbox, { recursive: true });
  writeFileSync(bootFile, boot);
  const runEnv = { ...env, DISPLAY: `:${x.display}`, LIBGL_ALWAYS_SOFTWARE: "1", WEBKIT_DISABLE_COMPOSITING_MODE: "1" };
  delete runEnv.WAYLAND_DISPLAY;
  const args = ["-I", path.join(repo, SHOOTER), base, out, bootFile, String(ceilingMs), ...(only ? [only] : [])];
  const py = spawn("python3", args, { env: runEnv, stdio: ["ignore", "pipe", "pipe"] });
  children.push(py);
  let tail = "";
  py.stderr.on("data", (d) => {
    tail = (tail + String(d)).slice(-4000);
  });
  for await (const line of createInterface({ input: py.stdout })) {
    if (!line.startsWith("{")) continue;
    const row = JSON.parse(line);
    if (row.count !== undefined) {
      console.log(`WebKitGTK 网页场景 ${row.count} 个`);
      continue;
    }
    const s = row.scene;
    const file = path.join(s.dir, `${s.id}.png`);
    const skip = needsDevtools(row);
    if (skip) {
      skipped.push(`${s.id}：${skip}`);
      console.log(`— ${s.dir}/${s.id} —— 跳过：${skip}`);
      continue;
    }
    let ok = row.state === "done";
    let note = row.error ?? "";
    if (row.unhandled?.length) problems.push(`${s.id}：假后端答不上 ${row.unhandled.join("、")}`);
    if (ok && row.layout?.length) {
      ok = false;
      note = `排版：${row.layout.join("；")}`;
      problems.push(`${s.id}：${note}`);
    } else if (!ok) problems.push(`${s.id}：没截成 —— ${note.split("\n")[0]}`);
    results.push({ ...s, file, ok });
    console.log(`${ok ? "✓" : "✗"} ${s.dir}/${s.id}${ok ? "" : ` —— ${note.split("\n")[0].slice(0, 300)}`}`);
  }
  const code = py.exitCode ?? (await new Promise((r) => py.once("exit", r)));
  if (code !== 0) problems.push(`WebKitGTK：浏览器那一份退出码 ${code}：${tail.trim().split("\n").slice(-4).join(" | ")}`);
  await x.stop();
  return { results, problems, skipped };
}
