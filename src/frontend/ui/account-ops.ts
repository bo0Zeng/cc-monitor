/**
 * 账号库那几条命令：经通道问 `origin` 那台后端（本机远端同一条路），成品按生成的线上类型严格收。
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
 * 界面只交意图（名字 · 类型 · 凭据文件 / 地址与 key · 是否默认）、只显那台后端答的那几句；
 * 建目录、搭链接、写清单、放凭据或 key、重写别名文件都由那台后端做。每条都能先 `dryRun` 预演（回将要做的那几步）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import { exactKeys, isObj, settle, unreadable, type Refusals } from "./control-said";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { ACCOUNT_NAME_MAX, accountNameOk } from "./generated/judgment-rules";
import type { AccountAddArgs } from "./generated/AccountAddArgs";
import type { AccountChange } from "./generated/AccountChange";
import type { AccountInitArgs } from "./generated/AccountInitArgs";
import type { AccountRemoveArgs } from "./generated/AccountRemoveArgs";
import type { AccountRepairArgs } from "./generated/AccountRepairArgs";
import type { AccountRollbackArgs } from "./generated/AccountRollbackArgs";
import type { AliasChange } from "./generated/AliasChange";
import type { AccountMcpView } from "./generated/AccountMcpView";
import type { VerifyCheck } from "./generated/VerifyCheck";
import type { VerifyReport } from "./generated/VerifyReport";

export type { AccountAddArgs, AccountChange, AccountMcpView, VerifyReport, VerifyCheck };

export type NameCheck = { ok: true } | { ok: false; reason: string };

/** 账号名能不能用 —— 规则读生成物（`accountNameOk`，与后端建号 · `ccm --account` 同一条），这里只多说一句「空」。 */
export function validateAcctName(name: string): NameCheck {
  if (!name) return { ok: false, reason: copyText("accountOps.name.empty") };
  if (!accountNameOk(name)) return { ok: false, reason: copyText("accountOps.name.shape", { max: ACCOUNT_NAME_MAX }) };
  return { ok: true };
}

/** 改账号库的命令要读写一批小文件、可能整棵复制一个号的目录进备份：给足一分钟。 */
const CHANGE_BUDGET_MS = 60_000;
/** 只读 / 只算的那两条。 */
const READ_BUDGET_MS = 20_000;

function refusals(): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        // 契约对不上是两端版本不配；其余各档那台后端已经说成人话了，原样上屏（码不上屏）。
        case "bad_args":
          return copyText("accountOps.said.contract", { detail });
        default:
          return detail.trim() !== "" ? detail : copyText("accountOps.said.noReason");
      }
    },
    noReason: () => copyText("accountOps.said.noReason"),
  };
}

const CHANGE_KEYS = [
  "applied",
  "steps",
  "notes",
  "backup",
  "account",
  "loginCmd",
  "aliasNames",
  "keyMasked",
  "keyProblem",
  "aliases",
] as const;

const optStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const strList = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");

function aliasChangeOk(v: unknown): v is AliasChange {
  return (
    isObj(v) &&
    exactKeys(v, ["path", "changed", "added", "removed", "skipped", "note"]) &&
    typeof v.path === "string" &&
    typeof v.changed === "boolean" &&
    strList(v.added) &&
    strList(v.removed) &&
    strList(v.skipped) &&
    optStr(v.note)
  );
}

/** 改账号库那几条的成品。形状不对 ⇒ `null`（调用方说「读不懂」）。 */
export function decodeAccountChange(v: unknown): AccountChange | null {
  if (!isObj(v) || !exactKeys(v, CHANGE_KEYS)) return null;
  const acct = v.account;
  const acctOk =
    acct === null ||
    (isObj(acct) && exactKeys(acct, ["name", "configDir"]) && typeof acct.name === "string" && typeof acct.configDir === "string");
  if (
    typeof v.applied !== "boolean" ||
    !strList(v.steps) ||
    !strList(v.notes) ||
    !optStr(v.backup) ||
    !acctOk ||
    !optStr(v.loginCmd) ||
    !strList(v.aliasNames) ||
    !optStr(v.keyMasked) ||
    !optStr(v.keyProblem) ||
    !Array.isArray(v.aliases) ||
    !v.aliases.every(aliasChangeOk)
  )
    return null;
  return v as unknown as AccountChange;
}

