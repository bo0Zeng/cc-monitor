/**
 * K-A1 `KAY4`：**没有第二条只看 `loggedIn` 就下可用性结论的路。**
 *
 * 判定（零命中守卫）：生产 `.ts`（`*.vitest.ts` / `*.test.ts` / `src/generated/` 之外）
 * 里，`loggedIn` / `authReady` / `authKind` 只许出现在**登记过的那几个文件**里；
 * 可用性一律走 `accounts.ts::isSelectable`。
 *
 * # 这条守卫**抓不到**什么（写清楚，别让它看起来比实际强）
 *
 * 1. **它按字符串找。** 有人写 `const flag = a["loggedIn" as const]`、或把字段先解构再改名
 *    传下去（`const { loggedIn: ok } = a; …` —— 这一形**能**抓到，因为字面量还在；
 *    但 `JSON.parse(raw).loggedIn` 这类经过 `any` 的读法，字面量若被拼出来就抓不到）。
 *    ⇒ 真正的地板不是本文件，是 **`isSelectable` 是唯一出口**这件事本身；
 *    本文件只让「又开一条」这个动作**被看见**。
 * 2. **它是 vitest，扫不到 Rust 侧。** Rust 那边的同类绕过由两条同名判据各守自己那份：
 *    `src-tauri/src/local_accounts.rs::tests::the_auth_dimension_has_exactly_one_computation_path`
 *    与 `remote-daemon-proto/src/observe/accounts_query.rs::tests::（同名）`。
 * 3. **它不判语义。** 一个文件即使不提 `loggedIn`，也可以自己写
 *    `a.mode === "isolated" && a.exists` 冒充可用性判据 —— 那一格今天不钉
 *    （针只有一根，多了会把大量正常代码判红）。
 *
 * # 阴性对照（这条守卫真的有牙吗）
 *
 * K-A1 `KAM5` 实测：往 `src/launch-menu.ts` 加一句 `const ok = a.loggedIn;`
 * ⇒ 本文件第 2 条当场红（实测输出贴在件计划 `§3`）。撤掉即绿。
 */
import { describe, it, expect } from "vitest";

import { productionTsFiles } from "./test-support/production-sources.ts";
import { stripComments } from "./test-support/strip-comments.ts";

/** 出现次数（**去注释后**的代码里）。用正则而不是 `.includes`：要的是次数，不是有无。 */
function hits(code: string, ident: string): number {
  return code.match(new RegExp(`\\b${ident}\\b`, "g"))?.length ?? 0;
}

/**
 * ★ **登记表：这三个字段今天各自允许住在哪里。**
 *
 * `null` = 允许出现但不钉次数（只钉「就这一个文件」）；数字 = 钉死次数（加一处就红）。
 *
 * ⚠ **`src/account-chip.ts` 那一格已经没了（K-A1 第二轮落的），别再加回来。**
 * 它曾经登记 `loggedIn: 1` —— 那是设置里那张账号表（`settings/accounts-section.ts`）的
 * **同职第二处**：状态栏 chip 的账号菜单渲染同一个三态，而第一轮只改了设置那侧
 * ⇒ api-key 号在 chip 菜单里仍显示「已登录」，`KA6a` 那段文案只堵了一半。
 * 第二轮把 `accountRow` 里那段 `if (a.mode === "in-place") … else if (!a.loggedIn) …
 * else "已登录"` 整段换成 `accountStatusBadge(a)`，本文件那一整行**连键一起删**
 * ⇒ 今天 `account-chip.ts` 的 `loggedIn` 命中数是 **0**（不是被豁免成 0，是真的没有）。
 * 阴性对照实测（第二轮 `M1`）：只删这一行、`account-chip.ts` 一字不动 ⇒ 下面第 2 条当场红，
 * 报文逐字 `src/account-chip.ts: loggedIn 出现 1 次（登记 0）`，`offenders` 长度 1。
 *
 * ⇒ **登记表今天只剩 `src/accounts.ts` 一格。** 谁要往这张表加第二格，先回答一句：
 * 「这个新落点凭什么不能走 `isSelectable` / `accountStatusBadge`」——
 * 上面那两轮的教训是：同职两处必然漂，而漂的那一侧用户先看见。
 */
