// A6：cc-acct-iso 部署 / 维护那几条命令 —— 设置「账号」组的内联向导把「要在那台机器的终端里跑的命令」交给
// `terminal-open.ts::openTerminal` 弹一个真实终端，让用户**亲眼看着、亲手确认**（DESIGN §6：动凭据的一切走终端，不经后端代跑；本模块不落盘、不读凭据）。
//
// 〔DUP2 · 主会话 09-26 裁 J4〕**这条命令由那台机器的后端出**（帧命令 `acct-iso-cmd`，`src/backend/accounts/iso.rs`），
// 预览与弹终端都经 `chan.call` 问它 —— 前端零拼 shell 串（`设计/90 §3` 判据 1 / 2 · `设计/01 §1.1`「命令串……都不在前端」）。
// 这里原来的 `sq`〔散文墓碑〕（POSIX 单引号，与 `shell_quote_core::posix_quote` 同一件事的第二份）· `buildAcctIsoCmd`〔散文墓碑〕
// （拼命令串）· 快照路径那道校验都搬进了后端；账号名那一格的**规则**在后端（`shell_quote_core::account_name_ok`），
// 表单要逐字即时反馈的那一句读生成物（下面的 `validateAcctName`，DUP2 · J18）。本机与远端同一条命令，`origin` 区分（`设计/01 §6.8`）。

import { copyText } from "../copy-table";
import { exactKeys, isObj, settle, unreadable, type Refusals } from "../control-said";
// 〔DUP2 · `设计/90 §3` 判据 2〕账号名规则只有一份（`shell_quote_core::account_name_ok`，与建账号的工具 `cc-acct-iso` 逐字同）；
// 这里读它现生成的那份（`src/generated/judgment-rules.ts`），不手抄。
import { ACCOUNT_NAME_MAX, accountNameOk } from "../generated/judgment-rules";
import { chan } from "../ipc/chan";
import { budgetWithin, jsonBody } from "../ipc/chan-caller";
import type { Origin } from "../ipc/origin";

// 〔MIG-3a · 主会话 09-28 预裁〕`deriveAcctIsoDir`〔散文墓碑〕（按用户名猜那台的部署目录）删了：落点由那台后端按自己的家目录算（`acct-iso-install`）。

export type NameCheck = { ok: true } | { ok: false; reason: string };

/**
 * 账号名能不能用 —— **按生成物求值的薄壳**（DUP2 · J18）：规则 = `accountNameOk`（生成物），这里只多说一句「空」。
 *
 * 〔DUP2〕这里原来手写了一份更宽的规则（放行 `.`、≤64、只禁 `-` / `.` 开头）—— 比建账号的那个工具宽：
 * 表单放行的名字，工具在终端里拒。今天与工具、与 Rust 那一份（`ccm …` 的 `--account` · 后端 ccm argv）是同一条。
 */
export function validateAcctName(name: string): NameCheck {
  if (!name) return { ok: false, reason: copyText("acctDeploy.name.empty") };
  if (!accountNameOk(name)) return { ok: false, reason: copyText("acctDeploy.name.shape", { max: ACCOUNT_NAME_MAX }) };
  return { ok: true };
}

export type AcctIsoStep =
  | { kind: "init-preview"; name: string } // dry-run：零落盘（A1 测试已断言）
  | { kind: "init-apply"; name: string } // 落盘迁移（用户在终端里看着跑）
  | { kind: "verify" }
  | { kind: "shellinit" }
  | { kind: "sync-apply" }
  | { kind: "add-apply"; name: string; credFile?: string }
  | { kind: "login"; name: string }; // cc-acct-iso run <名>：该号唯一登录入口（去 /login）

/** 问一次要多久算没答（显式期限，`X6`）。纯函数、不起进程、不碰盘 —— 给足通道往返的余量。 */
const CMD_BUDGET_MS = 10_000;

/** 一个步骤 ⇒ 线上那几格（键名一字不差，后端 `accounts/iso.rs::parse_cmd_args` 按形状严格收）。 */
function stepArgs(step: AcctIsoStep): Record<string, unknown> {
  const a: Record<string, unknown> = { step: step.kind };
  if ("name" in step) a.name = step.name;
  if (step.kind === "add-apply" && step.credFile !== undefined) a.credFile = step.credFile;
  return a;
}

function cmdRefusals(): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "refused":
          return copyText("acctDeploy.cmd.refused", { detail });
        case "bad_args":
          return copyText("acctDeploy.cmd.contract", { detail });
        default:
          return copyText("acctDeploy.cmd.otherCode", { code, detail });
      }
    },
    noReason: () => copyText("acctDeploy.cmd.noReason"),
  };
}

/**
 * 问 `origin` 那台机器的后端：这一步在终端里要跑的那一行是什么（`acct-iso-cmd`）。
 * 拿到的就是成品（已 quote、已校验），原样交给 `openTerminal` / 原样上屏；问不到 ⇒ 抛 [`ControlError`]（那一句已说好）。
 */
export async function askAcctIsoCmd(origin: Origin, step: AcctIsoStep): Promise<string> {
  const payload = jsonBody(stepArgs(step));
  const budget = budgetWithin(CMD_BUDGET_MS);
  const v = await settle(origin, "acct-iso-cmd", chan.call(origin, "acct-iso-cmd", payload, budget), cmdRefusals());
  if (!isObj(v) || !exactKeys(v, ["cmd"]) || typeof v.cmd !== "string") {
    throw unreadable(origin, "acct-iso-cmd", "is not exactly {cmd} (one string)");
  }
  return v.cmd;
}
