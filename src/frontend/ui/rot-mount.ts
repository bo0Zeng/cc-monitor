/**
 * 轮换四块的接线：读（`rotation-strategies` · `rotation-view`）→ 画（`rot-board.ts`）→ 手势翻成命令 → 重读重画。
 * 会话面板（目标 `{sid}`：改动先成这个会话的草稿，存在后端）与设置窗「轮换」栏（目标 `{strategy, settings}`：草稿在界面手上，
 * 选中的那份 ＝ 新会话用的）共用这一处；两边只差这几条命令发给谁。界面不判：收不下的照后端的码说。
 */
import { accountLabel } from "./acct-words";
import { board, type BoardActs, type BoardState } from "./rot-board";
import { copyText } from "./copy-table";
import { callTyped, saidFrom } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { askText, confirmDialog, type ConfirmFn } from "./kit/dialog";
import { failToast } from "./kit/toast";
import { readStrategies, readView, refusedCode, rotBudget } from "./rot-reads";
import type { RotSeg } from "./generated/RotSeg";
import type { RotationStrategiesReply } from "./generated/RotationStrategiesReply";
import type { RotationViewReply } from "./generated/RotationViewReply";

export interface RotMountSpec {
  origin: Origin;
  /** 会话面板 ⇒ 那个会话；设置窗 ⇒ `null`（看新会话用的那份）。 */
  sid: string | null;
  /** 设置窗第一行右边那台的名字。 */
  machine?: string;
  /** 问一句（测试注入）。 */
  confirm?: ConfirmFn;
  /** 问名字（测试注入）。 */
  ask?: (title: string, initial: string) => Promise<string | null>;
  /** 挑一份文件读成文本（测试注入）。 */
  pickFile?: () => Promise<{ name: string; text: string } | null>;
}

export interface RotMount {
  root: HTMLElement;
  /** 重读重画（推送来了 · 面板打开那一刻）。 */
  refresh(): Promise<void>;
}

/** 浏览器里挑一份 `.rhai` 读成文本。 */
function pickRhai(): Promise<{ name: string; text: string } | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".rhai";
    input.addEventListener("change", () => {
      const f = input.files?.[0];
      if (!f) return resolve(null);
      void f.text().then(
        (text) => resolve({ name: f.name.replace(/\.rhai$/i, ""), text }),
        () => resolve(null),
      );
    });
    input.click();
  });
}

