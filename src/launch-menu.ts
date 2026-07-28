/**
 * F09（unify-launch）：UI 层"枚举当前可用修饰"的独立发现层——不是 `LaunchDimension` 的一部分。
 *
 * R12 决策（见 `doc/INVARIANTS.md` §38、`.claude/planned-build/unify-launch/features/
 * F09-ui-convergence.md` §0）：`container`/`agent` 两条轴**不**收进 `LAUNCH_DIMENSIONS` 注册表，
 * 继续硬编码。`account` 组虽然已经是注册表维度（`ACCOUNT_DIMENSION`），但 `LaunchDimension` 接口
 * 本身从未回答过"这个维度当前有哪些可选值"——那向来是 `src/accounts.ts::fetchAccounts` 现查的活。
 * 所以本文件手写两个组的取值逻辑，不是"该走注册表却没走"，是两条轴本来就该用不同方式回答
 * "有哪些可选值"这个问题。
 */
import { fetchAccounts, isSelectable } from "./accounts.ts";

export interface ModifierOption {
  /** 稳定 key：账号名 / "__base__" / "tmux" / "none"。 */
  id: string;
  label: string;
  /** 当前 ctx 是否已是这个值（供打勾/高亮，非必需）。 */
  selected?: boolean;
  title?: string;
}

export interface ModifierGroup {
  id: "account" | "container";
  label: string;
  options: ModifierOption[];
}

/**
 * 枚举一个远端 origin 当前可用的修饰组，供 tabs.ts 的 flyout 渲染消费。
 *
 * account 组：≥1 个可选账号时才出现，恒含"基座（不隔离）"逃生口（F01 步骤2：有 ≥1 可选账号时,
 * follow 默认会注入某号 → 给老会话一个显式不隔离的出口，防 #75）；每个可选账号的具名选项只在
 * ≥2 个可选账号时才追加（只有 1 个账号时,"切到那唯一的账号"与跟随默认没有区别,不加噪）。
 * container 组：值域固定、硬编码两项——它不随 `LAUNCH_DIMENSIONS` 的增减而变化，是独立的一条轴
 * （R12 决策）。`mode`（create-or-attach/send-into/attach-only）不在此暴露：它是点击那一刻
 * 现查远端状态派生出来的值，用户从未也不该在 flyout 里选它。
 *
 * F09 Phase D 审计（后端架构，重要）：`selectable` 只过 `isSelectable`（mode===isolated &&
 * loggedIn && exists），**没有**复刻旧版 `appendAccountMenuItems` 那句 `if (!a.configDir)
 * continue`——这是有意的行为变化，不是遗漏：旧版对 `configDir` 落空的账号是**静默隐藏**菜单项
 * （用户看不到这个账号、不知道为什么），新版是**显示、点击后走 `withAccount` 的
 * `onUnselectable` 回调**弹一次"账号不可用"的 toast（`tabs.ts::buildResumeSubmenu` 走的正是
 * 这条路径）。显式反馈优于静默隐藏，故意不搬那条 continue。
 */
export async function enumerateModifierGroups(
  origin: string,
  currentContainerKind: "tmux" | "none",
): Promise<ModifierGroup[]> {
  const groups: ModifierGroup[] = [];

  let accountsAvailable = false;
  let selectable: { name: string }[] = [];
  try {
    const state = await fetchAccounts(origin);
    accountsAvailable = state.available;
    if (accountsAvailable) selectable = state.accounts.filter(isSelectable);
  } catch {
    accountsAvailable = false;
  }
  if (accountsAvailable && selectable.length >= 1) {
    const options: ModifierOption[] = [{ id: "__base__", label: "基座（不隔离）" }];
    if (selectable.length >= 2) {
      options.push(...selectable.map((a) => ({ id: a.name, label: a.name })));
    }
    groups.push({ id: "account", label: "账号", options });
  }

  groups.push({
    id: "container",
    label: "容器",
    options: [
      { id: "tmux", label: "tmux", selected: currentContainerKind === "tmux" },
      { id: "none", label: "直连（不建 tmux）", selected: currentContainerKind === "none" },
    ],
  });

  return groups;
}
