/**
 * 会话账号面板中间那一块（目标 `{sid}`：照这个会话载入的那份副本；只做这个会话的临时动作）：
 * 提示（后端那几条）→ 策略一行（下拉 ＋［编辑策略］；说明一行截断，点「说明」展开，展开与否归界面）→ 顺序（`rot-order.ts`）。
 * 不轮换 ＝ 不自动换：策略一行 ＋ 那一句 ＋ 同一个顺序（号照后端排、不可拖，点一行照样［立即用］）。与设置页共用的只有策略一行与提示（`rot-board.ts` 的 `firstRow` · `hints`）。
 *
 * - 读 `rotation-strategies` · `rotation-view {sid}`；手势翻成命令（`rotation-use` · `rotation-temp` · `rotation-use-now`），成了重读。
 * - 拖着一行 · 等回包时推送来的重读先记下，静止了再读（手上正在改的不被盖掉）。读不出 ⇒ 红条 ＋［重试］，上次所知的淡掉、不拿起。
 * 判据：`tests/frontend/ui/rot-session.vitest.ts`。
 */
import { accountLabel } from "./acct-words";
import { copyText } from "./copy-table";
import { callTyped, refusalIn } from "./ipc/chan-caller";
import type { Machine } from "./ipc/origin";
import { write } from "./ipc/write";
import { banner } from "./kit/banner";
import { button } from "./kit/button";
import { saidOf } from "./kit/detail";
import { el } from "./kit/el";
import { morph } from "./kit/morph";
import { failToast } from "./kit/toast";
import { openSettingsWindow } from "./open-settings";
import { firstRow, hints, type TopSpec } from "./rot-board";
import { rotAnchor, sendWrite, topActs } from "./rot-mount";
import { createOrder, type MoveEnd } from "./rot-order";
import { readStrategies, readView, rotBudget } from "./rot-reads";
import type { RotTempOp } from "./generated/RotTempOp";
import type { RotationStrategiesReply } from "./generated/RotationStrategiesReply";
import type { RotationViewReply } from "./generated/RotationViewReply";
import s from "./rot-order.module.css";

/** 骨架字与那几句失败标题（不随数据变）。 */
const W = {
  retry: copyText("rot.read.retry"),
  about: copyText("rot.top.about"),
  aboutClose: copyText("rot.top.aboutClose"),
  none: copyText("rot.none.desc"),
  failTemp: copyText("rot.fail.temp"),
  failUse: copyText("rot.fail.use"),
  failUseNow: copyText("rot.fail.useNow"),
};

export interface SessionRot {
  root: HTMLElement;
  /** 重读重画（推送来了 · 面板打开那一刻）；拿着一行时先记下。 */
  refresh(): Promise<void>;
}