export function mountRotation(spec: RotMountSpec): RotMount {
  const root = document.createElement("div");
  const { origin, sid } = spec;
  const state: BoardState = { hours: 12, open: null, refused: null };
  let view: RotationViewReply | null = null;
  let list: RotationStrategiesReply | null = null;
  let draft: unknown = undefined;
  let readErr: string | null = null;
  let conflict = false;
  let seq = 0;
  const confirmFn = spec.confirm ?? confirmDialog;
  const ask =
    spec.ask ??
    (async (title: string, initial: string): Promise<string | null> =>
      await askText({
        title,
        action: copyText("rot.ask.go"),
        label: copyText("rot.ask.name"),
        initial,
      }));
  const pickFile = spec.pickFile ?? pickRhai;

  const paint = (): void => {
    const top: HTMLElement[] = [];
    if (readErr !== null) {
      top.push(
        banner("error", readErr, [
          button({
            label: copyText("rot.read.retry"),
            kind: "ghost",
            size: "compact",
            onClick: () => void refresh(),
          }),
        ]),
      );
    }
    if (conflict) {
      top.push(
        banner("info", copyText("rot.hint.newer"), [
          button({
            label: copyText("rot.hint.loadNew"),
            kind: "secondary",
            size: "compact",
            onClick: () => void reloadNew(),
          }),
          button({
            label: copyText("rot.hint.keepMine"),
            kind: "ghost",
            size: "compact",
            onClick: () => ((conflict = false), paint()),
          }),
        ]),
      );
    }
    const b = board({
      view,
      list,
      state,
      acts,
      origin,
      where: sid === null ? "settings" : "session",
      machine: spec.machine,
    });
    b.prepend(...top);
    root.replaceChildren(b);
  };

  const refresh = async (): Promise<void> => {
    const my = ++seq;
    try {
      const l = await readStrategies(origin);
      let v: RotationViewReply | null = null;
      if (sid !== null) v = await readView(origin, { sid }, state.hours);
      else if (l.forNew !== "")
        v = await readView(
          origin,
          draft === undefined
            ? { strategy: l.forNew }
            : { strategy: l.forNew, settings: draft },
          state.hours,
        );
      if (my !== seq) return;
      list = l;
      view = v;
      readErr = null;
    } catch (e) {
      if (my !== seq) return;
      readErr = saidFrom(e, origin);
    }
    paint();
  };

  /** 发一条写；不成 ⇒ 一条失败 toast（字是后端那一句）。成了 ⇒ 重读。 */
  const write = async (
    what: string,
    run: () => Promise<unknown>,
  ): Promise<boolean> => {
    try {
      await run();
    } catch (e) {
      const r = refusedCode(e);
      if (r?.code === "conflict") {
        conflict = true;
        paint();
        return false;
      }
      failToast(what, e);
      return false;
    }
    await refresh();
    return true;
  };

  const reloadNew = async (): Promise<void> => {
    conflict = false;
    draft = undefined;
    if (sid !== null)
      await write(copyText("rot.fail.revert"), () =>
        callTyped(origin, "rotation-revert", { sid }, rotBudget()),
      );
    else await refresh();
  };

  const rev = (): string => view?.strategy?.rev ?? "";

  const acts: BoardActs = {
    pick: (id) =>
      void (async () => {
        if (sid === null) {
          draft = undefined;
          await write(copyText("rot.fail.forNew"), () =>
            callTyped(origin, "rotation-for-new", { strategy: id }, rotBudget()),
          );
          return;
        }
        try {
          await callTyped(origin, "rotation-use", {
            sid,
            strategy: id,
            discard: false,
          }, rotBudget());
        } catch (e) {
          if (refusedCode(e)?.code !== "dirty")
            return void failToast(copyText("rot.fail.use"), e);
          const ok = await confirmFn({
            title: copyText("rot.ask.discardTitle"),
            action: copyText("rot.ask.discardGo"),
            body: copyText("rot.ask.discardBody"),
          });
          if (!ok) return paint();
          if (
            !(await write(copyText("rot.fail.use"), () =>
              callTyped(origin, "rotation-use", {
                sid,
                strategy: id,
                discard: true,
              }, rotBudget()),
            ))
          )
            return;
        }
        await refresh();
      })(),
    copy: (id) =>
      void (async () => {
        const name = await ask(copyText("rot.ask.copyTitle", { name: id }), "");
        if (name)
          await write(copyText("rot.fail.copy"), () =>
            callTyped(origin, "rotation-strategy-copy", { id, name }, rotBudget()),
          );
      })(),
    rename: (id, ifRev) =>
      void (async () => {
        const name = await ask(
          copyText("rot.ask.renameTitle", { name: id }),
          id,
        );
        if (name && name !== id)
          await write(copyText("rot.fail.rename"), () =>
            callTyped(origin, "rotation-strategy-rename", { id, name, ifRev }, rotBudget()),
          );
      })(),
    forNew: (id) =>
      void write(copyText("rot.fail.forNew"), () =>
        callTyped(origin, "rotation-for-new", { strategy: id }, rotBudget()),
      ),
    remove: (id) =>
      void (async () => {
        const ok = await confirmFn({
          title: copyText("rot.ask.deleteTitle", { name: id }),
          action: copyText("rot.ask.deleteGo"),
          danger: true,
          body: copyText("rot.ask.deleteBody"),
        });
        if (ok)
          await write(copyText("rot.fail.delete"), () =>
            callTyped(origin, "rotation-strategy-delete", { id }, rotBudget()),
          );
      })(),
    add: () =>
      void (async () => {
        const f = await pickFile();
        if (!f) return;
        const name = await ask(copyText("rot.ask.addTitle"), f.name);
        if (name)
          await write(copyText("rot.fail.add"), () =>
            callTyped(origin, "rotation-strategy-add", { source: f.text, name }, rotBudget()),
          );
      })(),
    settings: (next) => {
      if (sid === null) {
        draft = next;
        void refresh();
        return;
      }
      void write(copyText("rot.fail.settings"), () =>
        callTyped(origin, "rotation-settings", {
          target: { sid },
          settings: next,
          ifRev: rev(),
        }, rotBudget()),
      );
    },
    save: () =>
      void (async () => {
        const id = view?.strategy?.id;
        if (sid !== null) {
          await write(copyText("rot.fail.save"), () =>
            callTyped(origin, "rotation-save", { target: { sid }, ifRev: rev() }, rotBudget()),
          );
        } else if (id) {
          const ok = await write(copyText("rot.fail.save"), () =>
            callTyped(origin, "rotation-save", {
              target: { strategy: id },
              settings: draft,
              ifRev: rev(),
            }, rotBudget()),
          );
          if (ok) {
            draft = undefined;
            await refresh();
          }
        }
      })(),
    revert: () => {
      if (sid !== null)
        void write(copyText("rot.fail.revert"), () =>
          callTyped(origin, "rotation-revert", { sid }, rotBudget()),
        );
      else {
        draft = undefined;
        void refresh();
      }
    },
    dayPaint: async (
      segs: RotSeg[],
      from: number,
      to: number,
      v: number | null,
    ) => {
      try {
        return (
          await callTyped(origin, "rotation-day-paint", { segs, from, to, v }, rotBudget())
        ).segs;
      } catch (e) {
        failToast(copyText("rot.fail.settings"), e);
        throw e;
      }
    },
    hours: (h) => {
      state.hours = h;
      void refresh();
    },
    open: (a) => {
      state.open = a;
      state.refused = null;
      paint();
    },
  };
  if (sid !== null) {
    acts.useNow = (account) =>
      void (async () => {
        try {
          await callTyped(origin, "rotation-use-now", { sid, account }, rotBudget());
          state.refused = null;
        } catch (e) {
          if (refusedCode(e)?.code !== "unusable")
            return void failToast(copyText("rot.fail.useNow"), e);
          const why =
            view?.accounts.find((f) => f.account === account)?.blocked?.text ??
            saidFrom(e, origin);
          state.refused = {
            account,
            text: copyText("rot.acct.refused", {
              account: accountLabel(account),
              why,
            }),
          };
        }
        await refresh();
      })();
    acts.temp = (op) =>
      void write(copyText("rot.fail.temp"), () =>
        callTyped(origin, "rotation-temp", { sid, op }, rotBudget()),
      );
  }
  paint();
  return { root, refresh };
}
