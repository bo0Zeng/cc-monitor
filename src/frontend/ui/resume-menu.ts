/**
 * **「恢复」的那一组选项怎么摆**：怎么开（tmux / 直连）× 用哪个号 —— 标签页右键「恢复 ▸」与历史页「恢复 ▾」同一个组件、同一套选项
 * （设计稿「文件与历史」乙4-⑤）。
 *
 * 只排版：有哪几个号、每个号选不选得了，由调用方从那台后端拿到的事实给（`launch-menu.ts::enumerateAccountModifiers`）；
 * 选了之后怎么起（问那台后端判号、起会话）也由调用方做。顶层 tmux / 直连两项跟随默认的号；给了号的那一组，每个号再嵌一层 tmux / 直连。
 * （从 `tab-menu.ts` 原样搬来：项、文案、顺序一个字没改。）
 */
import { copyText } from "./copy-table";
import type { MenuItem } from "./kit/menu";
import type { AccountModifierOption } from "./launch-menu";

/** 选了哪一项：`tmux` = 在 tmux 里（否则直连）· `account` = 点名的号（缺 = 跟随）· `useBase` = 基座（不隔离）。 */
export interface ResumePick {
  tmux: boolean;
  account: string | undefined;
  useBase: boolean;
}

/** 「恢复」那一组的菜单项。`accountOptions` 空 ⇒ 只有顶层 tmux / 直连两项。 */
export function resumeMenuItems(accountOptions: AccountModifierOption[], pick: (p: ResumePick) => void): MenuItem[] {
  const containerLeaves = (account: string | undefined, useBase: boolean): MenuItem[] => [
    { label: "tmux", onClick: () => pick({ tmux: true, account, useBase }) },
    { label: copyText("tabMenu.containerLeaves.direct"), onClick: () => pick({ tmux: false, account, useBase }) },
  ];
  const items: MenuItem[] = [...containerLeaves(undefined, false)];
  if (accountOptions.length > 0) {
    // F09 Phase D 审计（UX，建议）：纯展示性分隔线——把上面"跟随默认账号"两项和下面"换账号"
    // 一组视觉分开，降低扫描成本（不增加点击次数，审计原话："综合任务时间…新版很可能相当
    // 甚至更快，不建议再加独立一级项，折中是加视觉分组"）。
    items.push({ label: "", divider: true });
    // R05：判别联合取代了 `opt.id === "__base__"` 这个跨文件字符串比较。
    for (const opt of accountOptions) {
      items.push({
        label: opt.label,
        submenu: opt.kind === "base" ? containerLeaves(undefined, true) : containerLeaves(opt.name, false),
      });
    }
  }
  return items;
}
