// auto-e2e F-E4:`@tauri-apps/api/core` 的 e2e 命令级 shim。把 tabs.ts 真源(cleanupOrphanTmux)
// 发出的两条 invoke 重定向到**真 tmux**:
//   list_remote_tmux → `tmux list-sessions` 解析成 TmuxSession[]{name,path,command,attached,windows,sid}
//                      (sid 取 `@ccm_sid` user option;空 → null,同后端语义)。
//   kill_remote_tmux → `tmux kill-session -t <target>`。
//
// ★安全红线(这台真机上有用户真实的 cc-<8hex> / *_cc 会话!):**严格白名单隔离**——
//   `CCM_E2E_ORPHAN_SCOPE`(逗号分隔本轮 fixture 会话名)是唯一可见/可杀集合。
//   list 只返回名在 scope 里的会话 → findOrphanTmux 永远看不到用户真实会话 → 绝不会去杀它。
//   kill 再加一道防御:target 必须 ∈ scope **且** cc- 前缀(镜像后端 is_ccm_tmux_name 白名单),
//   否则抛错拒绝(纵深防御,防判据一旦回归也误杀不到真会话)。
//
// 其它 invoke(tabs.ts 图里别的模块可能发)一律 no-op 返 undefined(本套件只驱动孤儿清理路径)。
import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";

const SEP = "\t";
const KILL_LOG = process.env.CCM_ORPHAN_KILL_LOG;

function scopeSet() {
  return new Set(
    (process.env.CCM_E2E_ORPHAN_SCOPE || "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean),
  );
}
function tmux(args) {
  return spawnSync("tmux", args, { encoding: "utf8" });
}
function isCcmName(name) {
  return name.startsWith("cc-") && name.length > 3 && /^[A-Za-z0-9_-]+$/.test(name);
}

export async function invoke(cmd, args = {}) {
  switch (cmd) {
    case "list_remote_tmux": {
      const scope = scopeSet();
      const fmt = [
        "#{session_name}",
        "#{session_path}",
        "#{pane_current_command}",
        "#{session_attached}",
        "#{session_windows}",
        "#{@ccm_sid}",
      ].join(SEP);
      const r = tmux(["list-sessions", "-F", fmt]);
      if (r.status !== 0) return []; // 无 server / 无会话 → 空(同后端 null-ish)
      const out = [];
      for (const line of String(r.stdout || "").split("\n")) {
        if (!line) continue;
        const [name, path, command, attached, windows, sid] = line.split(SEP);
        if (!scope.has(name)) continue; // ★只暴露本轮 fixture,永不泄漏用户真实会话
        out.push({
          name,
          path: path ?? "",
          command: command ?? "",
          attached: attached === "1",
          windows: Number(windows || "1"),
          sid: sid ? sid : null,
        });
      }
      return out;
    }
    case "kill_remote_tmux": {
      const target = args && args.target;
      const scope = scopeSet();
      if (KILL_LOG) appendFileSync(KILL_LOG, `kill-attempt ${String(target)}\n`);
      // 纵深防御:只杀本轮 fixture 且 cc- 前缀(镜像后端白名单)。
      if (!target || !scope.has(target) || !isCcmName(target)) {
        if (KILL_LOG) appendFileSync(KILL_LOG, `kill-refused ${String(target)}\n`);
        throw new Error(`kill refused (out of e2e scope / not ccm): ${String(target)}`);
      }
      const rr = tmux(["kill-session", "-t", target]);
      if (rr.status !== 0) {
        if (KILL_LOG) appendFileSync(KILL_LOG, `kill-fail ${String(target)}\n`);
        throw new Error("tmux kill-session failed: " + String(rr.stderr || "").trim());
      }
      if (KILL_LOG) appendFileSync(KILL_LOG, `kill-ok ${String(target)}\n`);
      return undefined;
    }
    default:
      return undefined;
  }
}

// tabs.ts 真源图里别的模块从 `@tauri-apps/api/core` 取的其它导出(否则 import 报缺导出)。
export class Channel {
  onmessage = null;
}
export function convertFileSrc(p) {
  return p;
}
export function transformCallback() {
  return 0;
}
