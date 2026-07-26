// auto-e2e F-E3:`@tauri-apps/api/core` 的 **e2e 命令级 shim**（测试 fixture,非生产/daemon 改动）。
//
// 诚实层级:Linux headless 下 Tauri IPC 边界结构性不可达（app 不在跑、GUI 触发经
// `launch.rs::launch_powershell_window` 仅 Windows）。本 shim 把 `restartWithAccount`（真源）
// 编排真正发出的每一条 `invoke(...)` **重定向到真 tmux + fake-claude**,并把编排步骤按序写进
// $CCM_SEQ_LOG。于是被测代码是**真的** account-restart.ts 编排逻辑 + 真 tmux 效果 + 真账号解析,
// 唯一被替换的只是那道无法在 Linux 触达的 IPC 边界（本就该由后端 Rust 执行 tmux 的地方）。
//
// 失败注入（模拟后端确定性失败,§5.2 边界）：
//   CCM_KILL_FAIL=1     → kill_remote_tmux 抛（编排必须中止、不续 resume）。
//   CCM_RESUME_FAIL=1   → launch_remote_terminal 抛（resume 没起来,不得记账/报成功）。
//
// 账号 fixture 经 CCM_ACCOUNTS_JSON（RawAccountsResult）喂给真源 fetchAccounts/accountConfigDir。
import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";

const SEQ = process.env.CCM_SEQ_LOG || "/tmp/e2e-restart-seq.log";
function seq(line) {
  appendFileSync(SEQ, line + "\n");
}
function tmux(args) {
  return spawnSync("tmux", args, { encoding: "utf8" });
}

export async function invoke(cmd, args = {}) {
  switch (cmd) {
    // —— 账号解析真源所需的只读命令 ——
    case "load_config":
      return {};
    case "list_remote_accounts":
      return JSON.parse(
        process.env.CCM_ACCOUNTS_JSON ||
          '{"available":true,"error":null,"meta":null,"accounts":[]}',
      );
    case "list_remote_session_accounts":
      return { available: false, error: null, sessions: [] };
    case "check_account_trust":
      // trust 只警告不阻断（§5 ①）；e2e 里恒不可用 → 走"未知"分支,不影响主流程。
      return { available: false, trusted: false, known: false, error: null };

    // —— 编排真正的破坏性效果:全打到真 tmux ——
    case "tmux_send_keys": {
      const { target, keys, enter } = args;
      const label =
        keys === "/compact"
          ? "compact"
          : keys === "Escape"
            ? "escape"
            : keys === "/exit"
              ? "exit"
              : "sendkeys:" + keys;
      seq(label);
      const a = ["send-keys", "-t", target, keys];
      if (enter) a.push("Enter");
      const r = tmux(a);
      // 会话不在（已被杀/漂移）→ tmux 报错 → 抛,由真源 ④a 的 try/catch 兜（降级 kill）。
      if (r.status !== 0) throw new Error("tmux send-keys failed: " + String(r.stderr || "").trim());
      return undefined;
    }
    case "kill_remote_tmux": {
      const { target } = args;
      seq("kill-attempt");
      if (process.env.CCM_KILL_FAIL === "1") {
        seq("kill-fail");
        throw new Error("kill_remote_tmux rejected (injected IPC failure)");
      }
      const r = tmux(["kill-session", "-t", target]);
      if (r.status !== 0) {
        seq("kill-fail");
        throw new Error("tmux kill-session failed: " + String(r.stderr || "").trim());
      }
      seq("kill");
      return undefined;
    }
    case "launch_remote_terminal": {
      const { remoteCmd } = args;
      seq("resume-attempt");
      if (process.env.CCM_RESUME_FAIL === "1") {
        seq("resume-fail");
        throw new Error("launch_remote_terminal rejected (injected IPC failure)");
      }
      // runRemoteResumeTmux 的成功契约 = "拉起 IPC 被接受"（不等 attach）。真源命令串
      // 尾部 `tmux attach` 在无 tty 下即刻失败(无害),但 new-session -d + send-keys 已把
      // resume 打进 pane → fake-claude 用注入的 CLAUDE_CONFIG_DIR 起来并写 argv.log。
      // 故:只要 spawn 出去了就算成功（不看退出码/超时,对齐 GUI 拉起窗口即返回的语义）。
      spawnSync("bash", ["-c", remoteCmd], { encoding: "utf8", timeout: 8000 });
      seq("resume");
      return undefined;
    }
    case "update_history_metadata": {
      const acct = args && args.patch ? args.patch.lastAccount : undefined;
      seq("record account=" + String(acct));
      return undefined;
    }
    default:
      seq("invoke?:" + cmd);
      return undefined;
  }
}
