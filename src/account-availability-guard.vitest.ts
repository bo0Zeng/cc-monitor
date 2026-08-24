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
 * 4. **它不判 `authReady()` 自己算得对不对。** 阳性对照钉的是「`isSelectable` 的**函数体内**
 *    真的调了 `authReady(a)`」；规则本身住 `acct_core::auth_ready`，由 Rust 侧那三条
 *    （`acct-core` 的金样互钉 + 两条生产者对拍）守。把 `authReady` 的函数体改错，本文件四条全绿。
 * 5. **它扫不到测试侧的第二份实现。** 扫描面（`productionTsFiles`）**按构造**排掉
 *    `.vitest.` / `.test.` —— 那一行正是「让判据读不到自己」的机制，代价是一份**手抄进
 *    测试文件里的** `isSelectable` 结构上看不见（K-A1 第四轮 `R4` 就是这一形：
 *    `src/settings/cc-bus-section.vitest.ts` 那份 `vi.mock` 替身当时已与真身语义相反）。
 *    ⇒ 那一维靠「测试侧不许手抄纯函数，要么 `vi.importActual` 要么不 mock 它」这条纪律，
 *    本文件不钉。
 *
 * # 阴性对照（这条守卫真的有牙吗）
 *
 * K-A1 `KAM5` 实测：往 `src/launch-menu.ts` 加一句 `const ok = a.loggedIn;`
 * ⇒ 本文件第 2 条当场红（实测输出贴在件计划 `§3`）。撤掉即绿。
 *
 * # ⚠ 第四轮（`R2`）：阳性对照**曾经是半空真**，两处收紧各治一把刀
 *
 * 第三轮的阳性对照逐字是
 * `expect(acc).toMatch(/export function isSelectable\(a: Account\): boolean \{[\s\S]*?authReady\(a\)/)`
 * —— `[\s\S]*?` **无界**，只保证「开花括号**之后某处**有 `authReady(a)`」；再配上当时
 * `ALLOWED` 把 `authReady`/`authKind` 登记成 `null`（不钉次数），D 阶段审计**一刀就绕过去了**。
 * 第四轮逐字复现了那一刀（读数在件计划 `§3c`）：
 * 把 `authReady(a)` 挪进一个**定义在 `isSelectable` 之后**的包装、并让那个包装第二次直接读
 * `a.authReady` ⇒ 本文件 **3 passed**、vitest 全量 **117 文件 / 1467 passed**，`tsc` **0 错**，全绿。
 *
 * ⇒ 收紧两处，**一处治一把刀**（收紧后同一刀实测，读数同在 `§3c`）：
 *   · 阳性对照的窗口从「文件尾」收成「`isSelectable` 的函数体」（切到第一个行首 `}`）
 *     ⇒ 刀①（挪进包装）**1 红 / 4**；
 *   · `ALLOWED` 每一格钉死次数、`null` 那一档连类型一起删，并新增一条**登记表自检**
 *     ⇒ 刀①+②（再加第二处读 `a.authReady`）**2 红 / 4**，报文逐字
 *     `src/accounts.ts: authReady 出现 6 次（登记 5）`。
 * ⚠ 登记表自检为什么要在**运行时**再判一次（类型已经是 `number` 了）：
 * `authReady: null as unknown as number` 这种写法 `tsc` **不红**（实测 0 错），
 * 而它就是「把判据放宽回去」最省事的走法。实测那一刀 ⇒ 本文件 **2 红 / 4**。
 */
import { describe, it, expect } from "vitest";

import { productionTsFiles } from "./test-support/production-sources.ts";
import { stripComments } from "./test-support/strip-comments.ts";

/** 出现次数（**去注释后**的代码里）。用正则而不是 `.includes`：要的是次数，不是有无。 */
function hits(code: string, ident: string): number {
  return code.match(new RegExp(`\\b${ident}\\b`, "g"))?.length ?? 0;
}