/** 核对的成品。形状不对 ⇒ `null`。 */
export function decodeVerifyReport(v: unknown): VerifyReport | null {
  if (!isObj(v) || !exactKeys(v, ["pass", "fails", "warns", "checks"])) return null;
  if (typeof v.pass !== "boolean" || typeof v.fails !== "number" || typeof v.warns !== "number" || !Array.isArray(v.checks)) return null;
  const levels = ["ok", "warn", "fail", "skip"];
  const ok = v.checks.every(
    (c: unknown) =>
      isObj(c) &&
      exactKeys(c, ["level", "account", "text"]) &&
      typeof c.level === "string" &&
      levels.includes(c.level) &&
      optStr(c.account) &&
      typeof c.text === "string",
  );
  return ok ? (v as unknown as VerifyReport) : null;
}

function change(origin: Origin, op: string, v: unknown): AccountChange {
  const got = decodeAccountChange(v);
  if (!got) throw unreadable(origin, op, "is not an account change");
  return got;
}

/** 建账号库。 */
export async function accountsInit(origin: Origin, args: AccountInitArgs): Promise<AccountChange> {
  const body = jsonBody(args);
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return change(origin, "accounts-init", await settle(origin, "accounts-init", chan.call(origin, "accounts-init", body, budget), refusals()));
}

/** 新建一个号（API 号的 key 在 `args.key` 里，只在这一发里出现一次；预演时别带它）。 */
export async function accountsAdd(origin: Origin, args: AccountAddArgs): Promise<AccountChange> {
  const body = jsonBody(args);
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return change(origin, "accounts-add", await settle(origin, "accounts-add", chan.call(origin, "accounts-add", body, budget), refusals()));
}

/** 设这台的默认号（那台账号库清单里 `isDefault` 那一格；跟随时没有上次的号就落它）。 */
export async function accountsSetDefault(origin: Origin, name: string): Promise<AccountChange> {
  const body = jsonBody({ name });
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return change(origin, "accounts-set-default", await settle(origin, "accounts-set-default", chan.call(origin, "accounts-set-default", body, budget), refusals()));
}

/** 删一个号。 */
export async function accountsRemove(origin: Origin, args: AccountRemoveArgs): Promise<AccountChange> {
  const body = jsonBody(args);
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return change(origin, "accounts-remove", await settle(origin, "accounts-remove", chan.call(origin, "accounts-remove", body, budget), refusals()));
}

/** 修复。 */
export async function accountsRepair(origin: Origin, args: AccountRepairArgs): Promise<AccountChange> {
  const body = jsonBody(args);
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return change(origin, "accounts-repair", await settle(origin, "accounts-repair", chan.call(origin, "accounts-repair", body, budget), refusals()));
}

/** 按备份还原（`backup` 缺席 ⇒ 最近一份还没还原过的）。 */
export async function accountsRollback(origin: Origin, args: AccountRollbackArgs): Promise<AccountChange> {
  const body = jsonBody(args);
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return change(origin, "accounts-rollback", await settle(origin, "accounts-rollback", chan.call(origin, "accounts-rollback", body, budget), refusals()));
}

/** 核对（只读）。 */
export async function accountsVerify(origin: Origin): Promise<VerifyReport> {
  const body = jsonBody({});
  const budget = budgetWithin(READ_BUDGET_MS);
  const v = await settle(origin, "accounts-verify", chan.call(origin, "accounts-verify", body, budget), refusals());
  const got = decodeVerifyReport(v);
  if (!got) throw unreadable(origin, "accounts-verify", "is not a verify report");
  return got;
}

