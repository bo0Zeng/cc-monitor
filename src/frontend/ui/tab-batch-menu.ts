/**
 * **多选之后右键的那个菜单**（批量）：每一项写「动作（能做的个数）」。
 *
 * 每一项在一个 tab 上的语义就是单个菜单里那一项：杀死会话 · Resume（tmux 后台 / 各开一个终端）· 固定 / 取消固定 ·
 * 加入集合 / 新建集合 / 移出集合 · 隐藏 / 取消隐藏 · 关闭（同 ×）。「能做的个数」按单个菜单亮那一项用的同一个谓词数
 * （结束 ＝ `can.end`，确认框同单个那一个，`end-confirm.ts`；Resume ＝ `can.resume`；关闭 ＝ `can.close` 为 quiet；灰着的理由照抄那一格的 `why`）：
 * 它是确认框列的那一份清单的长度。结束 · 恢复 · 固定交本机后端一条（`sessions-batch`，选中的全部；跳不跳过 · 那一句 · 逐条都是后端的），结局照回包摆；
 * 固定是会话那一台的注解，回包里成了的那几个才翻。分组 · 隐藏 · 关闭做完的提示是那一口给的（关闭当场从栏上消失），不另出结果条。
 */
import type { Tab } from "./tab-model";
import { canEnd, canResume, closesQuietly } from "./face-can";
import { copyText } from "./copy-table";
import { openMenu, type MenuItem } from "./kit/menu";
import type { TabGroup } from "./tab-bar-prefs";
import { confirmDialog, type ConfirmFn } from "./kit/dialog";
import { groupSubmenu } from "./tab-menu";
import { askEnd, sayReply } from "./end-confirm";
import { machineName } from "./control-said";
import { fullTitle } from "./session-face";
import { isRemoteOrigin } from "./ipc/origin";
import { askSessions, itemOf, startMany, stopMany } from "./tab-batch-run";
import type { SessionsBatchReply } from "./generated/SessionsBatchReply";
import { failToast } from "./kit/toast";
import type { Launch } from "./generated/Launch";

/** 批量菜单要宿主给的：按 sid 取 tab · 集合 · 固定 · 关闭（都一次改完、落盘一次）。 */
export interface TabBatchHost {
  tab(sid: string): Tab | undefined;
  collectionsLoaded(): boolean;
  collections(): TabGroup[];
  pinnedLoaded(): boolean;
  /** 批量固定 · 取消固定的回包里成了的那几个：翻到 `on`、重画、账重读。 */
  landPinned(sids: readonly string[], on: boolean): void;
  /** 分组那三件（与单个右键 · 拖放同一口：做完重画、给可撤的提示；新建 ＝「分组 N」＋ 名字框立刻打开）。 */
  joinGroup(sids: readonly string[], gid: string): void;
  foundGroup(sids: readonly string[]): void;
  leaveGroup(sids: readonly string[]): void;
  closeTabs(sids: readonly string[]): void;
  /** 隐藏 · 取消隐藏（同单个右键那一项：交命令、撤销 toast；挪不挪等推帧）。 */
  hide(sids: readonly string[]): void;
  unhide(sids: readonly string[]): void;
}

/** 后端那几件（判据换替身）。 */
export interface TabBatchRun {
  stop(tabs: readonly Tab[]): Promise<SessionsBatchReply>;
  start(tabs: readonly Tab[], run: Launch): Promise<SessionsBatchReply>;
  pin(tabs: readonly Tab[], on: boolean): Promise<SessionsBatchReply>;
  confirm: ConfirmFn;
}

export const PRODUCTION_RUN: TabBatchRun = {
  stop: stopMany,
  start: startMany,
  pin: (tabs, on) => askSessions(on ? "pin" : "unpin", tabs.map(itemOf)),
  confirm: confirmDialog,
};

/** 不能做的那一个（灰着时第二行照第一个的理由）。 */
interface Skip {
  sid: string;
  why: string;
}

/** 把一批 tab 按一个谓词分成「能做的」与「不能做的（原因：一句，或按 tab 说）」。 */
function split(tabs: readonly Tab[], ok: (t: Tab) => boolean, why: string | ((t: Tab) => string)): [Tab[], Skip[]] {
  const yes: Tab[] = [];
  const no: Skip[] = [];
  for (const t of tabs) {
    if (ok(t)) yes.push(t);
    else no.push({ sid: t.sessionId, why: typeof why === "string" ? why : why(t) });
  }
  return [yes, no];
}

/** 那一样不给做时那台写好的那一句（`can.*.why`）。 */
const whyOf = (c: { can: string; why?: string } | undefined): string => c?.why ?? "";

/** 一行里怎么称呼一个 tab：远端的机器写在括号里（那一行的原因由那台写，不带机器）。 */
function lineTitle(t: Tab): string {
  return isRemoteOrigin(t.origin) ? copyText("tabBatch.line.remote", { title: fullTitle(t), machine: machineName(t.origin) }) : fullTitle(t);
}

