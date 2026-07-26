// auto-e2e F-E3:换号重启编排的**真源驱动器**（测试 fixture,非生产改动）。仿 e2e/resume-cmd-driver.ts,
// 但驱动的是**有副作用的编排** `restartWithAccount`（src/account-restart.ts,真源,绝不重写一份），
// 经 restart-shims/loader.mjs 把 Tauri IPC 边界重定向到真 tmux + fake-claude（见该 loader 头注）。
//
// 诚实天花板 = 命令级:真编排逻辑 + 真 tmux 效果 + 真账号解析（accountConfigDir/detectAccountMismatch
// 都是 src/accounts.ts 真源）。GUI 全链在 Linux 结构性不可达（launch.rs 仅 Windows→回退剪贴板）。
//
// 用法:
//   restart <origin> <sid> <cwd> <tmuxName> <account> <launcher> <compactFirst> <confirm> <awaitCompact> <awaitExit>
//        驱动真编排;布尔用 1/0。序列写进 $CCM_SEQ_LOG(shim 落),本进程 stdout 打印:
//          RESULT <true|false>          （restartWithAccount 返回值）
//          CONFIGDIR <dir|none>         （真 accountConfigDir 解析目标账号目录）
//   mismatch <liveAccount|-> <current|->   -> 真 detectAccountMismatch,打印 "true"/"false"
//   acct-dir <name>                        -> 真 accountConfigDir(CCM_ACCOUNTS_JSON),打印路径或 "none"
//
// 账号 fixture 经 $CCM_ACCOUNTS_JSON（RawAccountsResult 形状）注入。
import module from "node:module";

// 必须在动态 import 真源之前登记钩子（同 tick 登记 → 后续 import 生效）。
module.register(new URL("./restart-shims/loader.mjs", import.meta.url).href);

const { restartWithAccount } = await import("../src/account-restart.ts");
const { detectAccountMismatch, accountConfigDir } = await import("../src/accounts.ts");

function opt(v: string | undefined): string | undefined {
  return v === undefined || v === "-" || v === "" ? undefined : v;
}
function bool(v: string | undefined): boolean {
  return v === "1" || v === "true";
}
function stateFromEnv(): { accounts: unknown[]; [k: string]: unknown } {
  const raw = JSON.parse(process.env.CCM_ACCOUNTS_JSON || '{"accounts":[]}');
  // fetchAccounts 构造 AccountsState 时补 defaultName;accountConfigDir 只读 accounts,补足即可。
  return { origin: "aya", available: true, error: null, meta: null, defaultName: null, ...raw };
}

const [mode, ...a] = process.argv.slice(2);
try {
  switch (mode) {
    case "restart": {
      const ok = await restartWithAccount({
        origin: a[0],
        sessionId: a[1],
        cwd: a[2],
        tmuxName: a[3],
        accountName: a[4],
        launcher: a[5],
        compactFirst: bool(a[6]),
        confirm: () => bool(a[7]),
        awaitCompact: async () => bool(a[8]),
        awaitExit: async () => bool(a[9]),
      });
      const dir = accountConfigDir(stateFromEnv() as never, a[4]);
      process.stdout.write(`RESULT ${ok}\n`);
      process.stdout.write(`CONFIGDIR ${dir ?? "none"}\n`);
      break;
    }
    case "mismatch": {
      const res = detectAccountMismatch(opt(a[0]) ?? null, opt(a[1]) ?? null);
      process.stdout.write(String(res) + "\n");
      break;
    }
    case "acct-dir": {
      const dir = accountConfigDir(stateFromEnv() as never, a[0]);
      process.stdout.write((dir ?? "none") + "\n");
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