/** 在终端里起 claude 登录 `name` 那一行（那台后端出、已 quote）。 */
export async function accountsLoginCmd(origin: Origin, name: string): Promise<string> {
  const body = jsonBody({ name });
  const budget = budgetWithin(READ_BUDGET_MS);
  const v = await settle(origin, "accounts-login-cmd", chan.call(origin, "accounts-login-cmd", body, budget), refusals());
  if (!isObj(v) || !exactKeys(v, ["cmd"]) || typeof v.cmd !== "string" || v.cmd === "") {
    throw unreadable(origin, "accounts-login-cmd", "is not exactly {cmd} (one non-empty string)");
  }
  return v.cmd;
}

/** 各号共用的用户级 MCP 那四条的成品。形状不对 ⇒ `null`。 */
export function decodeAccountMcpView(v: unknown): AccountMcpView | null {
  if (!isObj(v) || !exactKeys(v, ["enabled", "sync", "servers", "conflicts", "changed", "notes"])) return null;
  if (typeof v.enabled !== "boolean" || typeof v.sync !== "boolean" || !strList(v.servers) || !strList(v.changed) || !strList(v.notes) || !Array.isArray(v.conflicts)) return null;
  const choiceOk = (x: unknown): boolean =>
    isObj(x) && exactKeys(x, ["from", "holders", "gone"]) && optStr(x.from) && strList(x.holders) && typeof x.gone === "boolean";
  const ok = v.conflicts.every(
    (c: unknown) => isObj(c) && exactKeys(c, ["name", "choices"]) && typeof c.name === "string" && Array.isArray(c.choices) && c.choices.every(choiceOk),
  );
  return ok ? (v as unknown as AccountMcpView) : null;
}

function mcpView(origin: Origin, op: string, v: unknown): AccountMcpView {
  const got = decodeAccountMcpView(v);
  if (!got) throw unreadable(origin, op, "is not a shared MCP view");
  return got;
}

/** 这台各号共用的用户级 MCP 此刻的样子（只读）。 */
export async function accountsMcpRead(origin: Origin): Promise<AccountMcpView> {
  const body = jsonBody({});
  const budget = budgetWithin(READ_BUDGET_MS);
  return mcpView(origin, "accounts-mcp-read", await settle(origin, "accounts-mcp-read", chan.call(origin, "accounts-mcp-read", body, budget), refusals()));
}

/** 从各号共用的用户级 MCP 里删一条（所有号一起撤）。 */
export async function accountsMcpRemove(origin: Origin, name: string): Promise<AccountMcpView> {
  const body = jsonBody({ name });
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return mcpView(origin, "accounts-mcp-remove", await settle(origin, "accounts-mcp-remove", chan.call(origin, "accounts-mcp-remove", body, budget), refusals()));
}

/** 两边都改了的那一条用哪一版：`from` = 那个号里的；`null` = 共享的那一版。 */
export async function accountsMcpPick(origin: Origin, name: string, from: string | null): Promise<AccountMcpView> {
  const body = jsonBody(from === null ? { name } : { name, from });
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return mcpView(origin, "accounts-mcp-pick", await settle(origin, "accounts-mcp-pick", chan.call(origin, "accounts-mcp-pick", body, budget), refusals()));
}

/** 停 / 开各号之间同步（停了已同步的不删；开回来那一刻同步一趟）。 */
export async function accountsMcpSync(origin: Origin, on: boolean): Promise<AccountMcpView> {
  const body = jsonBody({ on });
  const budget = budgetWithin(CHANGE_BUDGET_MS);
  return mcpView(origin, "accounts-mcp-sync", await settle(origin, "accounts-mcp-sync", chan.call(origin, "accounts-mcp-sync", body, budget), refusals()));
}
