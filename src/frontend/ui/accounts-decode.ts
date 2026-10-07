/**
 * 后端账号那两条帧命令的**成品** ⇒ 界面要的形状（按形状收，不解释）。
 *
 * - `accounts-list` ⇒ [`decodeAccountsList`]：`{meta, accounts, notice}`，并表（apikey 表 ⇒ `authKind` / `authReady`）
 *   **已在那台机器的后端做完**（`src/backend/observe/accounts_query.rs::list_product`，规则住 `acct-core`）；
 * - `accounts-trust` ⇒ [`decodeTrust`]：`{trusted, known}`。
 *
 * 为什么单独一个文件：`tests/frontend/ui/account-availability-guard.vitest.ts`（KAY4）数着「生产段里谁在读账号的鉴权字段」——
 * 收成品那一格必须逐字段核类型（读到了才核得了），它是**收**，不是**判**：可用性仍只由 `accounts.ts::isSelectable` 答。
 * 放在这里、在那张登记表里单独一行，两件事分得开。
 * 跨语言金样 `tests/__fixtures__/accounts.golden.json` 钉着后端出的形状与这里收的形状（`tests/frontend/ui/accounts-decode.vitest.ts`）。
 */
import type { Account, AccountsMeta, AuthKind } from "./accounts";
import { copyText } from "./copy-table";
import { ReplyUnreadable } from "./ipc/chan-caller";
// 账号种类的取值集只有一份（`acct_core::AUTH_KINDS`），这里读它现生成的那份，不手抄。
import { AUTH_KINDS } from "./generated/judgment-rules";
import { exactKeys, isObj } from "./ipc/decode";

const nullableStr = (v: unknown): v is string | null => v === null || typeof v === "string";

/**
 * 后端 `accounts-list` 的成品 ⇒ 界面那三格。**严格收**（口径同 C4b 的 `decodeSurvey`）：
 * 顶层 / `meta` / 每个账号的键集合都要恒等，类型逐格核；多一格、缺一格、类型不对 ⇒ 抛「两端契约对不上」——
 * 不替后端补一个值（那会把一个坏掉的号悄悄变成一个能选的号）。老后端回的 `{"lines": […]}` 也落这里。
 * 跨语言金样 `tests/__fixtures__/accounts.golden.json` 钉着两侧。
 */
export function decodeAccountsList(v: unknown): {
  meta: AccountsMeta;
  accounts: Account[];
  notice: string | null;
} {
  const bad = (what: string): never => {
    throw new ReplyUnreadable(`accounts-list reply: ${what}`);
  };
  if (!isObj(v) || !exactKeys(v, ["meta", "accounts", "notice"])) return bad(copyText("accountsDecode.where.top"));
  const m = v.meta;
  if (
    !isObj(m) ||
    !exactKeys(m, ["enabled", "acctsDir", "manifestPath", "updatedAt", "sharedStore", "count", "error", "unsupported", "nextDefault", "home"]) ||
    typeof m.enabled !== "boolean" ||
    typeof m.acctsDir !== "string" ||
    typeof m.manifestPath !== "string" ||
    !nullableStr(m.updatedAt) ||
    !nullableStr(m.sharedStore) ||
    typeof m.count !== "number" ||
    !nullableStr(m.error) ||
    !nullableStr(m.unsupported) ||
    !nullableStr(m.nextDefault) ||
    !nullableStr(m.home)
  ) {
    return bad("meta");
  }
  if (!Array.isArray(v.accounts)) return bad("accounts");
  if (!nullableStr(v.notice)) return bad("notice");
  const accounts: Account[] = v.accounts.map((a, i) => {
    if (
      !isObj(a) ||
      !exactKeys(a, [
        "name",
        "email",
        "configDir",
        "isDefault",
        "mode",
        "exists",
        "loggedIn",
        "authKind",
        "authReady",
        "keyMasked",
        "baseUrl",
      ]) ||
      typeof a.name !== "string" ||
      typeof a.email !== "string" ||
      !nullableStr(a.configDir) ||
      typeof a.isDefault !== "boolean" ||
      typeof a.mode !== "string" ||
      typeof a.exists !== "boolean" ||
      typeof a.loggedIn !== "boolean" ||
      !AUTH_KINDS.includes(a.authKind as AuthKind) ||
      typeof a.authReady !== "boolean" ||
      !nullableStr(a.keyMasked) ||
      !nullableStr(a.baseUrl)
    ) {
      return bad(copyText("accountsDecode.where.nth", { i }));
    }
    return {
      name: a.name,
      email: a.email,
      configDir: a.configDir,
      isDefault: a.isDefault,
      mode: a.mode,
      exists: a.exists,
      loggedIn: a.loggedIn,
      authKind: a.authKind as AuthKind,
      authReady: a.authReady,
      keyMasked: a.keyMasked,
      baseUrl: a.baseUrl,
    };
  });
  const meta: AccountsMeta = {
    enabled: m.enabled,
    acctsDir: m.acctsDir,
    manifestPath: m.manifestPath,
    updatedAt: m.updatedAt,
    sharedStore: m.sharedStore,
    count: m.count,
    error: m.error,
    unsupported: m.unsupported,
    nextDefault: m.nextDefault,
    home: m.home,
  };
  return { meta, accounts, notice: v.notice };
}

/** 后端 `accounts-trust` 的成品 ⇒ `{trusted, known}`。严格收（同上）。 */
export function decodeTrust(v: unknown): { trusted: boolean; known: boolean } {
  if (
    !isObj(v) ||
    !exactKeys(v, ["trusted", "known"]) ||
    typeof v.trusted !== "boolean" ||
    typeof v.known !== "boolean"
  ) {
    throw new ReplyUnreadable("accounts-trust reply shape");
  }
  return { trusted: v.trusted, known: v.known };
}
