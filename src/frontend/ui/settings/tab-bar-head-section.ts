/**
 * 设置页外观那一页「栏顶按钮」一节（锚 `tab-bar-head`：栏顶右键 · ⋯「调整栏顶按钮…」落到这里）：
 * 清单与会话头那一份同一个部件（`session-head-edit.ts::RowEditor`）—— 拖动排先后 · 勾不勾 · ［恢复默认］。
 * 立刻生效：每一下只写 `config.json` 的 `tabBar.head`（恢复缺省 ＝ 删这个键，`global-dests.ts`），写成了广播「设置已应用」，主窗口照它重摆；
 * 写不上出声、不打断（清单照这一下画着，下次打开照盘上的）。
 */
import { copyText } from "../copy-table";
import { failToast } from "../kit/toast";
import { DEFAULT_HEAD, HEAD_ENTRIES, HEAD_IDS, getTabBarHead, setTabBarHead, type HeadId } from "../global-dests";
import { RowEditor, type RowFace } from "../session-head-edit";

export interface TabBarHeadSectionDeps {
  /** 读盘上那一排（缺 ⇒ `global-dests.ts`）。 */
  read?: () => Promise<HeadId[]>;
  /** 写这一排；`null` ＝ 恢复缺省（缺 ⇒ `global-dests.ts`）。 */
  write?: (ids: readonly HeadId[] | null) => Promise<void>;
  /** 写成了：告诉主窗口重摆（设置窗广播「设置已应用」）。 */
  written: () => void;
}

const FACE = Object.fromEntries(HEAD_ENTRIES.map((e) => [e.id, { icon: e.icon, name: e.label }])) as Record<HeadId, RowFace>;

export function tabBarHeadSection(deps: TabBarHeadSectionDeps): HTMLElement {
  const read = deps.read ?? getTabBarHead;
  const write = deps.write ?? setTabBarHead;
  let cur: HeadId[] = [...DEFAULT_HEAD];
  const commit = (next: HeadId[] | null): void => {
    cur = next === null ? [...DEFAULT_HEAD] : next;
    ed.update();
    write(next).then(
      () => deps.written(),
      (e: unknown) => failToast(copyText("tabBarHead.edit.failed"), e),
    );
  };
  const ed = new RowEditor<HeadId>(
    {
      all: HEAD_IDS,
      face: FACE,
      title: copyText("tabBarHead.edit.title"),
      hint: copyText("tabBarHead.edit.hint"),
      overflow: copyText("tabBarHead.edit.overflow"),
    },
    { buttons: () => cur, set: (next) => commit(next), reset: () => commit(null) },
  );
  ed.el.dataset.anchor = "tab-bar-head";
  void read().then((h) => {
    cur = h;
    ed.update();
  });
  return ed.el;
}
