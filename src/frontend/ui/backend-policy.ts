// 每台机一份后端策略：今天只有一条「monitor 退出时是否结束它」。
//
// 值住后端所在那台机器上（后端自己的状态文件，只有后端写）；读、改都经后端命令
//   （`backend_exit_policy` / `set_backend_exit_policy` → 后端 `exit-policy-read` / `exit-policy-set`），monitor 这边不存第二份。
// 「不结束」是不是「继续跑」看后端有没有真脱离；那几档的话（含「继续跑，无人监护」）由后端出成品 `said`
//   （`control/exit_policy.rs::said`），「健康」那一格由 `backend_policy.rs::health_face` 出成品，`settings/backend-section.ts` 只排版。
// 文案住文案表（`src/shared/copy/table.json` 的 `backendPolicy.*`）；这里的导出名照旧，值从表里取。

/**
 * 本机在 origin 这套命名里的名字。
 *
 * ⚠ **跨语言常量**：Rust 侧是 `inbound_client::LOCAL_ORIGIN`。两边漂了**不会报错** ——
 * 本机开关会去操作一个谁都没登记过的 origin，设了没反应且不报错。
 * 由 Rust 侧一条判据逐字对拍（`the_local_origin_is_the_same_string_on_both_sides`）。
 */
export const LOCAL_ORIGIN = "<local>";
