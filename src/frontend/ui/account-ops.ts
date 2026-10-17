/**
 * 账号库那几条命令：经通道问 `origin` 那台后端（本机远端同一条路）。
 *
 * | 做什么 | 帧命令 |
 * |---|---|
 * | 建账号库（现在登录的身份收成默认号） | `accounts-init` |
 * | 新建一个号（订阅 / API；可导入凭据；可设默认） | `accounts-add` |
 * | 设默认号（起会话跟随时没有上次的号就落它） | `accounts-set-default` |
 * | 删一个号 | `accounts-remove` |
 * | 修复（补链接 · 修权限 · 刷新邮箱 · 补别名） | `accounts-repair` |
 * | 按备份还原 | `accounts-rollback` |
 * | 核对（只读） | `accounts-verify` |
 * | 在终端里登录一个号的那一行 | `accounts-login-cmd` |
 * | 各号共用的用户级 MCP：看 · 删一条 · 两边都改了时挑一版 | `accounts-mcp-read` · `accounts-mcp-remove` · `accounts-mcp-pick` |
 *
 * 都是 typed 帧命令（`callTyped`）：入参 · 成品的类型从 Rust 生成，形状由握手时的指纹核过，这里不再逐格解。
 * 界面只交意图（名字 · 类型 · 凭据文件 / 地址与 key · 是否默认）、只显那台后端答的那几句；
 * 建目录、搭链接、写清单、放凭据或 key、重写别名文件都由那台后端做。每条都能先 `dryRun` 预演（回将要做的那几步）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { settleTyped } from "./control-said";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { ACCOUNT_NAME_MAX, accountNameOk } from "./generated/judgment-rules";
import type { AccountAddArgs } from "./generated/AccountAddArgs";
import type { AccountChange } from "./generated/AccountChange";
import type { AccountInitArgs } from "./generated/AccountInitArgs";
import type { AccountRemoveArgs } from "./generated/AccountRemoveArgs";
import type { AccountRepairArgs } from "./generated/AccountRepairArgs";
import type { AccountRollbackArgs } from "./generated/AccountRollbackArgs";
import type { AccountMcpView } from "./generated/AccountMcpView";
import type { VerifyCheck } from "./generated/VerifyCheck";
import type { VerifyReport } from "./generated/VerifyReport";

export type {
  AccountAddArgs,
  AccountChange,
  AccountMcpView,
  VerifyReport,
  VerifyCheck,
};

export type NameCheck = { ok: true } | { ok: false; reason: string };

/** 账号名能不能用 —— 规则读生成物（`accountNameOk`，与后端建号 · `ccm --account` 同一条），这里只多说一句「空」。 */
export function validateAcctName(name: string): NameCheck {
  if (!name) return { ok: false, reason: copyText("accountOps.name.empty") };
  if (!accountNameOk(name))
    return {
      ok: false,
      reason: copyText("accountOps.name.shape", { max: ACCOUNT_NAME_MAX }),
    };
  return { ok: true };
}

/** 改账号库的命令要读写一批小文件、可能整棵复制一个号的目录进备份：给足一分钟。 */
const CHANGE_BUDGET_MS = 60_000;
/** 只读 / 只算的那两条。 */
const READ_BUDGET_MS = 20_000;

/** 建账号库。 */
export async function accountsInit(
  origin: Origin,
  args: AccountInitArgs,
): Promise<AccountChange> {
  return settleTyped(
    origin,
    "accounts-init",
    callTyped(origin, "accounts-init", args, budgetWithin(CHANGE_BUDGET_MS)),
  );
}

/** 新建一个号（API 号的 key 在 `args.key` 里，只在这一发里出现一次；预演时别带它）。 */
export async function accountsAdd(
  origin: Origin,
  args: AccountAddArgs,
): Promise<AccountChange> {
  return settleTyped(
    origin,
    "accounts-add",
    callTyped(origin, "accounts-add", args, budgetWithin(CHANGE_BUDGET_MS)),
  );
}

