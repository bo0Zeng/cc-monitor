/**
 * G6（branch-anywhere）：分叉起会话前的**一次性追问小窗**。
 *
 * 只在那台推出的三格（`session-fork` 回复的 `launch`）里有不知道的格时出现 —— 主要是**源会话已经退出、真的查不出来**时。
 * 什么时候问那条判断在 `fork-start.ts`，本模块不重复它；为什么不知道由那台给码，这里照码说。
 *
 * # 这个窗口存在的唯一理由
 *
 * 「已退出的会话，账号还原不出」是**实测结论**（G3a §1：pidfile 随进程消失、
 * 各账号 `projects/` 是同一个 inode 的软链、jsonl 44 个顶层键里没有账号字段）。
 * 既然还原不出，就只有两条路：**猜一个**，或者**问一次**。
 * 猜的代价是「静默用错身份跑一条对话」，界面上看不出任何异样 —— 所以问。
 *
 * # 为什么账号默认落在「账号 0」而不是「当前账号」
 *
 * 因为默认值会被大量用户直接回车确认。把「当前账号」摆在默认位，等于把
 * 「不拿当前账号顶替」那条防线绕过去了 —— 只不过多了一次点击。
 * 账号 0 = 不注入 `CLAUDE_CONFIG_DIR`，是**保守**的那个默认。
 */

import type { ForkAskSlot, ForkChoices } from "./fork-start";
import type { ForkLaunch, ForkSlot } from "./session-writes";
import { copyText } from "./copy-table";

export interface ForkAskOptions {
  launch: ForkLaunch;
  slots: readonly ForkAskSlot[];
  /** 可选的号（名字；不含账号 0，账号 0 由本模块固定摆在首位）。 */
  accounts: readonly string[];
  /** tmux 复选框的初始态。远端默认勾上（远端会话惯例住在 tmux 里），本机传 false。 */
  defaultUseTmux: boolean;
  /** 挂载点，默认 `document.body`（测试可注入）。 */
  host?: HTMLElement;
}

/** 账号 0 在 `<select>` 里的 value（空串会被 falsy 判断吃掉，故用一个显式哨兵）。 */
export const ACCOUNT_ZERO_VALUE = "__account_zero__";

/** 号那一格为什么要问（那台给的码 ⇒ 这一句）。 */
function accountWhy(s: ForkSlot<string | null>): string {
  if (s.kind === "known") return "";
  return s.why === "live_no_account" ? copyText("forkAsk.why.liveNoAccount") : copyText("forkAsk.why.exited");
}

/**
 * 弹一次追问小窗。resolve 到用户的选择；**取消（按钮 / Esc / 点背景）resolve 到 `null`**。
 *
 * `null` 与「答了但没改」是两件不同的事，调用方据此决定起不起会话 —— 所以取消绝不
 * 退化成一个空的 `{}`（那会被当成「用户确认了默认值」照常起）。
 */
/**
 * 上一个还没结算的小窗。**Phase G 审计抓出的一条**：原来只是
 * `host.querySelector(".fork-ask-backdrop")?.remove()` 把前一个 backdrop 摘出 DOM ——
 * 它的 Promise 与那条 capture 阶段的 `keydown` 监听都还活着。后果有两层：
 * ① 第一条 `runForkFlow` **永挂**（无 toast、无取消，那次分叉悄无声息地没了下文）；
 * ② 之后用户随便按一次 Esc，孤儿监听先在捕获阶段 `stopPropagation()` **吞掉这次全局 Esc**，
 *    再把第一条静默取消。
 *
 * 触发不需要多罕见：`branch-button.ts` 的 busy 标志在 `onForked` 这个 fire-and-forget
 * 调用之后**立刻**复位，所以连点两下 `⑂`、或在两张卡上各点一次就够了。
 *
 * ⇒ 新窗开之前，**把旧窗按取消结算掉**（而不是只摘 DOM）。
 */
let pendingCancel: (() => void) | null = null;

