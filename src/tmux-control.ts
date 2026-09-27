/**
 * 〔C4e · 第四波 4C · `设计/05 §8` 步 5〕**界面直接说的 tmux 控制类帧命令** —— 抓一屏（`capture-pane`）·
 * 结束会话（`kill`）· 往会话里发按键 / 就地恢复（`launch` 的 `send-into` / `send-keys-raw` 两个 mode）。
 *
 * # 它顶掉了什么
 *
 * 此前界面经 monitor 的四条 Tauri 命令做这几件事：`capture_remote_pane` / `kill_remote_tmux` / `tmux_send_keys` /
 * `backend_send_into`〔散文墓碑〕。monitor 在每一条上做的都只是：先拒空目标、转一条后端帧命令、核应答那一格、
 * 把拒绝码与「通道不在」说成人话、给就地恢复判「能不能回落」—— 那一份解释住 monitor 中层（`backend/control/`
 * 的 `tmux.rs` · `backend_kill.rs` · `backend_send_keys.rs` · `backend_launch.rs`）。后端的帧应答本来就是成品
 * ⇒ 按 `设计/05 §14.3` 的判准（业务解释只有一个家）迁：界面经 `chan.call` 直接问那台机器的后端，
 * 解释只剩本文件这一份，monitor 那四条命令与那一份解释一起删了。**本机与远端同一条路**（本机由 `<local>` 那条长连接答）。
 *
 * # 本文件做的只有四件（都是调用方那一侧的事）
 *
 * 1. **不判目标名**（〔DUP3 · 主会话 09-26 裁〕`§34` Gate 1 并进 `gate-core` 的 tmux 名那一族，TS 零）：会话名原样交给后端；
 *    空目标由后端入口拒（`invalid_args`，`=:` 会被 tmux 读成「当前会话」那一格），本文件照各动作那句「后端不接受这个会话名」
 *    带上后端原话说出来。**Gate 2 / 3（身份门 · 窗口门）只在后端 `control/gate.rs`**，本文件不写第二份。
 *    （先前这里有一道「空目标就地拒、一个字节都不发」—— 那是 Gate 1 在界面的一份，`设计/90 §3` 判据 2 删了。）
 * 2. **按形状收**：成品恰好是那几格、类型对 ⇒ 收；多一格 / 缺一格 / 类型不对 ⇒ 当成「两边版本对不上」抛，不猜。
 *    破坏性的两件（结束 · 发按键）还要成品**明说做成了**（`killed` / `typed` 为真）—— 形状不对或没说做成
 *    ⇒ 当成「不知道做了没有」，**不当成功、也不换条路重做**。
 *    线上形状由跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 钉着（后端产出 == 金样 · 本文件读同一份）。
 * 3. **失败怎么说**（`§3.3.2`「说法归调用方」）：拒绝码 → 一句话（逐码分开，认不出的码原样带出去，不猜）；
 *    通道的三层错误 → 一句话（本机与远端的下一步不同，话就不一样）。句子住文案表 `tmuxControl.*`。
 * 4. **就地恢复能不能回落**（F14）：只有**能证明这条命令一个字节都没到对端**才许回落到那条整串
 *    （`ipc/chan-caller.ts::provablyNotSent`，与 Rust `backend_route::route_call_error` 同一条规则，
 *    跨语言金样 `tests/__fixtures__/reach-collapse.golden.json` 钉着两份）。那条整串**没有 §34 的门**：
 *    把一次「被门拒绝」或「后端已键入但应答超时」回落过去，就是用一条无门的路重做一遍
 *    （后者会把载荷第二次键入一个已经在跑 claude 的 pane ⇒ 被当成 prompt 提交，**不可撤销**）。
 *
 * # 送键为什么是两个 mode 名，不是一个 `enter` 字段
 *
 * 后端的 `parse_request` 不拒认不出的键 ⇒ 旧后端会**静默忽略**一个 `enter` 字段、照样附回车 ⇒
 * 把「打断当前回合」（`Escape`）变成「提交输入框里排队的文本」。新 mode 名 `send-keys-raw` 在旧后端上
 * 回 `invalid_args` —— 天然 fail-closed（F04c 的理由，随发送端从 monitor 搬到这里）。
 *
 * # 期限（`X6`：调用点显式给）
 *
 * 抓一屏 20 秒；结束 / 发按键 / 就地恢复 10 秒 —— 与它们上一个住址（monitor 那几个发送端）同值。
 */
import { copyText } from "./copy-table";
import {
  ControlError,
  exactKeys,
  isObj,
  machineName,
  saidOfControl,
  settle,
  unreadable,
  type Refusals,
} from "./control-said";
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody, provablyNotSent } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { isIdentityRefusal } from "./resync";

// 〔C4e 批 3〕这一层与 `src/cc-bus-control.ts` 说的是同一件事的那几样（`ControlError` · 通道三层的说法 · 成品形状核验 ·
//   `settle`）搬进了 `src/control-said.ts`；本文件的调用方照旧从这里取那两样。
export { ControlError, saidOfControl };