/** 设这台的默认号（那台账号库清单里 `isDefault` 那一格；跟随时没有上次的号就落它）。 */
export async function accountsSetDefault(
  origin: Origin,
  name: string,
): Promise<AccountChange> {
  return settleTyped(
    origin,
    "accounts-set-default",
    callTyped(
      origin,
      "accounts-set-default",
      { name },
      budgetWithin(CHANGE_BUDGET_MS),
    ),
  );
}

/** 删一个号。 */
export async function accountsRemove(
  origin: Origin,
  args: AccountRemoveArgs,
): Promise<AccountChange> {
  return settleTyped(
    origin,
    "accounts-remove",
    callTyped(origin, "accounts-remove", args, budgetWithin(CHANGE_BUDGET_MS)),
  );
}

/** 修复。 */
export async function accountsRepair(
  origin: Origin,
  args: AccountRepairArgs,
): Promise<AccountChange> {
  return settleTyped(
    origin,
    "accounts-repair",
    callTyped(origin, "accounts-repair", args, budgetWithin(CHANGE_BUDGET_MS)),
  );
}

/** 按备份还原（`backup` 缺席 ⇒ 最近一份还没还原过的）。 */
export async function accountsRollback(
  origin: Origin,
  args: AccountRollbackArgs,
): Promise<AccountChange> {
  return settleTyped(
    origin,
    "accounts-rollback",
    callTyped(
      origin,
      "accounts-rollback",
      args,
      budgetWithin(CHANGE_BUDGET_MS),
    ),
  );
}

/** 核对（只读）。 */
export async function accountsVerify(origin: Origin): Promise<VerifyReport> {
  return settleTyped(
    origin,
    "accounts-verify",
    callTyped(origin, "accounts-verify", {}, budgetWithin(READ_BUDGET_MS)),
  );
}

/** 在终端里起 claude 登录 `name` 那一行（那台后端出、已 quote；「⋯」里的逃生口用）。 */
export async function accountsLoginCmd(
  origin: Origin,
  name: string,
): Promise<string> {
  const v = await settleTyped(
    origin,
    "accounts-login-cmd",
    callTyped(
      origin,
      "accounts-login-cmd",
      { name },
      budgetWithin(READ_BUDGET_MS),
    ),
  );
  return v.cmd;
}

/** 这台各号共用的用户级 MCP 此刻的样子（只读）。 */
export async function accountsMcpRead(origin: Origin): Promise<AccountMcpView> {
  return settleTyped(
    origin,
    "accounts-mcp-read",
    callTyped(origin, "accounts-mcp-read", {}, budgetWithin(READ_BUDGET_MS)),
  );
}

/** 从各号共用的用户级 MCP 里删一条（所有号一起撤）。 */
export async function accountsMcpRemove(
  origin: Origin,
  name: string,
): Promise<AccountMcpView> {
  return settleTyped(
    origin,
    "accounts-mcp-remove",
    callTyped(
      origin,
      "accounts-mcp-remove",
      { name },
      budgetWithin(CHANGE_BUDGET_MS),
    ),
  );
}

/** 两边都改了的那一条用哪一版：`from` = 那个号里的；`null` = 共享的那一版。 */
export async function accountsMcpPick(
  origin: Origin,
  name: string,
  from: string | null,
): Promise<AccountMcpView> {
  const req = from === null ? { name } : { name, from };
  return settleTyped(
    origin,
    "accounts-mcp-pick",
    callTyped(origin, "accounts-mcp-pick", req, budgetWithin(CHANGE_BUDGET_MS)),
  );
}

/** 停 / 开各号之间同步（停了已同步的不删；开回来那一刻同步一趟）。 */
export async function accountsMcpSync(
  origin: Origin,
  on: boolean,
): Promise<AccountMcpView> {
  return settleTyped(
    origin,
    "accounts-mcp-sync",
    callTyped(
      origin,
      "accounts-mcp-sync",
      { on },
      budgetWithin(CHANGE_BUDGET_MS),
    ),
  );
}
