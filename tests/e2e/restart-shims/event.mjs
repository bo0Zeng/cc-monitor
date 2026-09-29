// 〔FIX4 · 主会话裁 ④〕`@tauri-apps/api/event` 的 e2e 替身：换号重启今天等「那台报出这条会话起来了」才算成（`launch-arrival.ts::awaitArrival`）。
//
// 生产那一侧：发起方 `emit("launch-arrival-expect", {…, ticket})` → 主窗口看会话流的 `live` 格认出它 → 回 `launch-arrival-done {ticket, arrived}`。
// 命令级驱动器没有主窗口、也没有会话流 ⇒ 本替身站在主窗口那个位置，用这台 e2e 能看得见的那一格代替 `live`：
// **预期里那个 tmux 会话名真的在（私有 socket 上 `tmux has-session`）且 fake-claude 已写下这条 sid 的 `--resume` 那一行**，
// 限时轮询；等到 ⇒ 回 `arrived: true`，等不到 ⇒ `false`（与生产「预算内没见到」同一结局）。其余事件照收照发、不报错。
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const listeners = new Map();
const EXPECT = "launch-arrival-expect";
const DONE = "launch-arrival-done";
const WAIT_MS = Number(process.env.CCM_ARRIVAL_WAIT_MS || 12000);

function alive(name) {
  return spawnSync("tmux", ["has-session", "-t", `=${name}:`], { encoding: "utf8" }).status === 0;
}
function resumed(sid) {
  const dir = process.env.CCM_ARRIVAL_ARGV_DIR;
  if (!dir || !sid) return true; // 驱动器没给 argv 目录 ⇒ 只认会话在不在
  try {
    return readFileSync(`${dir}/argv.log`, "utf8").split("\n").some((l) => l.includes(`sid=${sid} `) && l.includes(`--resume ${sid}`));
  } catch {
    return false;
  }
}

export async function listen(name, cb) {
  if (!listeners.has(name)) listeners.set(name, new Set());
  listeners.get(name).add(cb);
  return () => listeners.get(name)?.delete(cb);
}

export async function emit(name, payload) {
  if (name !== EXPECT || !payload?.ticket) return;
  const sid = payload.match?.sid;
  const deadline = Date.now() + WAIT_MS;
  let arrived = false;
  while (Date.now() < deadline) {
    if (payload.tmuxName && alive(payload.tmuxName) && resumed(sid)) {
      arrived = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  for (const cb of listeners.get(DONE) ?? []) cb({ payload: { ticket: payload.ticket, arrived } });
}
