// auto-e2e F-E2:resume 命令**真源驱动器**（测试 fixture，非生产改动）。
//
// 目的:让 bash 套件拿到「app 真正会跑的 resume 命令串 / 账号解析结果」——直接 import
// src/ 里的**真实**纯函数(remote-launch.ts 命令构造 + accounts.ts 账号解析),绝不在 shell 里
// 重写一份(那样测的是复制品、不是被测代码)。#75(CLAUDE_CONFIG_DIR 注入) / #76(复用 cc-<sid8>
// 名不产 -N 孤儿) 的修复都活在这些函数里,套件据本驱动器的 stdout 断言并真跑到 tmux。
//
// 用法(每个 mode 打印一行 stdout):
//   into-existing <sid> <name> <launcher> [configDir]   -> buildResumeIntoExistingTmuxCmd
//   tmux-new      <sid> <cwd> <launcher> <name> [configDir] -> buildResumeTmuxCmd
//   direct        <sid> <cwd> <launcher> [configDir]      -> buildResumeDirectCmd
//   pick-fresh    <sid> <existing-comma-list>             -> pickFreshTmuxName
//   env-prefix    [configDir]                             -> buildEnvPrefix
//   follow        <lastAccount|-> <current|-> <stateJson> -> resolveFollowAccount(名或 "<base>")
//   acct-dir      <name> <stateJson>                      -> accountConfigDir(路径或 "<none>")
//
// configDir 传字面 "-" 或省略 = undefined(基座,无账号注入)。
import {
  buildResumeIntoExistingTmuxCmd,
  buildResumeTmuxCmd,
  buildResumeDirectCmd,
  pickFreshTmuxName,
  buildEnvPrefix,
} from "../src/remote-launch.ts";
import { resolveFollowAccount, accountConfigDir } from "../src/accounts.ts";

function opt(v: string | undefined): string | undefined {
  return v === undefined || v === "-" || v === "" ? undefined : v;
}

const [mode, ...a] = process.argv.slice(2);
try {
  switch (mode) {
    case "into-existing":
      process.stdout.write(
        buildResumeIntoExistingTmuxCmd(a[0], a[1], a[2], opt(a[3])) + "\n",
      );
      break;
    case "tmux-new":
      process.stdout.write(
        buildResumeTmuxCmd(a[0], a[1], a[2], a[3], opt(a[4])) + "\n",
      );
      break;
    case "direct":
      process.stdout.write(buildResumeDirectCmd(a[0], a[1], a[2], opt(a[3])) + "\n");
      break;
    case "pick-fresh": {
      const existing = new Set((a[1] ?? "").split(",").filter(Boolean));
      process.stdout.write(pickFreshTmuxName(a[0], existing) + "\n");
      break;
    }
    case "env-prefix":
      process.stdout.write(buildEnvPrefix(opt(a[0])) + "\n");
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