/** 抓一屏的期限（见头注）。 */
const CAPTURE_BUDGET_MS = 20_000;
/** 结束 / 发按键 / 就地恢复的期限（见头注）。 */
const CONTROL_BUDGET_MS = 10_000;

// ─── 抓一屏 ───

/** 抓屏的拒绝码 ⇒ 一句话。五个码逐一分开（它们的下一步各不相同）；认不出的码原样带出去。 */
function captureRefusals(target: string): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "no_tmux":
          return copyText("tmuxControl.capture.noTmux", { target, detail });
        case "no_server":
          return copyText("tmuxControl.capture.noServer", { target, detail });
        case "no_such_session":
          return copyText("tmuxControl.capture.noSuchSession", { target, detail });
        case "invalid_args":
          return copyText("tmuxControl.capture.badName", { target, detail });
        case "capture_failed":
          return copyText("tmuxControl.capture.failed", { target, detail });
        default:
          return copyText("tmuxControl.capture.otherCode", { target, code, detail });
      }
    },
    noReason: () => copyText("tmuxControl.capture.noReason", { target }),
  };
}

/** `capture-pane` 的成品 ⇒ 那一屏。形状不对 ⇒ 抛。**空屏是合法的成功**（刚建起来、什么都没打印的 pane）。 */
export function decodeCapture(origin: Origin, v: unknown): string {
  if (!isObj(v) || !exactKeys(v, ["name", "screen"]) || typeof v.name !== "string" || typeof v.screen !== "string") {
    throw unreadable(origin, "capture-pane", "is not exactly {name, screen} (two strings)");
  }
  return v.screen;
}

/**
 * 抓 `origin` 上 tmux 会话 `target` 当前的一屏（**只读快照，不 attach**；刻意不过身份门，同后端那一侧）。
 * 失败 ⇒ 抛 [`ControlError`]（`message` 是给人看的那一句）。
 */
export async function capturePane(origin: Origin, target: string): Promise<string> {
  const payload = jsonBody({ name: target });
  const budget = budgetWithin(CAPTURE_BUDGET_MS);
  const v = await settle(origin, "capture-pane", chan.call(origin, "capture-pane", payload, budget), captureRefusals(target));
  return decodeCapture(origin, v);
}

// ─── 结束会话 ───

/** 结束会话的拒绝码 ⇒ 一句话。身份门 / 窗口门两档说清拦下的原因（它们的下一步与「会话不在」完全不同）。 */
function killRefusals(target: string): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "invalid_args":
          return copyText("tmuxControl.kill.badName", { target, detail });
        case "no_tmux":
          return copyText("tmuxControl.kill.noTmux", { target, detail });
        case "no_such_session":
          return copyText("tmuxControl.kill.noSuchSession", { target, detail });
        case "wrong_owner":
          return copyText("tmuxControl.kill.wrongOwner", { target, detail });
        case "too_many_windows":
          return copyText("tmuxControl.kill.tooManyWindows", { target, detail });
        case "kill_failed":
          return copyText("tmuxControl.kill.failed", { target, detail });
        default:
          return copyText("tmuxControl.kill.otherCode", { target, code, detail });
      }
    },
    noReason: () => copyText("tmuxControl.kill.noReason", { target }),
  };
}

/**
 * `kill` 的成品 ⇒ 做成了没有。恰好 `{session, killed}` 且 `killed === true` 才算结束了；
 * 形状对但 `killed` 不为真 ⇒ 「后端没确认结束」（**不当成功**：破坏性动作在未知状态上不许往下走）。
 */
export function decodeKilled(origin: Origin, target: string, v: unknown): void {
  if (!isObj(v) || !exactKeys(v, ["session", "killed"]) || typeof v.session !== "string" || typeof v.killed !== "boolean") {
    throw unreadable(origin, "kill", "is not exactly {session, killed}");
  }
  if (!v.killed) {
    throw new ControlError(
      copyText("tmuxControl.kill.notConfirmed", { machine: machineName(origin), target }),
      "kill reply has killed=false but no refusal code",
    );
  }
}

/**
 * 结束 `origin` 上的 tmux 会话 `target`（**破坏性**：调用方先二次确认）。身份门 · 窗口门在后端先过，
 * 后端对**句柄**下手（不是名字）。失败 ⇒ 抛 [`ControlError`]；**没有第二条路可回落**。
 */
export async function killSession(origin: Origin, target: string): Promise<void> {
  const payload = jsonBody({ name: target });
  const budget = budgetWithin(CONTROL_BUDGET_MS);
  const v = await settle(origin, "kill", chan.call(origin, "kill", payload, budget), killRefusals(target));
  decodeKilled(origin, target, v);
}

// ─── 发按键 · 就地恢复（都是 `launch` 那条帧命令） ───

