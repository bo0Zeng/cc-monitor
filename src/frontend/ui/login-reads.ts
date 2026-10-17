/**
 * 在 app 里登录那几条帧命令（`login-start` · `login-finish` · `login-cancel`）；会话外的需手动清单在各台 `sessions-needs` 那一份里（`needs-book.ts`）。
 *
 * | 做什么 | 帧命令 |
 * |---|---|
 * | 起一趟登录（回登录链接 · 粘什么 · 会不会自己登完） | `login-start`（帧面硬臂：跟着发起它的那条连接走） |
 * | 把登录页给的那一段交回去 | `login-finish` |
 * | 撤掉这一趟 | `login-cancel` |
 *
 * 判 · 拼句 · 能不能接着粘全在后端：失败回包的 `data.again` 说「接着粘 · 重新开始 · 没法再来」，这里只把它原样交出去，不按码分。
 */
import { budgetWithin, callTyped, refusalIn } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { detailOf, saidOf } from "./kit/detail";
import type { ManualItem } from "./generated/ManualItem";
import type { LoginStartReq } from "./generated/LoginStartReq";
import type { LoginStarted } from "./generated/LoginStarted";
import type { LoginAgainHow } from "./generated/LoginAgainHow";
import type { SignInState } from "./generated/SignInState";

export type { ManualItem, LoginStartReq, LoginStarted, LoginAgainHow, SignInState };

/** 起登录：那一家要先问一次服务端拿登录参数，后端自己至多等 30 秒（`sign_in::START_WITHIN`），这里多给一点。 */
const START_BUDGET_MS = 40_000;
/** 交码：后端至多等 60 秒（`sign_in::FINISH_WITHIN`）。 */
const FINISH_BUDGET_MS = 70_000;
const CANCEL_BUDGET_MS = 10_000;

/** 登录那几条没成：那一句（后端写好）· 原话 · 还能怎么办（后端 `data.again`）。 */
export class LoginFailed extends Error {
  constructor(
    readonly said: string,
    readonly detail: string,
    readonly again: LoginAgainHow,
  ) {
    super(said);
    this.name = "LoginFailed";
  }
}

/** 通道上的失败 ⇒ [`LoginFailed`]，那一句 monitor 写好。对端说不行 ⇒ 照它的 `data.again`；别的层（够不着 · 答没答）⇒ 作业在不在不知道 ⇒ 重新开始。 */
function failedOf(e: unknown): LoginFailed {
  if (e instanceof LoginFailed) return e;
  const r = refusalIn(e);
  return new LoginFailed(saidOf(e), detailOf(e), r ? againOf(r.data) : "restart");
}

/** 拒绝体里的 `data.again`；没带（只有 `bad_args`：入参形状不对，不是一条能走的路）⇒ 不给重来。 */
function againOf(data: unknown): LoginAgainHow {
  const a = data !== null && typeof data === "object" ? (data as { again?: unknown }).again : undefined;
  return a === "paste" || a === "restart" ? a : "none";
}

/**
 * 起一趟登录。`browserHere` ＝ 打开登录链接的浏览器在不在那台：桌面连本机 ⇒ 在，连远端 ⇒ 不在（后端照它答 `auto`）。
 * 同一个目标再起一趟，后端替掉旧的。
 */
export async function loginStart(origin: Origin, target: LoginStartReq): Promise<LoginStarted> {
  try {
    return await callTyped(origin, "login-start", { ...target, browserHere: isLocalOrigin(origin) }, budgetWithin(START_BUDGET_MS));
  } catch (e) {
    throw failedOf(e);
  }
}

/** 把登录页上给的那一段原样交回去。成了 ⇒ 返回；没成 ⇒ 抛 [`LoginFailed`]（照 `again` 画）。 */
export async function loginFinish(origin: Origin, job: string, pasted: string): Promise<void> {
  try {
    await callTyped(origin, "login-finish", { job, pasted }, budgetWithin(FINISH_BUDGET_MS));
  } catch (e) {
    throw failedOf(e);
  }
}

/** 撤掉这一趟（尽力：撤不了也不拦着关抽屉）。 */
export async function loginCancel(origin: Origin, job: string): Promise<void> {
  try {
    await callTyped(origin, "login-cancel", { job }, budgetWithin(CANCEL_BUDGET_MS));
  } catch (e) {
    console.warn("[login] login-cancel 没成：", e);
  }
}
