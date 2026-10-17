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
  /** 问名字（测试注入）；`note` ＝ 框下面那一句。 */
  ask?: (title: string, initial: string, note?: string) => Promise<string | null>;
  /** 挑一份文件读成文本（测试注入）。 */
  pickFile?: () => Promise<{ name: string; text: string } | null>;
}

export interface RotMount {
  root: HTMLElement;
  /** 重读重画（推送来了 · 面板打开那一刻）。 */
  refresh(): Promise<void>;
  /** 设置窗：看这一份、开这个号的编辑小窗（会话面板［编辑策略］跳过来）。 */
  focus(strategy: string, account: string | null): void;
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

/** 设置窗目的地里「看哪份 · 开哪个号」那一格（`rot:<策略>/<号>`；策略名 · 号名里都没有 `/`）。 */
export function rotAnchor(strategy: string, account: string | null): string {
  return `rot:${strategy}/${account ?? ""}`;
}

/** 认 [`rotAnchor`] 写的那一格；不是 ⇒ `null`。 */
export function parseRotAnchor(anchor: string | undefined): { strategy: string; account: string | null } | null {
  const m = anchor === undefined ? null : /^rot:([^/]+)\/(.*)$/.exec(anchor);
  return m ? { strategy: m[1], account: m[2] === "" ? null : m[2] } : null;
}

export function mountRotation(spec: RotMountSpec): RotMount {
  const root = document.createElement("div");
  const { origin, sid } = spec;
  const state: BoardState = { hours: 12, open: null, refused: null };
  let view: RotationViewReply | null = null;
  let list: RotationStrategiesReply | null = null;
  let readErr: string | null = null;
  let seq = 0;
  /** 设置窗在看哪一份（缺 ⇒ 新会话用的那份）。 */
  let shown: string | undefined;
  const confirmFn = spec.confirm ?? confirmDialog;
  const ask =
    spec.ask ??
    (async (title: string, initial: string, note?: string): Promise<string | null> =>
      await askText({
        title,
        action: note === undefined ? copyText("rot.ask.go") : copyText("rot.ask.createGo"),
        label: note === undefined ? copyText("rot.ask.name") : copyText("rot.ask.createName"),
        initial,
        note,
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
      else {
        const id = shown !== undefined && l.strategies.some((x) => x.id === shown) ? shown : l.forNew;
        if (id !== "") v = await readView(origin, { strategy: id }, state.hours);
      }
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

  /** 设置窗换去看 `id` 那一份（缺 ⇒ 新会话用的）。 */
  const show = (id: string | undefined, account: string | null = null): void => {
    shown = id;
    state.open = account;
    void refresh();
  };

  const acts: BoardActs = {
    // 会话：这个会话用它。设置窗：看它；「不轮换」⇒ 新会话不轮换。
    pick: (id) => {
      if (sid !== null)
        return void write(copyText("rot.fail.use"), () => callTyped(origin, "rotation-use", { sid, strategy: id }, rotBudget()));
      if (id !== null) return show(id);
      shown = undefined;
      void write(copyText("rot.fail.forNew"), () => callTyped(origin, "rotation-for-new", { strategy: null }, rotBudget()));
    },
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
    create: () =>
      void (async () => {
        const name = await ask(copyText("rot.ask.createTitle"), "", copyText("rot.ask.createNote"));
        if (!name) return;
        try {
          const r = await callTyped(origin, "rotation-strategy-add", { name }, rotBudget());
          if (sid === null) shown = r.id;
        } catch (e) {
          return void failToast(copyText("rot.fail.create"), e);
        }
        await refresh();
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
    // 编辑策略：开设置窗那台的「轮换」栏、看这个会话用的那份（给了号 ⇒ 开它的编辑小窗；策略只在那里改）。
    acts.editStrategy = (account) => {
      const id = view?.strategy?.id;
      void openSettingsWindow(null, { machine: origin, tab: "rot", ...(id === undefined ? {} : { anchor: rotAnchor(id, account ?? null) }) });
    };
  }
  paint();
  return { root, refresh, focus: (strategy, account) => show(strategy, account) };
}
