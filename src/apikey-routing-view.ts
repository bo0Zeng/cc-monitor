/**
 * 〔FE1 · 第四波 4D〕`apikey_routing_for` 的**线上形状** —— 零 import 的叶子。
 *
 * 守的要求：`设计/01 §5` D1「一个判定只有一个家」（类型住被依赖的一侧）。它是 `ipc/commands.ts` 那条命令的返回类型；
 * 先前住 `accounts.ts` ⇒ `ipc/commands.ts` 为一个类型回头 import 账号域，
 * `accounts · accounts-decode · config · history-reads · ipc/chan · ipc/chan-caller · ipc/commands` 七个模块的类型环
 * 就靠这一条边闭合（审计 B §4）。搬到这里之后，通信层只依赖一个什么都不 import 的文件。
 *
 * ⚠ 仍是**手写镜像**（Rust 那份是 monitor 的 `lib.rs::ApikeyRouting`），字段名手动同步、今天没有判据对拍 ——
 * 如实记，别读成有人守。apikey 视图那一块在 US1 手里（上游选择只留后端一份），它改线上形状时改这一处。
 */

/**
 * `K-H2b` `KH2B7` 的**产出方**：问后端「这几个**本机** configDir 在 apikey 表里有没有行（走不走 apikey 端点改写）」。
 *
 * # 它为什么是一条只答本机的命令（而不是账号列表上的两个字段）
 *
 * 中转是**每台机器自己的一个进程**，注入的又是回环地址（自指）⇒ 「本机这台的中转
 * 在不在跑」这个问题，本机这一侧**在结构上答不了远端那台**。往账号列表里加字段，
 * 就是让远端那些行也带上两个这一侧答不出来的值。
 * 命令面的登记（`apikey.routing`，`NaturallyAsymmetric`）写着同一条理由。
 *
 * ★〔第四拍〕**取数那一跳接上了**：走包装层 `commands.apikey_routing_for`。
 * ⚠ 经过如实记：第三拍它退回过一次 —— 注册一条命令会同时动两个钉死计数
 * （`parity_ledger.rs` 5 个数 + `tests/ipc/commands.vitest.ts` 两处 `144`），
 * 而后者当时不在写区。**那两个数是联动的**：注册了不调 ⇒ 前一个红；调了没注册 ⇒ 编不过。
 *
 * ⚠ **两个字段各自的射程，别读宽**：`routed` 说的是「apikey 表里有这一行」，
 * **不是**「那把 key 能用」；`running` 说的是「我们起过它而且没停过」，
 * **不是**「那个口上真有人听」。
 */
export interface ApikeyRoutingView {
  /** 传进去的那些 configDir 里，apikey 表里**有对应行**的那几个（原样回）。 */
  routed: string[];
  /** 本机中转在不在跑。 */
  running: boolean;
}
