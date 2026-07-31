/**
 * F09（unify-launch）：UI 层"枚举当前可用修饰"的独立发现层——不是 `LaunchDimension` 的一部分。
 *
 * R12 决策（见 `doc/INVARIANTS.md` §38、`.claude/planned-build/unify-launch/features/
 * F09-ui-convergence.md` §0）：`container`/`agent` 两条轴**不**收进 `LAUNCH_DIMENSIONS` 注册表，
 * 继续硬编码。`account` 组虽然已经是注册表维度（`ACCOUNT_DIMENSION`），但 `LaunchDimension` 接口
 * 本身从未回答过"这个维度当前有哪些可选值"——那向来是 `src/accounts.ts::fetchAccounts` 现查的活。
 * 所以本文件手写取值逻辑，不是"该走注册表却没走"，是这条轴本来就该用另一种方式回答
 * "有哪些可选值"这个问题。
 *
 * **R05：原先本文件还产出一个 `container` 组，已删——它是死代码。** 证据（当时逐条核实）：
 * 全仓**唯一**生产调用点 `tabs.ts` 只做 `groups.find(g => g.id === "account")`，
 * container 组造出来从未被读；且它把第二参 `currentContainerKind` 恒传 `"tmux"`，
 * 于是那两项的 `selected` 标志也恒定退化；`"none"` 那条分支**只被测试驱动过、生产从未走到**。
 * 真正在渲染容器两项的是 `tabs.ts::buildResumeSubmenu` 里的 `containerLeaves`
 * （label 与这里原来那份**逐字相同**，两处各写一遍）。删掉本文件那份后它成为唯一来源
 * ——刻意**不**为它另抽公共常量：只有一个消费者的常量是无收益的间接层。
 *
 * 连带结果：组只剩一个，"枚举若干组"这个形状本身就是假象，故函数改名为
 * `enumerateAccountModifiers`、直接返回选项数组（组的 `label` 从来没有被消费过——
 * 审计另核实 `ModifierOption.title` 从未被写也从未被读、`selected` 只有 container 组写过无人读，
 * 即这次连带删掉了三个零消费者的字段）。
 *
 * **本文件现在只负责账号轴**；容器那两项（tmux / 直连）住在 `tabs.ts::containerLeaves`，
 * 是全仓唯一来源。找它们别再来这里。
 */
import { fetchAccounts, selectableAccounts } from "./accounts.ts";

/**
 * 一个账号修饰选项。
 *
 * **R05：判别联合取代了原来的裸魔法串 `id: "__base__"`。** 原先"这一项是基座还是某个账号"
 * 靠跨文件比较字符串字面量回答（本文件产出 `"__base__"`，`tabs.ts` 两处各自 `=== "__base__"`），
 * 拼错一个字符 tsc 抓不到，而行为是"基座选项静默变成一个名叫 `__base__` 的普通账号"
 * ——又是 R11/R08 那族「看起来生效了，只是用错了号」。改成判别联合后这个比较本身就不存在了，
 * `name` 也只在 `kind === "account"` 分支上可见（基座没有账号名，类型上就取不到）。
 */
export type AccountModifierOption =
  | { kind: "base"; label: string }
  | { kind: "account"; name: string; label: string };

/** `kind: "account"` 那一支——`buildRestartSubmenu` 只接受具名账号（重启不提供基座逃生口）。 */
export type NamedAccountModifier = Extract<AccountModifierOption, { kind: "account" }>;

/**
 * 枚举一个远端 origin 当前可用的**账号**修饰项，供 `tabs.ts` 的 flyout 渲染消费。
 * 无可用账号（账号功能不可用 / 0 个可选 / 探测失败）→ 返回空数组，调用方据此不渲染这一组。
 *
 * 恒含"基座（不隔离）"逃生口（F01 步骤2：有 ≥1 可选账号时 follow 默认会注入某号 →
 * 给老会话一个显式不隔离的出口，防 #75）；每个可选账号的具名选项只在 **≥2** 个可选账号时才追加
 * （只有 1 个账号时，"切到那唯一的账号"与跟随默认没有区别，不加噪）。
 *
 * F09 Phase D 审计（后端架构，重要）：`selectable` 只过 `isSelectable`（mode===isolated &&
 * loggedIn && exists），**没有**复刻旧版 `appendAccountMenuItems` 那句 `if (!a.configDir)
 * continue`——这是有意的行为变化，不是遗漏：旧版对 `configDir` 落空的账号是**静默隐藏**菜单项
 * （用户看不到这个账号、不知道为什么），新版是**显示、点击后走 `withAccount` 的
 * `onUnselectable` 回调**弹一次"账号不可用"的 toast（`tabs.ts::buildResumeSubmenu` 走的正是
 * 这条路径）。显式反馈优于静默隐藏，故意不搬那条 continue。
 */
export async function enumerateAccountModifiers(origin: string): Promise<AccountModifierOption[]> {
  let accountsAvailable = false;
  let selectable: { name: string }[] = [];
  try {
    const state = await fetchAccounts(origin);
    accountsAvailable = state.available;
    // R05 Phase D 审计（建议）：用既有的 `selectableAccounts`——它自己的注释就写着
    // 「休眠判据 / 计数一律走它，别各处再 filter 一遍」，而这里原先手写了同一个 filter。
    if (accountsAvailable) selectable = selectableAccounts(state);
  } catch {
    accountsAvailable = false;
  }
  if (!accountsAvailable || selectable.length < 1) return [];

  const options: AccountModifierOption[] = [{ kind: "base", label: "不指定账号（用已登录的那个）" }];
  if (selectable.length >= 2) {
    options.push(...selectable.map((a) => ({ kind: "account" as const, name: a.name, label: a.name })));
  }
  return options;
}
