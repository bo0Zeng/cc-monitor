/**
 * 设置窗「轮换」栏的接线：读（`rotation-strategies` · `rotation-view {strategy}`）→ 画（`rot-board.ts`）→ 手势翻成命令 → 重读重画。
 * 目标是一份策略（选中的那份 ＝ 新会话用的；改一格当场存成那份的新版）。会话面板另有一处（`rot-session.ts`），
 * 两处共用的只有策略一行与提示那几颗按钮的命令（[`topActs`]）。界面不判：收不下的照后端的码说。
 */
import { board, draftKey, type BoardActs, type BoardState, type TopActs } from "./rot-board";
import { copyText } from "./copy-table";
import { callTyped } from "./ipc/chan-caller";
import type { Machine } from "./ipc/origin";
import { banner } from "./kit/banner";
import { openOnMachine } from "./file-window";
import { button } from "./kit/button";
import { askText, confirmDialog, type ConfirmFn } from "./kit/dialog";
import { failToast } from "./kit/toast";
import { readStrategies, readView, rotBudget } from "./rot-reads";
import { write } from "./ipc/write";
import { morph } from "./kit/morph";
import type { RotationStrategiesReply } from "./generated/RotationStrategiesReply";
import type { RotationViewReply } from "./generated/RotationViewReply";
import { saidOf } from "./kit/detail";