/**
 * ★ **登记表：这三个字段今天各自允许住在哪里，各自允许出现几次。**
 *
 * 每一格都是**钉死的次数**（加一处、少一处都红）。
 *
 * ⚠ **「允许出现但不钉次数」这一档（第三轮写成 `null`）第四轮整个删掉了，连类型一起。**
 * 那不是洁癖：`authReady`/`authKind` 登记成 `null` 时，`src/accounts.ts` 文件**内部**
 * 可以长出第二条直接读 `a.authReady` 的路而本文件全绿 —— D 阶段审计一刀就走通了
 * （复现读数在件计划 `§3c`）。留着那个档，等于给「再开一条」留了一扇不响的门。
 * ⇒ 谁要改这三个数，先回答「这一处凭什么不能走 `isSelectable` / `accountStatusBadge`」；
 * **把数字调大就是在放宽判据**，不是在修测试。
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
const ALLOWED: Record<string, Record<string, number>> = {
  // 08-24 第四轮实测（`stripComments` 之后）：`loggedIn` 1 处（`authReady()` 里那句回落）·
  // `authReady` 5 处（函数名 1 + 那句回落里的 `a.authReady` 1 + 三个消费点
  // `accountStatusBadge` / `accountLoginActionLabel` / `isSelectable` 各 1）·
  // `authKind` 2 处（`accountStatusBadge` 与 `accountLoginActionLabel` 各按 kind 分流一次）。
  "src/accounts.ts": { loggedIn: 1, authReady: 5, authKind: 2 },
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
        // 没登记的文件 ⇒ 一次都不许出现；登记了但没列这个字段 ⇒ 同样是 0。
        const expected = allow?.[field] ?? 0;
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

  it("★ 登记表自检：每一格都钉死次数（没有「允许出现但不钉次数」的格子）", () => {
    // 第四轮加的一条。它守的不是生产代码，是**上面那张表自己**：
    // 第三轮 `authReady`/`authKind` 登记成 `null`（不钉次数），于是 `accounts.ts` 文件内部
    // 可以长出第二处直接读 `a.authReady` 的路而全绿（D 阶段审计实测走通）。
    // ⇒ 放宽这张表最省事的走法就是「把某一格改回不钉次数」，本条把那条走法关上：
    //   要放宽只能**明写一个更大的数字**，那是一个看得见的动作，会出现在 diff 里。
    const cells = Object.entries(ALLOWED).flatMap(([file, fields]) =>
      Object.entries(fields).map(([field, cap]) => ({ file, field, cap })),
    );
    expect(cells.length, "登记表空了 —— 下面两条会零命中地绿").toBeGreaterThanOrEqual(3);
    const loose = cells.filter((c) => typeof c.cap !== "number" || !Number.isInteger(c.cap));
    expect(
      loose.map((c) => `${c.file}.${c.field}`),
      "登记表里出现了不钉次数的格子 —— 那等于给「再开一条读鉴权字段的路」留一扇不响的门",
    ).toEqual([]);
  });

  it("★ 阳性对照：`isSelectable` 的**函数体内**真的经 `authReady` 而不是裸 `loggedIn`", () => {
    // 上一条是「没有第二处」；这一条是「第一处确实在做那件事」——
    // 否则把 `isSelectable` 整个删掉，上一条也会绿（零命中的另一种到法）。
    //
    // ⚠ **窗口必须有界到函数体。** 第三轮这里写的是
    // `/export function isSelectable\(a: Account\): boolean \{[\s\S]*?authReady\(a\)/` ——
    // `[\s\S]*?` 无界，只保证「开花括号**之后某处**有 `authReady(a)`」。
    // D 阶段审计一刀就绕过去了：把 `authReady(a)` 挪进一个**定义在 `isSelectable` 之后**的
    // 语义等价包装 ⇒ 本文件当时 3 passed、vitest 全量 1467 passed，全绿（第四轮复现过，
    // 读数在件计划 `§3c`）。⇒ 改成先切出函数体（到第一个**行首** `}` 为止）再匹配。
    const acc = byFile.get("src/accounts.ts") ?? "";
    const m = /export function isSelectable\(a: Account\): boolean \{\n([\s\S]*?)\n\}/.exec(acc);
    expect(
      m,
      "切不出 `isSelectable` 的函数体（签名或大括号形状变了）—— 下面三条会零命中地绿",
    ).not.toBeNull();
    const body = m?.[1] ?? "";
    // 反空真：闭合锚点若滑过了 `isSelectable` 的尾巴，窗口就又变无界了。
    expect(body, "切出来的「函数体」跨进了下一个函数 —— 窗口又变无界了").not.toContain(
      "export function",
    );
    // `\b` 不能省：`toContain("authReady(a)")` 会被 `xauthReady(a)` 这种改名蒙过去。
    expect(
      /\bauthReady\(a\)/.test(body),
      "`isSelectable` 的函数体里没有 `authReady(a)` —— 鉴权判据被挪到别处去了（哪怕挪进一个语义等价的包装也不行：那就是第二条路）",
    ).toBe(true);
    expect(
      /\ba\.loggedIn\b/.test(body),
      "`isSelectable` 里又出现了裸 `a.loggedIn` —— 那条回落该只住 `authReady()` 里",
    ).toBe(false);
  });
});