const ALLOWED: Record<string, Record<string, number | null>> = {
  "src/accounts.ts": { loggedIn: 1, authReady: null, authKind: null },
};

/** 被钉的三个字段名。 */
const FIELDS = ["loggedIn", "authReady", "authKind"] as const;

describe("KAY4 账号可用性只有一个出口", () => {
  const sources = productionTsFiles("src").map((s) => ({
    file: s.file,
    code: stripComments(s.text, "ts"),
  }));
  const byFile = new Map(sources.map((s) => [s.file, s.code]));

  it("抽取器自检：真的扫到了生产源码树，且登记表里的文件都还在", () => {
    // 文件数地板（形态照 `cc_bus_boundary_guard.rs` 那条 `>= 20`）：
    // 遍历坏了 / 路径推错了会得到一个空集合，而空集合上「零命中」恒真。
    // 08-24 实测 126 个（`src/generated/` 由 `productionTsFiles` 自己排掉）。
    expect(
      sources.length,
      `只扫到 ${sources.length} 个生产 .ts（08-24 实测 126）—— 遍历坏了，下面两条会零命中地绿`,
    ).toBeGreaterThanOrEqual(100);
    for (const f of Object.keys(ALLOWED)) {
      expect(byFile.has(f), `登记表点着 ${f}，但它不在扫到的清单里 —— 文件被搬/改名了`).toBe(true);
    }
    // 剥注释别剥过头：本守卫的全部承重都落在「剥完还剩代码」上。
    const acc = byFile.get("src/accounts.ts") ?? "";
    expect(acc, "剥过头了：accounts.ts 里连 isSelectable 都没剩下").toContain(
      "export function isSelectable",
    );
    // 而且**注释真的被剥掉了** —— 否则下面的次数全是散文。
    // `accounts.ts` 的注释里逐字出现过 `a.loggedIn`（讲 K-A1 换了哪一项）。
    expect(hits(acc, "loggedIn"), "注释没被剥掉：次数把散文也数进来了").toBeLessThan(4);
  });

  it("★ 生产段里没有第二处读 loggedIn / authReady / authKind", () => {
    const offenders: string[] = [];
    for (const { file, code } of sources) {
      const allow = ALLOWED[file];
      for (const field of FIELDS) {
        const n = hits(code, field);
        const cap = allow ? allow[field] : 0;
        if (cap === null) continue; // 登记为「不钉次数」
        const expected = cap ?? 0;
        if (n !== expected) offenders.push(`${file}: ${field} 出现 ${n} 次（登记 ${expected}）`);
      }
    }
    expect(
      offenders,
      "有生产文件直接读了账号的鉴权字段。可用性一律走 `accounts.ts::isSelectable`；\n" +
        "要显示登录态走 `accountStatusBadge`；要按 kind 分流的规则住 `acct_core::auth_ready`。\n" +
        "真要新开一处 ⇒ 把它写进本文件的 `ALLOWED` 并说明凭什么。\n" +
        `实得：\n${offenders.map((o) => `  ${o}`).join("\n")}`,
    ).toEqual([]);
  });

  it("★ 阳性对照：`isSelectable` 真的经 `authReady` 而不是裸 `loggedIn`", () => {
    // 上一条是「没有第二处」；这一条是「第一处确实在做那件事」——
    // 否则把 `isSelectable` 整个删掉，上一条也会绿（零命中的另一种到法）。
    const acc = byFile.get("src/accounts.ts") ?? "";
    expect(acc).toMatch(/export function isSelectable\(a: Account\): boolean \{[\s\S]*?authReady\(a\)/);
    expect(
      /export function isSelectable\(a: Account\): boolean \{[\s\S]{0,400}?a\.loggedIn/.test(acc),
      "`isSelectable` 里又出现了裸 `a.loggedIn` —— 那条回落该只住 `authReady()` 里",
    ).toBe(false);
  });
});