/** `launch` 那条（发按键 / 就地恢复）的拒绝码 ⇒ 一句话。`wrong_owner` 来自后端的身份门（§34 Gate 2；〔TL2〕后端登记表与金样已补上它）。 */
function keysRefusals(target: string): Refusals {
  return {
    byCode(code, detail) {
      switch (code) {
        case "invalid_args":
          return copyText("tmuxControl.keys.badRequest", { target, detail });
        case "no_tmux":
          return copyText("tmuxControl.keys.noTmux", { target, detail });
        case "no_such_session":
          return copyText("tmuxControl.keys.noSuchSession", { target, detail });
        case "create_failed":
          return copyText("tmuxControl.keys.createFailed", { target, detail });
        case "typed_unconfirmed":
          return copyText("tmuxControl.keys.unconfirmed", { target, detail });
        case "wrong_owner":
          return copyText("tmuxControl.keys.wrongOwner", { target, detail });
        default:
          return copyText("tmuxControl.keys.otherCode", { target, code, detail });
      }
    },
    noReason: () => copyText("tmuxControl.keys.noReason", { target }),
  };
}

/**
 * `launch` 的成品 ⇒ 键进去了没有。恰好 `{session, created, typed}` 且 `typed === true` 才算送到；
 * `typed` 不为真 ⇒ 「后端没确认送到」（不当成功、不换条路重发）。
 * ⚠ `typed` 只有 `tmux send-keys` 的退出码那么强（pane 在 copy-mode 时照样退 0）—— 后端那一侧的判据钉着这句。
 */
export function decodeTyped(origin: Origin, target: string, v: unknown): void {
  if (
    !isObj(v) ||
    !exactKeys(v, ["session", "created", "typed"]) ||
    typeof v.session !== "string" ||
    typeof v.created !== "boolean" ||
    typeof v.typed !== "boolean"
  ) {
    throw unreadable(origin, "launch", "is not exactly {session, created, typed}");
  }
  if (!v.typed) {
    throw new ControlError(
      copyText("tmuxControl.keys.notConfirmed", { machine: machineName(origin), target }),
      "launch reply has typed=false but no refusal code",
    );
  }
}

/**
 * 往 `origin` 上已存在的 tmux 会话 `target` 里发按键。`enter` 真 ⇒ mode `send-into`（键入 ＋ 回车：`/compact` · `/exit`）；
 * 假 ⇒ mode `send-keys-raw`（裸键、不附回车：打断当前回合的 `Escape`）。**只发按键、不杀不建。**
 * 失败 ⇒ 抛 [`ControlError`]；**没有第二条路可回落**。
 */
export async function sendKeys(origin: Origin, target: string, keys: string, enter = true): Promise<void> {
  const mode = enter ? "send-into" : "send-keys-raw";
  const payload = jsonBody({ mode, name: target, payload: keys });
  const budget = budgetWithin(CONTROL_BUDGET_MS);
  const v = await settle(origin, "launch", chan.call(origin, "launch", payload, budget), keysRefusals(target));
  decodeTyped(origin, target, v);
}

/** 就地恢复那一次键入的结局（F14，见头注第 4 条）。 */
export type SendIntoOutcome =
  | { verdict: "typed" }
  /** **能证明一个字节都没到对端** ⇒ 调用方可以回落到那条整串（重做不会重复执行）。 */
  | { verdict: "fallback"; reason: string }
  /** 对端说了话，**或者**拿不准它执行没有 ⇒ **不许回落**，把这句话交给用户。
   *  〔RESYNC · `99 §2.1` ㉒〕`gate2` = 是关卡 2（身份门）拒的 ⇒ 提示可以带「对齐后重试」（别的拒绝不带：拿不准执行没有时重试会重复键入）。 */
  | { verdict: "refused"; reason: string; gate2?: true };

/**
 * 就地恢复：往已存在的空 tmux 会话 `name` 里键入载荷 `payload`（`launch{mode:"send-into"}`）。**不抛。**
 * 会话名或载荷为空 ⇒ `refused`（坏数据不是缺省，也不许拿去渲染整串）。
 */
export async function sendInto(origin: Origin, name: string, payload: string): Promise<SendIntoOutcome> {
  // 〔FIX · `设计/99 §2 ㊹`〕空名 / 空载荷不在这里判：交给那台后端（`launch` 进门拒 ⇒ refused，带它的原话）；
  //   回落那一跳渲整串的是 Rust 渲染器（`payload.rs::render_tmux_outer`，目标过 `gate-core` 已有会话那一条）⇒ 坏数据两条路都被同一处拒。
  try {
    const body = jsonBody({ mode: "send-into", name, payload });
    const budget = budgetWithin(CONTROL_BUDGET_MS);
    const v = await settle(origin, "launch", chan.call(origin, "launch", body, budget), keysRefusals(name));
    decodeTyped(origin, name, v);
    return { verdict: "typed" };
  } catch (e) {
    const reason = saidOfControl(e);
    if (e instanceof ControlError && e.error !== undefined && provablyNotSent(e.error)) {
      return { verdict: "fallback", reason };
    }
    return isIdentityRefusal(e) ? { verdict: "refused", reason, gate2: true } : { verdict: "refused", reason };
  }
}
