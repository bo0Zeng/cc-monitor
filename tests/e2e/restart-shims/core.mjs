// auto-e2e F-E3:`@tauri-apps/api/core` 的 **e2e 命令级 shim**（测试 fixture,非生产/backend 改动）。
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
import { fileURLToPath } from "node:url";
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
      // F01：与 Rust 侧 `exact_target` 同构——`=<名>:` 精确匹配。裸目标是「精确→名字开头→glob」
      // 三级解析，会把按键投进兄弟会话（`cc-<sid8>-2`）。shim 必须与生产同构，否则 e2e 对这条假绿。
      const a = ["send-keys", "-t", `=${target}:`, keys];
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
      const r = tmux(["kill-session", "-t", `=${target}:`]); // F01：同上，精确匹配
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
    // 〔E2 尾 09-27〕账号三问与 tmux 两条控制（结束 · 发按键）今天走**通道**（`chan.call(origin, op, payload)` ⇒
    //   包装层 `chan_call`，`src/ipc/chan.ts`），不再是各自的 Tauri 命令。旧的那几臂（`list_remote_accounts` ·
    //   `kill_remote_tmux` · `tmux_send_keys` …）从那天起没有调用方，编排拿到 `undefined` 当场判「账号不可用」
    //   ⇒ 本 shim 跟着改成**说通道**：去程是 JSON 字节、回程是 JSON 字节；「不行」按对端拒绝信封
    //   `{err:"Refused", body:<{code,message} 的字节>}` 抛（`chan.ts::decodeFail` 认的那一形）。
    case "chan_call":
      return chanCall(args.op, JSON.parse(Buffer.from(args.payload || []).toString("utf8") || "{}"));
    // 〔E2 尾 09-27〕resume 那一串今天由 **Rust 渲染器**出（`remote-launch-run.ts::renderLaunchCommand` ⇒
    //   `commands.render_launch_payload`）。本 shim 从前返回 `undefined` ⇒ `launch_remote_terminal` 拿到空命令、
    //   什么都没起。⇒ 交给**生产那一条**：`launch-render-emit.sh` → Rust `emit_launch_render_for_e2e`
    //   → 生产 `launch_wire::render_launch_payload`（与 `launch-render-driver.ts` 同一个出口，一字不另写）。
    case "render_launch_payload":
      return renderViaProduction(args.req);
    case "render_ccm_launch":
      // `ccm …` 调用行那条路要先探远端 ccm；命令级驱动器没有远端 ⇒ 如实说渲不出，编排照生产逻辑降级到载荷那条。
      return { ok: false, cmd: null, reason: "e2e shim：没有远端 ccm 可探" };
    case "relay_endpoint_for_launch":
      return null; // 不走中转（夹具的账号都是订阅号）
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

const enc = (v) => new TextEncoder().encode(JSON.stringify(v)).buffer;
function refused(code, message) {
  // 与后端的拒绝信封同形 ⇒ 编排走的是真实的「对端拒了」那一支（`control-said.ts::settle`）。
  throw { err: "Refused", body: Array.from(new TextEncoder().encode(JSON.stringify({ code, message }))) };
}

function chanCall(op, body) {
  switch (op) {
    case "accounts-list": {
      // 成品形状 = `accounts-decode.ts::decodeAccountsList` 逐键要求的那一份（夹具只给 accounts，meta 按「库已启用」补齐）。
      const raw = JSON.parse(process.env.CCM_ACCOUNTS_JSON || '{"accounts":[]}');
      return enc({
        meta: { enabled: true, acctsDir: "", manifestPath: "", updatedAt: null, sharedStore: null, count: raw.accounts.length, error: null },
        accounts: raw.accounts,
        notice: null,
      });
    }
    case "accounts-trust":
      // trust 只警告不阻断（§5 ①）；e2e 里恒答「不知道」⇒ 走「未知」分支，不影响主流程。
      return enc({ trusted: false, known: false });
    case "accounts-sessions":
      return enc({ sessions: [] });
    case "launch": {
      // 发按键：`send-into`（键入 ＋ 回车）/ `send-keys-raw`（裸键）。与后端同构：`=<名>:` 精确寻址（F01）。
      const { mode, name, payload } = body;
      const label =
        payload === "/compact" ? "compact" : payload === "Escape" ? "escape" : payload === "/exit" ? "exit" : "sendkeys:" + payload;
      seq(label);
      const a = ["send-keys", "-t", `=${name}:`, payload];
      if (mode === "send-into") a.push("Enter");
      const r = tmux(a);
      // 会话不在（已被杀/漂移）⇒ 与后端同一个码，由真源的 try/catch 兜（降级 kill）。
      if (r.status !== 0) refused("no_such_session", String(r.stderr || "").trim());
      return enc({ session: name, created: false, typed: true });
    }
    case "kill": {
      const { name } = body;
      seq("kill-attempt");
      if (process.env.CCM_KILL_FAIL === "1") {
        seq("kill-fail");
        refused("kill_failed", "injected refusal (CCM_KILL_FAIL=1)");
      }
      const r = tmux(["kill-session", "-t", `=${name}:`]); // F01：同上，精确匹配
      if (r.status !== 0) {
        seq("kill-fail");
        refused("no_such_session", String(r.stderr || "").trim());
      }
      seq("kill");
      return enc({ session: name, killed: true });
    }
    default:
      seq("chan?:" + op);
      refused("unsupported_in_e2e_shim", "restart-shims/core.mjs 不认这条 op：" + op);
  }
}

const EMIT = fileURLToPath(new URL("../launch-render-emit.sh", import.meta.url));
function renderViaProduction(req) {
  const r = spawnSync("bash", [EMIT], {
    env: { ...process.env, CCM_E2E_RENDER_REQ: JSON.stringify(req) },
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  const out = `${r.stdout ?? ""}`;
  const ok = /LAUNCH_RENDER<<<(.*)>>>/.exec(out);
  if (ok) return ok[1];
  const err = /LAUNCH_RENDER_ERR<<<(.*)>>>/.exec(out);
  // 拒了 ⇒ 照生产那样抛（`REFUSE:` 那句原样带出去）；取不到标记行 ⇒ 抛，**绝不回空串**（空串会被读成「渲出了空命令」）。
  throw new Error(err ? err[1] : `取不到生产渲染器的输出（launch-render-emit.sh 退出码 ${String(r.status)}）：${out.slice(-800)}${String(r.stderr ?? "").slice(-800)}`);
}
