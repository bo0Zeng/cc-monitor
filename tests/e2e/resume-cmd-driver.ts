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
//   into-existing <sid> <name> <launcher> [configDir]   -> planResumeIntoExistingTmux → 生产渲染
//   tmux-new      <sid> <cwd> <launcher> <name> [configDir] -> planResumeTmux → 生产渲染
//   direct        <sid> <cwd> <launcher> [configDir] -> planResumeDirect → 生产渲染
//   （`mint-name` 那个 mode 删了：tmux 名的派生 ＋ 避让只在后端 `tmux-name-mint`，前端那份铸名口没了）
//   follow        <lastAccount|-> <current|-> <stateJson> -> resolveFollowAccount(名或 "<base>")
//   acct-dir      <name> <stateJson>                      -> accountConfigDir(路径或 "<none>")
//
// configDir 传字面 "-" 或省略 = undefined(基座,无账号注入)。
import {
  planResumeIntoExistingTmux,
  planResumeTmux,
  planResumeDirect,
} from "../../src/frontend/ui/launch-requests.ts";
import { renderCmdViaProduction } from "./launch-render-driver.ts";
// 这几套跑的是 claude 那一家（假的 claude 当启动器）。
import { DEFAULT_AGENT } from "../../src/frontend/ui/agent-profile.ts";
import { resolveFollowAccount, accountConfigDir } from "../../src/frontend/ui/accounts.ts";

function opt(v: string | undefined): string | undefined {
  return v === undefined || v === "-" || v === "" ? undefined : v;
}

const [mode, ...a] = process.argv.slice(2);
try {
  switch (mode) {
    case "into-existing":
      process.stdout.write(
        renderCmdViaProduction(
          planResumeIntoExistingTmux(DEFAULT_AGENT, a[0], a[1], a[2], { configDir: opt(a[3]) }),
        ) + "\n",
      );
      break;
    case "tmux-new":
      process.stdout.write(
        renderCmdViaProduction(
          planResumeTmux(DEFAULT_AGENT, a[0], a[1], a[2], a[3], { configDir: opt(a[4]) }),
        ) + "\n",
      );
      break;
    case "direct":
      process.stdout.write(
        renderCmdViaProduction(planResumeDirect(DEFAULT_AGENT, a[0], a[1], a[2], { configDir: opt(a[3]) })) +
          "\n",
      );
      break;
    case "follow": {
      // resolveFollowAccount(state, {lastAccount, current}) -> 名 or null(基座)
      const state = JSON.parse(a[2] ?? "{}");
      const res = resolveFollowAccount(state, {
        lastAccount: opt(a[0]) ?? null,
        current: opt(a[1]) ?? null,
      });
      process.stdout.write((res ?? "<base>") + "\n");
      break;
    }
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
