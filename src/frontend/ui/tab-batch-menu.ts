/**
 * **多选之后右键的那个菜单**（批量）：每一项写「动作（能做的个数）」，能做的照做、不能做的跳过，做完一条结果提示
 * （做了几个 · 跳过几个、各为什么 · 失败几个、各为什么）。
 *
 * 每一项在一个 tab 上的语义就是单个菜单里那一项：杀死会话 · Resume（tmux 后台 / 各开一个终端）· 固定 / 取消固定 ·
 * 加入集合 / 新建集合 / 移出集合 · 关闭（同 ×）。「能做的个数」按单个菜单亮那一项用的同一个谓词数
 * （杀 = 有终端可去 `hasTerminal`；Resume / 关闭 = 已结束 `isResumeOnly`）；那台能不能做、怎么做由那台后端判（`tab-batch-run.ts`）。
 * 固定 · 集合 · 关闭是 monitor 自己的界面状态，不经后端。
 */
import type { Tab } from "./tab-model";
import { canResume, hasTerminal, isResumeOnly } from "./tab-session-state";
import { copyText } from "./copy-table";
import { openMenu, type MenuItem } from "./kit/menu";
import { createRefusal, newCollectionId, type CollectionRefusal, type TabCollection } from "./tab-collections";
import { sayCollectionRefusal } from "./tab-bar-prefs";
import { confirmDialog, askText, type ConfirmFn } from "./kit/dialog";
import { recentToasts, toast, type ToastRecord } from "./kit/toast";
import { showMessage } from "./status-messages";
import { machineName } from "./control-said";
import { fullTitle } from "./session-face";
import { isRemoteOrigin } from "./ipc/origin";
import { startMany, stopMany, type BatchOutcome } from "./tab-batch-run";
import { writeSessionRotation, type RulesRead, type SessionRotationWrite } from "./quota-reads";
import { reasonLabel } from "./acct-view";
import type { SwitchOutcome } from "./generated/SwitchOutcome";
import type { Origin } from "./ipc/origin";

/** 批量菜单要宿主给的：按 sid 取 tab · 集合 · 固定 · 关闭（都一次改完、落盘一次）。 */
export interface TabBatchHost {
  tab(sid: string): Tab | undefined;
  collectionsLoaded(): boolean;
  collections(): TabCollection[];
  pinnedLoaded(): boolean;
  setPinned(sids: readonly string[], on: boolean): void;
  joinGroup(sids: readonly string[], gid: string): void;
  foundGroup(sids: readonly string[], name: string, id: string): CollectionRefusal | null;
  leaveGroup(sids: readonly string[]): void;
  closeTabs(sids: readonly string[]): void;
  /** 那台的规则表（还没读到 ⇒ `null`，菜单里那一项转圈）；不给 / 回 `undefined`（宿主没接）⇒ 不出「轮换规则」。 */
  rulesOf?(origin: Origin): RulesRead | null | undefined;
  /** 「管理规则…」：设置窗那台的「轮换」栏。 */
  openRules?(origin: Origin): void;
}

/** 后端那两件（判据换替身）。 */
export interface TabBatchRun {
  stop(tabs: readonly Tab[]): Promise<BatchOutcome[]>;
  start(tabs: readonly Tab[], mode: "tmux" | "window"): Promise<BatchOutcome[]>;
  confirm: ConfirmFn;
  /** 一批同机会话的轮换来源一次写（`rotation-session-set`），回逐会话的结局。 */
  rotate?(origin: Origin, sids: string[], to: SessionRotationWrite): Promise<Record<string, SwitchOutcome>>;
}

export const PRODUCTION_RUN: TabBatchRun = { stop: stopMany, start: startMany, confirm: confirmDialog, rotate: writeSessionRotation };

