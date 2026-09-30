/**
 * 判据里的仓内住址一律是正斜杠形（`src/frontend/ui/x.ts`）：拿 `path.*` 拼出来的路径去和这种字面量比、或当表键之前，过这一道。
 *
 * 为什么：`path.join / resolve / relative` 在 Windows 上吐 `\`（还有 `src\common/copy-core/src/lib.rs` 这种混形），
 * 一比就「集合里多一个、少一个」—— 4.0.0 在 windows runner 上红的 vitest 大半是这一个病。
 * ⚠ 不看 `path.sep`、无条件把 `\` 换成 `/`：它在哪台机器上都是同一个函数，本机验过的就是 Windows 上跑的那一份。
 */
export function toPosix(p: string): string {
  return p.replaceAll("\\", "/");
}
