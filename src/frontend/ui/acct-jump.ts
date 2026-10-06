/**
 * 设置窗账号页那一行「时间轴 · 默认轮换」点进来（`open-account-panel` 事件）：主窗口拉到前面，开账号面板并滚到那一节。
 *
 * - 当前标签页就在那台 ⇒ 开它的面板。
 * - 不在那台、那台有会话 ⇒ 不替人切标签页：一条提示说那一节在哪、第二行是那台排最前的会话，［切过去］点了才切并开面板。
 * - 那台一个会话都没开 ⇒ 一条提示，不开面板。
 *
 * 只开、只滚：不写轮换、不改跟不跟随默认。
 */
import { copyText } from "./copy-table";
import { machineName } from "./control-said";
import type { Origin } from "./ipc/origin";
import { toast } from "./kit/toast";
import type { AcctPanelAnchor } from "./acct-panel";
import type { OpenAccountPanel } from "./settings/events";

export interface AcctJumpHost {
  /** 主窗口拉到前面。 */
  raise(): void;
  /** 当前标签页（会话 · 哪台）；没有 ⇒ `null`。 */
  active(): { sid: string; origin: Origin } | null;
  /** 那台在标签页栏上排最前的会话；没有 ⇒ `null`。 */
  firstOn(origin: Origin): { sid: string; title: string } | null;
  switchTo(sid: string): void;
  openAt(sid: string, origin: Origin, anchor: AcctPanelAnchor): void;
}

export function jumpToAccountPanel(p: OpenAccountPanel, host: AcctJumpHost): void {
  host.raise();
  const origin = p.machine;
  const cur = host.active();
  if (cur && cur.origin === origin) {
    host.openAt(cur.sid, origin, p.anchor);
    return;
  }
  const machine = machineName(origin);
  const first = host.firstOn(origin);
  if (!first) {
    toast(copyText("acct.jump.noSession", { machine }), "", { level: "info" });
    return;
  }
  const where = p.anchor === "timeline" ? copyText("acct.jump.timelineElsewhere", { machine }) : copyText("acct.jump.rotationElsewhere", { machine });
  // 会话标题写在第二行、按钮只两个字：标题常很长，写进按钮会把那一句挤出 toast。
  toast(where, first.title, {
    level: "info",
    action: {
      label: copyText("acct.jump.switchTo"),
      run: () => {
        host.switchTo(first.sid);
        host.openAt(first.sid, origin, p.anchor);
      },
    },
  });
}