/** 右键一个选中的 tab（多选 ≥ 2；键盘菜单键 ⇒ 锚在焦点那一行）⇒ 开批量菜单。`sids` 按条上的顺序。`done` = 做完一项之后（宿主清多选）。 */
export function openBatchMenu(
  e: MouseEvent | HTMLElement,
  sids: readonly string[],
  host: TabBatchHost,
  run: TabBatchRun = PRODUCTION_RUN,
  done: () => void = () => {},
): void {
  const tabs = sids.map((s) => host.tab(s)).filter((t): t is Tab => t !== undefined);
  const item = (label: string, n: number, onClick: () => void | Promise<void>, danger = false, why?: string): MenuItem => ({
    label,
    danger,
    enabled: n > 0,
    why: n > 0 ? undefined : why,
    onClick: () => {
      void (async () => {
        await onClick();
        done();
      })();
    },
  });
  const [stoppable, notStoppable] = split(tabs, (t) => canEnd(t.face), (t) => whyOf(t.face?.can.end));
  const [startable, notStartable] = split(tabs, (t) => canResume(t.face), (t) => whyOf(t.face?.can.resume));
  const start = async (how: Launch, title: string): Promise<void> => {
    try {
      sayReply(await run.start(tabs, how));
    } catch (e) {
      failToast(title, e);
    }
  };
  // 固定 · 取消固定：交出去的是选中的全部（已固定的那台照注解判跳过）；回包里成了的那几个才翻。
  const pin = async (on: boolean, title: string): Promise<void> => {
    try {
      const reply = await run.pin(tabs, on);
      host.landPinned(reply.results.filter((o) => o.outcome === "done").map((o) => o.sid), on);
      sayReply(reply);
    } catch (e) {
      failToast(title, e);
    }
  };
  // 那一项的字也是失败那一条的标题。
  const startTmux = copyText("tabBatch.menu.startTmux", { n: startable.length });
  const startWindow = copyText("tabBatch.menu.startWindow", { n: startable.length });
  const items: MenuItem[] = [
    { label: copyText("tabBatch.menu.head", { n: tabs.length }), heading: true },
    // 结束：确认框只列结束得了的（同后端判跳过的那一格）；交出去的是选中的全部，跳过的由回包逐条列。
    item(copyText("tabBatch.menu.stop", { n: stoppable.length }), stoppable.length, async () => {
      if (!(await askEnd(stoppable.map(lineTitle), run.confirm))) return;
      try {
        sayReply(await run.stop(tabs));
      } catch (e) {
        failToast(copyText("tabBatch.stop.title", { n: tabs.length }), e);
      }
    }, true, notStoppable[0]?.why),
    // 恢复：交出去的是选中的全部，跳过的（恢复不了 · 本来就在跑）由回包逐条列。
    item(startTmux, startable.length, () => start("tmux", startTmux), false, notStartable[0]?.why),
    item(startWindow, startable.length, () => start("direct", startWindow), false, notStartable[0]?.why),
  ];
  if (host.pinnedLoaded()) {
    const [pinnable, pinnedAlready] = split(tabs, (t) => !t.pinned, copyText("tabBatch.why.pinned"));
    const [unpinnable, notPinned] = split(tabs, (t) => t.pinned, copyText("tabBatch.why.notPinned"));
    const pinLabel = copyText("tabBatch.menu.pin", { n: pinnable.length });
    const unpinLabel = copyText("tabBatch.menu.unpin", { n: unpinnable.length });
    items.push(
      item(pinLabel, pinnable.length, () => pin(true, pinLabel), false, pinnedAlready[0]?.why),
      item(unpinLabel, unpinnable.length, () => pin(false, unpinLabel), false, notPinned[0]?.why),
    );
  }
  if (host.collectionsLoaded()) {
    // 「分组 ▸」：与单个右键同一份（每项带能做的个数）；做完的提示是那一口给的可撤 toast，不另出结果条。
    const after = (fn: () => void) => () => {
      fn();
      done();
    };
    items.push({
      label: copyText("tabMenu.collection.add"),
      submenu: groupSubmenu({
        collections: host.collections(),
        movable: (gid) => tabs.filter((t) => t.group !== gid).length,
        grouped: tabs.filter((t) => t.group !== null).length,
        many: tabs.length,
        found: after(() => host.foundGroup(tabs.map((t) => t.sessionId))),
        join: (gid) => after(() => host.joinGroup(tabs.filter((t) => t.group !== gid).map((t) => t.sessionId), gid))(),
        leave: after(() => host.leaveGroup(tabs.filter((t) => t.group !== null).map((t) => t.sessionId))),
        why: { inThatGroup: copyText("tabBatch.why.inThatGroup"), notInGroup: copyText("tabBatch.why.notInGroup") },
      }),
    });
  }
  // 隐藏 · 取消隐藏：做完的提示是那一口给的撤销 toast，不另出结果条。
  const [hideable] = split(tabs, (t) => (t.hidden ?? null) === null, "");
  const [unhideable] = split(tabs, (t) => (t.hidden ?? null) !== null, "");
  items.push(
    item(copyText("tabBatch.menu.hide", { n: hideable.length }), hideable.length, () => host.hide(hideable.map((t) => t.sessionId)), false, copyText("tabBatch.why.hidden")),
    item(copyText("tabBatch.menu.unhide", { n: unhideable.length }), unhideable.length, () => host.unhide(unhideable.map((t) => t.sessionId)), false, copyText("tabBatch.why.notHidden")),
  );
  const [closable, notClosable] = split(tabs, (t) => closesQuietly(t.face), (t) => {
    const c = t.face?.can.close;
    return c?.can === "no" ? c.why : (t.face?.tip ?? "");
  });
  items.push(
    // 关闭：关掉的当场从栏上消失、留下的看得见，不另出结果条。
    item(copyText("tabBatch.menu.close", { n: closable.length }), closable.length, () => host.closeTabs(closable.map((t) => t.sessionId)), false, notClosable[0]?.why),
  );
  openMenu(e instanceof HTMLElement ? { el: e, align: "end" } : { x: e.clientX, y: e.clientY }, items);
}