/** 把一批 tab 按一个谓词分成「能做的」与「跳过的（原因：一句，或按 tab 说）」。 */
function split(tabs: readonly Tab[], ok: (t: Tab) => boolean, why: string | ((t: Tab) => string)): [Tab[], BatchOutcome[]] {
  const yes: Tab[] = [];
  const no: BatchOutcome[] = [];
  for (const t of tabs) {
    if (ok(t)) yes.push(t);
    else no.push({ sid: t.sessionId, outcome: "skipped", why: typeof why === "string" ? why : why(t) });
  }
  return [yes, no];
}

/** 说不清（那台暂时看不见）的那几个说「看不见」，不说「已经结束了 / 还在运行」。 */
function unseenOr(why: string): (t: Tab) => string {
  return (t) => (t.state.liveness === "unseen" ? copyText("sessionState.unseen.tooltip") : why);
}

/** 结果提示：一句汇总（做了 · 失败几个）；逐个跳过与失败的（各为什么）进「消息」那一条，［查看］展开。 */
export function sayBatch(action: string | ((done: number, failed: number) => string), tabs: readonly Tab[], outcomes: readonly BatchOutcome[]): void {
  const title = (sid: string): string => {
    const t = tabs.find((x) => x.sessionId === sid);
    return t ? fullTitle(t) : sid;
  };
  const n = (k: BatchOutcome["outcome"]) => outcomes.filter((o) => o.outcome === k).length;
  // 一句结果（`已结束 11 · 失败 1`）；第二行起逐条写没做的为什么（出错的不自己走）。
  const head =
    typeof action === "function"
      ? action(n("done"), n("failed"))
      : n("failed") > 0
        ? copyText("tabBatch.result.someFailed", { action, done: n("done"), failed: n("failed") })
        : copyText("tabBatch.result.done", { action, done: n("done") });
  const lines: string[] = [];
  for (const o of outcomes) {
    if (o.outcome === "failed") lines.push(copyText("tabBatch.result.failedLine", { title: title(o.sid), why: o.why }));
  }
  for (const o of outcomes) {
    if (o.outcome === "skipped") lines.push(copyText("tabBatch.result.skippedLine", { title: title(o.sid), why: o.why }));
  }
  // toast 只说一句汇总；逐条原因进「消息」那一条，［查看］点了展开它。
  const made: { rec?: ToastRecord } = {};
  const view = lines.length > 0 ? { label: copyText("tabBatch.result.view"), run: () => made.rec && showMessage(made.rec), toastOnly: true } : undefined;
  // 复制详情：失败那几个各一段（那一句 ＋ 那台写的详情），段间空一行；成功的不进（条带 §5.6）。
  const detail = outcomes
    .filter((o) => o.outcome === "failed" && o.detail)
    .map((o) => `${o.why}\n${o.detail}`)
    .join("\n\n");
  toast(head, "", { level: n("failed") > 0 ? "error" : "success", more: lines, action: view, detail });
  made.rec = recentToasts()[0];
}

/** 只经 monitor 的那几项：同步改完 ⇒ 每一个都做成了，跳过的照原因列。 */
function local(action: string, tabs: readonly Tab[], doable: Tab[], skipped: BatchOutcome[], run: (sids: string[]) => void): void {
  if (doable.length > 0) run(doable.map((t) => t.sessionId));
  sayBatch(action, tabs, [...doable.map((t): BatchOutcome => ({ sid: t.sessionId, outcome: "done", why: "" })), ...skipped]);
}

