// auto-e2e F-E2:resume 命令**真源驱动器**（测试 fixture，非生产改动）。
//
// 目的:让 bash 套件拿到「app 真正会跑的 resume 命令串 / 账号解析结果」——绝不在 shell 里
// 重写一份(那样测的是复制品、不是被测代码)。#75(CLAUDE_CONFIG_DIR 注入) / #76(复用 cc-<sid8>
// 名不产 -N 孤儿) 的修复都活在生产那条链上,套件据本驱动器的 stdout 断言并真跑到 tmux。
//
// 命令串的三个 mode 走**生产那条链**（`launch-render-driver.ts`：生产 `plan*` →
// 生产 `buildCliRenderRequest` → 生产 Rust `render_ccm_launch`），产出是那一行 `ccm …`。
//
// 用法(每个 mode 打印一行 stdout):
//   into-existing <sid> <name> <launcher> [account]   -> 往已有 tmux 会话里键入那一形（send_into）→ 生产渲染
//   tmux-new      <sid> <cwd> <launcher> <name> [account] -> 新建 tmux 会话 resume（带身份标记）→ 生产渲染
//   direct        <sid> <cwd> <launcher> [account] -> planResumeDirect → 生产渲染
//   （`mint-name` 那个 mode 删了：tmux 名的派生 ＋ 避让只在后端 `terminal-name-mint`，前端那份铸名口没了）
//   （`follow` 那个 mode 删了：跟随判号住那台后端 `control/launch_account.rs::pick`，由 Rust 判据与 ccm 端到端那一条钉）
//   acct-dir      <name> <stateJson>                      -> accountConfigDir(路径或 "<none>")
//
// account 传字面 "-" 或省略 = 跟随（判据渲染这一侧当「不表态」，远端那一行落 `--base`）；给了 = 点名那个号（`--account <名>`，
// 号要登记在沙箱家目录的账号清单里：调用它的套件自己写那份 `accounts.json`）。
import { planResumeDirect } from "../../src/frontend/ui/launch-requests.ts";
import { renderCmdViaProduction } from "./launch-render-driver.ts";
// 这几套跑的是 claude 那一家（假的 claude 当启动器）。
import { DEFAULT_AGENT } from "../../src/frontend/ui/agent-profile.ts";
import { accountConfigDir } from "../../src/frontend/ui/accounts.ts";
import type { LaunchContext, LaunchModifiers } from "../../src/frontend/ui/launch-types.ts";

/** 在 tmux 会话 `name` 里 resume `sid`（后端换号重启 / 分叉起的就是这一形）。 */
function resumeInTmux(sid: string, name: string, cwd: string, launcher: string, mods: LaunchModifiers, ccmSid: string | undefined): LaunchContext {
  return {
    agent: DEFAULT_AGENT,
    action: { kind: "resume", sid },
    container: { kind: "tmux", name, mode: "create" },
    cwd: cwd.trim() || null,
    account: mods.account ?? { kind: "follow" },
    models: mods.models ?? {},
    launcherOverride: launcher,
    ccmSid,
  };
}

function opt(v: string | undefined): string | undefined {
  return v === undefined || v === "-" || v === "" ? undefined : v;
}

function acct(name: string | undefined): LaunchModifiers {
  return name === undefined ? {} : { account: { kind: "named", name } };
}

const [mode, ...a] = process.argv.slice(2);
try {
  switch (mode) {
    case "into-existing":
      process.stdout.write(
        renderCmdViaProduction(
          resumeInTmux(a[0], a[1], "", a[2], acct(opt(a[3])), undefined),
          true,
        ) + "\n",
      );
      break;
    case "tmux-new":
      process.stdout.write(
        renderCmdViaProduction(
          resumeInTmux(a[0], a[3], a[1], a[2], acct(opt(a[4])), a[0]),
        ) + "\n",
      );
      break;
    case "direct":
      process.stdout.write(
        renderCmdViaProduction(planResumeDirect(DEFAULT_AGENT, a[0], a[1], a[2], acct(opt(a[3])))) +
          "\n",
      );
      break;
    case "acct-dir": {
      const state = JSON.parse(a[1] ?? "{}");
      const dir = accountConfigDir(state, a[0]);
      process.stdout.write((dir ?? "<none>") + "\n");
      break;
    }
    default:
      process.stderr.write(`unknown mode: ${String(mode)}\n`);
      process.exit(2);
  }
} catch (e) {
  process.stderr.write(`DRIVER_THROW ${String((e as Error).message ?? e)}\n`);
  process.exit(3);
}