export interface RotMountSpec {
  origin: Machine;
  /** 第一行右边那台的名字。 */
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
  /** 重读重画（推送来了 · 栏打开那一刻）。 */
  refresh(): Promise<void>;
  /** 看这一份、开这个号的编辑小窗（会话面板［编辑策略］跳过来）。 */
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

/** 发一条写，结局归统一写口（`ipc/write.ts`）；不成 ⇒ 一条失败 toast（字是后端那一句）。 */
export async function sendWrite(what: string, run: () => Promise<unknown>, reread: () => Promise<void>, clear?: () => void): Promise<boolean> {
  const end = await write(run, { reread, clear });
  if (!end.ok) failToast(what, end.error);
  return end.ok;
}

/**
 * 策略一行与提示那几颗按钮的命令（会话面板 · 设置页同一处）：开文件 · 设时区 · 载入新版（会话 ⇒ 这个会话载入此刻那一版；
 * 设置页 ⇒ 在用旧版的会话都载入新版）· 旧格式那几份展开。`pick` 两处不同，由调用方给。
 */
export function topActs(o: {
  origin: Machine;
  sid: string | null;
  view: () => RotationViewReply | null;
  refresh: () => Promise<void>;
  toggleOld: () => void;
  pick: (id: string | null) => void;
}): TopActs {
  const { origin, sid } = o;
  return {
    pick: o.pick,
    showFile: (path) =>
      void openOnMachine(origin, { revealFile: path }).catch((e: unknown) => failToast(copyText("rot.fail.open"), e)),
    setTz: (zone) => {
      const id = o.view()?.strategy?.id;
      const ifRev = o.view()?.strategy?.rev;
      if (id === undefined || ifRev === undefined) return;
      void sendWrite(copyText("rot.fail.tz"), () => callTyped(origin, "rotation-strategy-tz", { id, tz: zone, ifRev }, rotBudget()), o.refresh);
    },
    loadNew: () => {
      const id = o.view()?.strategy?.id;
      if (id === undefined) return;
      void (sid !== null
        ? sendWrite(copyText("rot.fail.reload"), () => callTyped(origin, "rotation-use", { sid, strategy: id }, rotBudget()), o.refresh)
        : sendWrite(copyText("rot.fail.reload"), () => callTyped(origin, "rotation-reload", { strategy: id }, rotBudget()), o.refresh));
    },
    toggleOld: o.toggleOld,
  };
}

export function mountRotation(spec: RotMountSpec): RotMount {
  const root = document.createElement("div");
  const { origin } = spec;
  const state: BoardState = { hours: 12, open: null };
  let view: RotationViewReply | null = null;
  let list: RotationStrategiesReply | null = null;
  let readErr: string | null = null;
  let seq = 0;
  /** 在看哪一份（缺 ⇒ 新会话用的那份）。 */
  let shown: string | undefined;
  const confirmFn = spec.confirm ?? confirmDialog;
  const ask =
    spec.ask ??
    (async (title: string, initial: string, note?: string): Promise<string | null> =>
      await askText({
        title,
        action: note === undefined ? copyText("rot.ask.go") : copyText("mobTerms.new.detached"),
        label: note === undefined ? copyText("rot.ask.name") : copyText("acctNew.name.label"),
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
            label: copyText("front.act.retry"),
            kind: "ghost",
            size: "compact",
            onClick: () => void refresh(),
          }),
        ]),
      );
    }
    const b = board({ view, list, state, acts, origin, machine: spec.machine });
    b.prepend(...top);
    // 正打着字的那一格（焦点所在 · 改过没交）留着：kit morph。
    morph(root, [b]);
  };

  const refresh = async (): Promise<void> => {
    const my = ++seq;
    try {
      const l = await readStrategies(origin);
      const id = shown !== undefined && l.strategies.some((x) => x.id === shown) ? shown : l.forNew;
      const v = id !== "" ? await readView(origin, { strategy: id }, state.hours) : null;
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

  const send = (what: string, run: () => Promise<unknown>, clear?: () => void): Promise<boolean> => sendWrite(what, run, refresh, clear);

  /** 换去看 `id` 那一份（缺 ⇒ 新会话用的）。 */
  const show = (id: string | undefined, account: string | null = null): void => {
    shown = id;
    state.open = account;
    void refresh();
  };

  const acts: BoardActs = {
    ...topActs({
      origin,
      sid: null,
      view: () => view,
      refresh,
      toggleOld: () => {
        state.oldOpen = !state.oldOpen;
        paint();
      },
      // 看它；「不轮换」⇒ 新会话不轮换。
      pick: (id) => {
        if (id !== null) return show(id);
        shown = undefined;
        void send(copyText("rot.fail.forNew"), () => callTyped(origin, "rotation-for-new", { strategy: null }, rotBudget()));
      },
    }),
    copy: (id) =>
      void (async () => {
        const name = await ask(copyText("rot.ask.copyTitle", { name: id }), "");
        if (name)
          await send(copyText("detail.act.failed"), () =>
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
          await send(copyText("rot.fail.rename"), () =>
            callTyped(origin, "rotation-strategy-rename", { id, name, ifRev }, rotBudget()),
          );
      })(),
    forNew: (id) =>
      void send(copyText("rot.fail.forNew"), () =>
        callTyped(origin, "rotation-for-new", { strategy: id }, rotBudget()),
      ),
    remove: (id) =>
      void (async () => {
        const ok = await confirmFn({
          title: copyText("rot.ask.deleteTitle", { name: id }),
          action: copyText("rsFilewinWriteops.label.delete"),
          danger: true,
          body: copyText("rot.ask.deleteBody"),
        });
        if (ok)
          await send(copyText("rot.fail.delete"), () =>
            callTyped(origin, "rotation-strategy-delete", { id }, rotBudget()),
          );
      })(),
    create: () =>
      void (async () => {
        const name = await ask(copyText("rot.ask.createTitle"), "", copyText("rot.ask.createNote"));
        if (!name) return;
        try {
          const r = await callTyped(origin, "rotation-strategy-add", { name }, rotBudget());
          shown = r.id;
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
          await send(copyText("rot.fail.add"), () =>
            callTyped(origin, "rotation-strategy-add", { source: f.text, name }, rotBudget()),
          );
      })(),
    // 改一格当场存成那份的新版（版本对不上 ⇒ 重读，照此刻的再改）。草稿那一行交成了 ⇒ 草稿清掉。
    edit: (edit) => {
      const st = view?.strategy;
      if (!st) return;
      const seg = "seg" in edit && edit.seg.at === undefined && edit.seg.seg !== undefined ? edit.seg : null;
      const account = seg?.row === undefined ? null : (view?.board.rows.find((r) => r.row === seg.row)?.account ?? null);
      const done = seg ? () => void delete state.drafts?.[draftKey(account, seg.key, seg.window)] : undefined;
      void send(
        copyText("rot.fail.settings"),
        () => callTyped(origin, "rotation-settings", { strategy: st.id, edit, ifRev: st.rev }, rotBudget()),
        done,
      );
    },
    hours: (h) => {
      state.hours = h;
      void refresh();
    },
    open: (a) => {
      state.open = a;
      paint();
    },
  };
  paint();
  return { root, refresh, focus: (strategy, account) => show(strategy, account) };
}