/** 右键一个选中的 tab（多选 ≥ 2）⇒ 开批量菜单。`sids` 按条上的顺序。`done` = 做完一项之后（宿主清多选）。 */
export function openBatchMenu(
  e: MouseEvent,
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
  const [stoppable, notStoppable] = split(tabs, (t) => hasTerminal(t.state), unseenOr(copyText("tabBatch.why.ended", { ended: copyText("sessionState.ended.name") })));
  const [startable, notStartable] = split(tabs, (t) => canResume(t.state), unseenOr(copyText("tabBatch.why.live")));
  const items: MenuItem[] = [
    { label: copyText("tabBatch.menu.head", { n: tabs.length }), heading: true },
    item(copyText("tabBatch.menu.stop", { n: stoppable.length }), stoppable.length, async () => {
      const ended = notStoppable.filter((o) => o.why === copyText("tabBatch.why.ended", { ended: copyText("sessionState.ended.name") })).length;
      const confirm = {
        title: copyText("tabBatch.stop.title", { n: stoppable.length }),
        action: copyText("tabBatch.stop.action", { n: stoppable.length }),
        danger: true,
        rows: [
          { label: copyText("kit.interrupts.cut"), items: [copyText("tabBatch.stop.cuts")] },
          { label: copyText("kit.interrupts.keep"), items: [copyText("tabSessionActions.kill.keeps")] },
        ],
        // 机器只出一次：标题用不带 `[机器]` 前缀的那一形，远端的机器写在括号里。
        list: stoppable.map((t) => (isRemoteOrigin(t.origin) ? copyText("tabBatch.stop.line", { title: fullTitle(t), machine: machineName(t.origin) }) : fullTitle(t))),
        note: ended > 0 ? copyText("sessionState.batch.skipped", { n: ended }) : undefined,
      };
      if (!(await run.confirm(confirm))) return;
      const said = (done: number, failed: number): string =>
        failed > 0 ? copyText("sessionState.batch.someFailed", { done, failed }) : copyText("sessionState.batch.done", { done });
      sayBatch(said, tabs, [...(await run.stop(stoppable)), ...notStoppable]);
    }, true, notStoppable[0]?.why),
    item(copyText("tabBatch.menu.startTmux", { n: startable.length }), startable.length, async () => {
      sayBatch(copyText("tabBatch.action.startTmux"), tabs, [...(await run.start(startable, "tmux")), ...notStartable]);
    }, false, notStartable[0]?.why),
    item(copyText("tabBatch.menu.startWindow", { n: startable.length }), startable.length, async () => {
      sayBatch(copyText("tabBatch.action.startWindow"), tabs, [...(await run.start(startable, "window")), ...notStartable]);
    }, false, notStartable[0]?.why),
  ];
  const rot = rotationItem(tabs, host, run, done);
  if (rot) items.push({ label: "", divider: true }, rot);
  if (host.pinnedLoaded()) {
    const [pinnable, pinnedAlready] = split(tabs, (t) => !t.pinned, copyText("tabBatch.why.pinned"));
    const [unpinnable, notPinned] = split(tabs, (t) => t.pinned, copyText("tabBatch.why.notPinned"));
    items.push(
      item(copyText("tabBatch.menu.pin", { n: pinnable.length }), pinnable.length, () =>
        local(copyText("tabBatch.action.pin"), tabs, pinnable, pinnedAlready, (s) => host.setPinned(s, true)),
      false, pinnedAlready[0]?.why),
      item(copyText("tabBatch.menu.unpin", { n: unpinnable.length }), unpinnable.length, () =>
        local(copyText("tabBatch.action.unpin"), tabs, unpinnable, notPinned, (s) => host.setPinned(s, false)),
      false, notPinned[0]?.why),
    );
  }
  if (host.collectionsLoaded()) {
    const join: MenuItem[] = host.collections().map((col) => {
      const [movable, already] = split(tabs, (t) => t.group !== col.id, copyText("tabBatch.why.inThatGroup"));
      return item(copyText("tabBatch.menu.joinOne", { name: col.name, n: movable.length }), movable.length, () =>
        local(copyText("tabBatch.action.join", { name: col.name }), tabs, movable, already, (s) => host.joinGroup(s, col.id)),
      false, already[0]?.why);
    });
    join.push(
      item(copyText("tabBatch.menu.found", { n: tabs.length }), tabs.length, async () => {
        // 同单个菜单「新建集合…」：到上界先说，再问名字；整批进同一个新组。
        const full = createRefusal(host.collections());
        if (full) return sayCollectionRefusal(full);
        const name = await askText({ title: copyText("tabMenu.collection.title"), label: copyText("tabMenu.collection.namePrompt"), action: copyText("tabMenu.collection.action") });
        if (!name?.trim()) return;
        const why = host.foundGroup(tabs.map((t) => t.sessionId), name, newCollectionId());
        if (why) return sayCollectionRefusal(why);
        sayBatch(copyText("tabBatch.action.found", { name: name.trim() }), tabs, tabs.map((t) => ({ sid: t.sessionId, outcome: "done", why: "" })));
      }),
    );
    items.push({ label: copyText("tabMenu.collection.add"), submenu: join });
    const [grouped, loose] = split(tabs, (t) => t.group !== null, copyText("tabBatch.why.notInGroup"));
    items.push(
      item(copyText("tabBatch.menu.leave", { n: grouped.length }), grouped.length, () =>
        local(copyText("tabBatch.action.leave"), tabs, grouped, loose, (s) => host.leaveGroup(s)),
      false, loose[0]?.why),
    );
  }
  const [closable, notClosable] = split(tabs, (t) => isResumeOnly(t.state), copyText("tabBatch.why.live"));
  items.push(
    item(copyText("tabBatch.menu.close", { n: closable.length }), closable.length, () =>
      local(copyText("tabBatch.action.close"), tabs, closable, notClosable, (s) => host.closeTabs(s)),
    false, notClosable[0]?.why),
  );
  openMenu({ x: e.clientX, y: e.clientY }, items);
}

