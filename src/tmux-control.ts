/**
 * 〔C4e · 第四波 4C · `设计/05 §8` 步 5〕**界面直接说的 tmux 控制类帧命令** —— 抓一屏（`capture-pane`）。
 *
 * # 它顶掉了什么
 *
 * 此前界面经 monitor 的 Tauri 命令 `capture_remote_pane`〔散文墓碑〕抓屏：monitor 先拒空目标、预问那台后端认不认
 * `capture-pane`、转过去、取 `screen`、把五个拒绝码与「通道不在」说成人话 —— 那一份解释住 monitor 中层
 * （`backend/control/tmux.rs`）。后端的帧应答本来就是成品（`{name, screen}`），monitor 那一层只在转 ＋ 说话
 * ⇒ 按 `设计/05 §14.3` 的判准（业务解释只有一个家）迁：界面经 `chan.call` 直接问那台机器的后端，
 * 解释只剩本文件这一份，monitor 那条命令与那一份解释一起删了。**本机与远端同一条路**（本机由 `<local>` 那条长连接答）。
 *
 * # 本文件做的只有三件（都是调用方那一侧的事）
 *
 * 1. **空目标先拒**（`§34` Gate 1 的本地那一格）：`=:` 会被 tmux 读成「当前会话」，空目标不该先花一次往返。
 *    一个字节都不发（Gate 2 / 3 在后端 `control/gate.rs`，本文件不写第二份）。
 * 2. **按形状收**：成品恰好是那几格、类型对 ⇒ 收；多一格 / 缺一格 / 类型不对 ⇒ 当成「两边版本对不上」抛，不猜。
 *    线上形状由跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 钉着（后端产出 == 金样 · 本文件读同一份）。
 * 3. **失败怎么说**（`§3.3.2`「说法归调用方」）：拒绝码 → 一句话（逐码分开，认不出的码原样带出去，不猜）；
 *    通道的三层错误 → 一句话（本机与远端的下一步不同，话就不一样）。句子住文案表 `tmuxControl.*`。
 *
 * # 期限
 *
 * 抓一屏 20 秒（`X6`：调用点显式给）—— 与它上一个住址（monitor `capture_via_backend`）同值。
 */
import { copyText } from "./copy-table";
import { chan, ChanError, type CallError } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, refusalOf } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";

/** 抓一屏的期限（见头注）。 */
const CAPTURE_BUDGET_MS = 20_000;

/**
 * 一次控制动作没做成。`message` 就是给人看的那一句（已经说成人话）；`detail` 只进日志。
 * 调用方拿 [`saidOfControl`] 取那一句，不自己拼。
 */
export class ControlError extends Error {
  readonly detail: string;
  constructor(said: string, detail: string) {
    super(said);
    this.name = "ControlError";
    this.detail = detail;
  }
}

/** 一次控制动作失败 ⇒ 给人看的那一句。本文件抛的是 [`ControlError`]；别的（调用方自己的错）原样取 `message`。 */
export function saidOfControl(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

const isObj = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === "object" && !Array.isArray(v);

/** 成品的键集合恰好是 `keys`（多一格 / 缺一格都不收）。 */
function exactKeys(v: Record<string, unknown>, keys: readonly string[]): boolean {
  const got = Object.keys(v).sort();
  const want = [...keys].sort();
  return got.length === want.length && got.every((k, i) => k === want[i]);
}

function machineName(origin: Origin): string {
  return isLocalOrigin(origin) ? copyText("tmuxControl.machine.local") : origin;
}

/** 空目标先拒（Gate 1 的本地那一格）。 */
function rejectEmptyTarget(target: string): void {
  if (target === "") throw new ControlError(copyText("tmuxControl.target.empty"), "空的 tmux 目标，一个字节都没发");
}

/** 应答形状不对 ⇒ 抛（哪一格不对只进 `detail`）。 */
function unreadable(origin: Origin, op: string, what: string): ControlError {
  return new ControlError(copyText("tmuxControl.reply.unreadable", { machine: machineName(origin) }), `${op} 的应答${what}`);
}

/**
 * 通道三层里「对端说不行」之外的那几层 ⇒ 一句话（三个动作共用：它们说的都是「这台机器够不够得着、答没答」）。
 * `peer/refused` 不在这里 —— 那一层的话按动作分，调用方传进来。
 */
function saidOfTransport(origin: Origin, err: CallError): string {
  switch (err.layer) {
    case "hop":
      if (err.reach === "NotSent") {
        return isLocalOrigin(origin)
          ? copyText("tmuxControl.channel.localDown")
          : copyText("tmuxControl.channel.remoteDown", { machine: origin });
      }
      return copyText("tmuxControl.channel.unsure", { machine: machineName(origin) });
    case "peer":
      // `unsupported`：那台后端事前就说不认这条命令（比这条动作老）。`refused` 由调用方先接走，走不到这里。
      return copyText("tmuxControl.channel.oldBackend", { machine: machineName(origin) });
    case "ours":
      return err.why === "Cancelled" ? copyText("tmuxControl.channel.cancelled") : copyText("tmuxControl.channel.broken");
  }
}

/** 抓屏的拒绝码 ⇒ 一句话。五个码逐一分开（它们的下一步各不相同）；认不出的码原样带出去。 */
function captureRefusal(target: string, code: string, detail: string): string {
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
}

/** `capture-pane` 的成品 ⇒ 那一屏。形状不对 ⇒ 抛。**空屏是合法的成功**（刚建起来、什么都没打印的 pane）。 */
export function decodeCapture(origin: Origin, v: unknown): string {
  if (!isObj(v) || !exactKeys(v, ["name", "screen"]) || typeof v.name !== "string" || typeof v.screen !== "string") {
    throw unreadable(origin, "capture-pane", "不是恰好 `{name, screen}` 两格字符串");
  }
  return v.screen;
}

/**
 * 抓 `origin` 上 tmux 会话 `target` 当前的一屏（**只读快照，不 attach**）。
 * 失败 ⇒ 抛 [`ControlError`]（`message` 是给人看的那一句）。
 */
export async function capturePane(origin: Origin, target: string): Promise<string> {
  rejectEmptyTarget(target);
  const payload = jsonBody({ name: target });
  const budget = budgetWithin(CAPTURE_BUDGET_MS);
  let body: Uint8Array;
  try {
    body = await chan.call(origin, "capture-pane", payload, budget);
  } catch (e) {
    if (!(e instanceof ChanError)) throw e;
    const err = e.error;
    if (err.layer === "peer" && err.why === "refused") {
      const r = refusalOf(err.body);
      throw new ControlError(
        r ? captureRefusal(target, r.code, r.message) : copyText("tmuxControl.capture.noReason", { target }),
        `capture-pane 被拒：${r ? r.code : "（拒绝体读不出来）"}`,
      );
    }
    throw new ControlError(saidOfTransport(origin, err), e.message);
  }
  let v: unknown;
  try {
    v = readJson(body);
  } catch {
    throw unreadable(origin, "capture-pane", "不是 JSON");
  }
  return decodeCapture(origin, v);
}