export function askForkLaunch(opts: ForkAskOptions): Promise<ForkChoices | null> {
  const host = opts.host ?? document.body;
  pendingCancel?.();
  host.querySelector(".fork-ask-backdrop")?.remove(); // 兜底：万一 DOM 被外部搬过

  const backdrop = document.createElement("div");
  backdrop.className = "fork-ask-backdrop";
  const modal = document.createElement("div");
  modal.className = "fork-ask";
  backdrop.appendChild(modal);

  const title = document.createElement("div");
  title.className = "fork-ask-title";
  title.textContent = copyText("forkAsk.askForkLaunch.title");
  modal.appendChild(title);

  const lead = document.createElement("div");
  lead.className = "fork-ask-lead";
  lead.textContent =
    copyText("forkAsk.askForkLaunch.intro");
  modal.appendChild(lead);

  const askAccount = opts.slots.includes("account");
  const askTmux = opts.slots.includes("terminal");
  const askCwd = opts.slots.includes("cwd");

  let accountSel: HTMLSelectElement | null = null;
  if (askAccount) {
    const row = document.createElement("label");
    row.className = "fork-ask-row";
    const label = document.createElement("span");
    label.className = "fork-ask-label";
    label.textContent = copyText("forkAsk.field.account");
    accountSel = document.createElement("select");
    accountSel.className = "fork-ask-account";
    const zero = document.createElement("option");
    zero.value = ACCOUNT_ZERO_VALUE;
    zero.textContent = copyText("forkAsk.account.base");
    accountSel.appendChild(zero);
    for (const name of opts.accounts) {
      const o = document.createElement("option");
      o.value = name;
      o.textContent = name;
      accountSel.appendChild(o);
    }
    row.append(label, accountSel);
    modal.appendChild(row);

    const why = document.createElement("div");
    why.className = "fork-ask-why";
    why.textContent = accountWhy(opts.launch.account);
    modal.appendChild(why);
  }

  let tmuxBox: HTMLInputElement | null = null;
  if (askTmux) {
    const row = document.createElement("label");
    row.className = "fork-ask-row";
    tmuxBox = document.createElement("input");
    tmuxBox.type = "checkbox";
    tmuxBox.className = "fork-ask-tmux";
    tmuxBox.checked = opts.defaultUseTmux;
    const label = document.createElement("span");
    label.className = "fork-ask-label";
    label.textContent = copyText("forkAsk.field.tmux");
    row.append(tmuxBox, label);
    modal.appendChild(row);
  }

  let cwdInput: HTMLInputElement | null = null;
  if (askCwd) {
    const row = document.createElement("label");
    row.className = "fork-ask-row";
    const label = document.createElement("span");
    label.className = "fork-ask-label";
    label.textContent = copyText("forkAsk.field.cwd");
    cwdInput = document.createElement("input");
    cwdInput.type = "text";
    cwdInput.className = "fork-ask-cwd";
    row.append(label, cwdInput);
    modal.appendChild(row);
  }

  const buttons = document.createElement("div");
  buttons.className = "fork-ask-buttons";
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "fork-ask-cancel";
  cancel.textContent = copyText("forkAsk.askForkLaunch.cancel");
  const ok = document.createElement("button");
  ok.type = "button";
  ok.className = "fork-ask-ok";
  ok.textContent = copyText("forkAsk.askForkLaunch.launch");
  buttons.append(cancel, ok);
  modal.appendChild(buttons);

  return new Promise<ForkChoices | null>((resolve) => {
    let settled = false;
    const finish = (v: ForkChoices | null): void => {
      if (settled) return; // Esc + 点按钮可能同一轮到达；只认第一次
      settled = true;
      if (pendingCancel === cancelSelf) pendingCancel = null;
      document.removeEventListener("keydown", onKey, true);
      backdrop.remove();
      resolve(v);
    };
    const cancelSelf = (): void => finish(null);
    pendingCancel = cancelSelf;
    const onKey = (ev: KeyboardEvent): void => {
      if (ev.key === "Escape") {
        ev.stopPropagation();
        finish(null);
      }
    };
    document.addEventListener("keydown", onKey, true);
    backdrop.addEventListener("click", (ev) => {
      if (ev.target === backdrop) finish(null); // 点背景 = 取消
    });
    cancel.addEventListener("click", () => finish(null));
    ok.addEventListener("click", () => {
      const choices: ForkChoices = {};
      if (accountSel) {
        // 交的是 `<option>` 的 value（号名本身），不是显示文本。
        choices.account = accountSel.value === ACCOUNT_ZERO_VALUE ? null : accountSel.value;
      }
      if (tmuxBox) choices.useTmux = tmuxBox.checked;
      if (cwdInput && cwdInput.value.trim()) choices.cwd = cwdInput.value.trim();
      finish(choices);
    });

    host.appendChild(backdrop);
    ok.focus();
  });
}