/**
 * 「轮换规则 ▸」（稿 §5.4）：子菜单 跟随默认（默认那条的名字）· 各条规则 · 管理规则…（没有「本会话」：批量写本会话没有意义）。
 * 账号按机器分开 ⇒ 只许同机批量，跨机灰着写为什么。选一项 ⇒ 这一批一次交那台，逐会话结局照那台说的进结果提示。
 */
function rotationItem(tabs: readonly Tab[], host: TabBatchHost, run: TabBatchRun, done: () => void): MenuItem | null {
  if (!host.rulesOf || !run.rotate || tabs.length === 0) return null;
  const origin = tabs[0].origin;
  // 个数 ＝ 同机可套用的那几个（跨机 ⇒ 0、灰着说为什么）。
  if (tabs.some((t) => t.origin !== origin))
    return { label: copyText("tabBatch.menu.rot", { n: 0 }), enabled: false, why: copyText("tabBatch.why.crossMachine") };
  const label = copyText("tabBatch.menu.rot", { n: tabs.length });
  const rules = host.rulesOf(origin);
  if (rules === undefined) return null;
  if (rules === null) return { label, pending: true };
  const def = rules.rules.find((r) => r.id === rules.defaultRule);
  const apply = (to: SessionRotationWrite, src: string): void => {
    void (async () => {
      const sids = tabs.map((t) => t.sessionId);
      let got: Record<string, SwitchOutcome> = {};
      let failed: string | null = null;
      try {
        got = await run.rotate!(origin, sids, to);
      } catch (e) {
        failed = e instanceof Error ? e.message : String(e);
      }
      const outcomes = tabs.map((t): BatchOutcome => {
        const o = got[t.sessionId];
        if (failed !== null || !o) return { sid: t.sessionId, outcome: "failed", why: failed ?? copyText("acct.reason.unknown") };
        if (o.state === "done") return { sid: t.sessionId, outcome: "done", why: "" };
        return { sid: t.sessionId, outcome: o.state, why: reasonLabel(o.code, { agent: "", target: "" }) };
      });
      sayBatch(copyText("tabBatch.action.rot", { src }), tabs, outcomes);
      done();
    })();
  };
  const submenu: MenuItem[] = [
    { label: copyText("rot.src.followOf", { name: def?.name ?? "" }), onClick: () => apply("follow", copyText("rot.src.follow")) },
    ...rules.rules.map((r): MenuItem => ({ label: r.name, onClick: () => apply({ rule: r.id }, copyText("rot.src.rule", { name: r.name })) })),
    { label: "", divider: true },
    { label: copyText("rot.src.manage"), onClick: () => host.openRules?.(origin) },
  ];
  return { label, submenu };
}
