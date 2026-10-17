/**
 * 轮换的接线：读（`rotation-strategies` · `rotation-view`）→ 画（`rot-board.ts`）→ 手势翻成命令 → 重读重画。
 * 会话面板（目标 `{sid}`：照它载入的那份副本；只做这个会话的临时动作）与设置窗「轮换」栏（目标 `{strategy}`：选中的那份 ＝ 新会话用的；
 * 改一格当场存成那份的新版）共用这一处。界面不判：收不下的照后端的码说。
 */
import { accountLabel } from "./acct-words";
import { board, type BoardActs, type BoardState } from "./rot-board";
import { copyText } from "./copy-table";
import { callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { banner } from "./kit/banner";
import { openOnMachine } from "./file-window";
import { openSettingsWindow } from "./open-settings";
import { button } from "./kit/button";
import { askText, confirmDialog, type ConfirmFn } from "./kit/dialog";
import { failToast } from "./kit/toast";
import { readStrategies, readView, refusedCode, rotBudget } from "./rot-reads";
import type { RotationStrategiesReply } from "./generated/RotationStrategiesReply";
import type { RotationViewReply } from "./generated/RotationViewReply";
import { saidOf } from "./kit/detail";

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
  let readErr: string | null = null;
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
      else if (l.forNew !== "") v = await readView(origin, { strategy: l.forNew }, state.hours);
      if (my !== seq) return;
      list = l;
      view = v;
      readErr = null;
    } catch (e) {
      if (my !== seq) return;
      readErr = saidOf(e);
    }
    paint();
  };

  /** 发一条写；不成 ⇒ 一条失败 toast（字是后端那一句）；撞上 `conflict`（别处改过）⇒ 不 toast、重读（提示由后端出）。成了 ⇒ 重读。 */
  const write = async (
    what: string,
    run: () => Promise<unknown>,
  ): Promise<boolean> => {
    try {
      await run();
    } catch (e) {
      if (refusedCode(e)?.code === "conflict") {
        await refresh();
        return false;
      }
      failToast(what, e);
      return false;
    }
    await refresh();
    return true;
  };

  const acts: BoardActs = {
    pick: (id) =>
      void (sid === null
        ? write(copyText("rot.fail.forNew"), () =>
            callTyped(origin, "rotation-for-new", { strategy: id }, rotBudget()),
          )
        : write(copyText("rot.fail.use"), () =>
            callTyped(origin, "rotation-use", { sid, strategy: id }, rotBudget()),
          )),
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
    // 设置页：改一格当场存成那份的新版（版本对不上 ⇒ 重读，照此刻的再改）。
    edit: (edit) => {
      const st = view?.strategy;
      if (st)
        void write(copyText("rot.fail.settings"), () =>
          callTyped(origin, "rotation-settings", { strategy: st.id, edit, ifRev: st.rev }, rotBudget()),
        );
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
    showFile: (path) =>
      void openOnMachine(origin, { revealFile: path }).catch((e: unknown) =>
        failToast(copyText("rot.fail.open"), e),
      ),
    setTz: (zone) => {
      const id = view?.strategy?.id;
      const ifRev = view?.strategy?.rev;
      if (id === undefined || ifRev === undefined) return;
      void write(copyText("rot.fail.tz"), () =>
        callTyped(origin, "rotation-strategy-tz", { id, tz: zone, ifRev }, rotBudget()),
      );
    },
    // 会话：载入它那份此刻的版本（同下拉选它）。设置页：在用旧版的会话都载入新版。
    loadNew: () => {
      const id = view?.strategy?.id;
      if (id === undefined) return;
      void (sid !== null
        ? write(copyText("rot.fail.reload"), () => callTyped(origin, "rotation-use", { sid, strategy: id }, rotBudget()))
        : write(copyText("rot.fail.reload"), () => callTyped(origin, "rotation-reload", { strategy: id }, rotBudget())));
    },
    toggleOld: () => {
      state.oldOpen = !state.oldOpen;
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
            saidOf(e);
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
    // 编辑策略：开设置窗那台的「轮换」栏（策略只在那里改）。
    acts.editStrategy = () => void openSettingsWindow(null, { machine: origin, tab: "rot" });
  }
  paint();
  return { root, refresh };
}