export function mountSessionRotation(origin: Machine, sid: string): SessionRot {
  const root = el("div", s.rotSession);
  let view: RotationViewReply | null = null;
  let list: RotationStrategiesReply | null = null;
  let readErr: string | null = null;
  let refused: { account: string; text: string } | null = null;
  let aboutOpen = false;
  let oldOpen = false;
  let seq = 0;
  /** 拿着一行时来过一次重读 · 刚交成一次挪动：静止了再读。 */
  let owed = false;

  const reread = async (): Promise<void> => {
    const my = ++seq;
    try {
      const l = await readStrategies(origin);
      const v = await readView(origin, { sid });
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
  const refresh = async (): Promise<void> => {
    if (order.busy()) {
      owed = true;
      return;
    }
    await reread();
  };

  /** 换到 `account`（小窗［立即用］）· 拖到第一格：那台说它事实上发不出去（unusable）⇒ 不换，原因写在顺序上方。 */
  const blockedLine = (account: string, said: string): { account: string; text: string } => {
    const why = view?.accounts.find((f) => f.account === account)?.blocked?.text ?? said;
    return { account, text: copyText("rot.acct.refused", { account: accountLabel(account), why }) };
  };
  const temp = (op: RotTempOp): Promise<boolean> => sendWrite(W.failTemp, () => callTyped(origin, "rotation-temp", { sid, op }, rotBudget()), refresh);

  const order = createOrder(
    origin,
    {
      move: async (account, to): Promise<MoveEnd> => {
        const end = await write(() => callTyped(origin, "rotation-temp", { sid, op: { move: { account, to } } }, rotBudget()));
        if (end.ok) {
          refused = null;
          owed = true;
          return "done";
        }
        const code = refusalIn(end.error)?.code;
        // 没有拒码（超时 · 断线）⇒ 说不清：留在放下处，下一次重读（连上 · 推送）照后端画。
        if (code === undefined) return "unsure";
        const blocked = blockedLine(account, end.said);
        if (code === "unusable") refused = blocked;
        else failToast(W.failTemp, end.error);
        return "back";
      },
      off: (account, off) => void temp({ off: { account, off } }),
      clear: () => void temp("clear"),
      useNow: (account) =>
        void (async () => {
          const end = await write(() => callTyped(origin, "rotation-use-now", { sid, account }, rotBudget()));
          const blocked = end.ok ? null : blockedLine(account, end.said);
          if (end.ok) refused = null;
          else if (refusalIn(end.error)?.code !== "unusable") return void failToast(W.failUseNow, end.error);
          else refused = blocked;
          await refresh();
        })(),
      editStrategy: (account) => editStrategy(account),
    },
    // 静止了：交成过 · 拿着时来过推送 ⇒ 重读；否则照手上那一份重画（被拦下的那一句）。
    () => {
      if (!owed) return paint();
      owed = false;
      void reread();
    },
  );

  // 编辑策略：开设置窗那台的「轮换」栏、看这个会话用的那份（给了号 ⇒ 开它的编辑小窗；策略只在那里改）。
  const editStrategy = (account?: string): void => {
    const id = view?.strategy?.id;
    void openSettingsWindow(null, { machine: origin, tab: "rot", ...(id === undefined ? {} : { anchor: rotAnchor(id, account ?? null) }) });
  };

  const top: TopSpec["acts"] = {
    ...topActs({
      origin,
      sid,
      view: () => view,
      refresh,
      toggleOld: () => {
        oldOpen = !oldOpen;
        paint();
      },
      pick: (id) => void sendWrite(W.failUse, () => callTyped(origin, "rotation-use", { sid, strategy: id }, rotBudget()), refresh),
    }),
    editStrategy,
  };

  /** 说明：一行截断 ＋「说明」展开全文（不轮换 ⇒ 只那一句；号照样在顺序里）。 */
  const about = (v: RotationViewReply): HTMLElement => {
    const box = el("div", s.rotSessionAbout);
    if (!v.strategy) {
      box.append(el("span", s.rotSessionAboutText, W.none));
      return box;
    }
    const st = v.strategy;
    const text = el("span", s.rotSessionAboutText, aboutOpen ? st.description : st.summary);
    box.dataset.unfolded = String(aboutOpen);
    box.appendChild(text);
    if (st.description.trim() !== st.summary.trim()) {
      const t = button({
        label: aboutOpen ? W.aboutClose : W.about,
        kind: "ghost",
        size: "compact",
        onClick: () => {
          aboutOpen = !aboutOpen;
          paint();
        },
      });
      t.setAttribute("aria-expanded", String(aboutOpen));
      box.appendChild(t);
    }
    return box;
  };

  const paint = (): void => {
    const kids: Node[] = [];
    if (readErr !== null) kids.push(banner("error", readErr, [button({ label: W.retry, kind: "ghost", size: "compact", onClick: () => void refresh() })]));
    const spec: TopSpec = { view, list, acts: top, oldOpen };
    kids.push(...hints(spec), firstRow(spec));
    if (view) {
      kids.push(about(view));
      order.draw({ view, refused, offline: readErr !== null });
      kids.push(order.el);
    }
    root.dataset.stale = String(readErr !== null && view !== null);
    morph(root, kids);
  };

  paint();
  return { root, refresh };
}
