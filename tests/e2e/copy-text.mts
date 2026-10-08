/**
 * e2e 脚本按文案键取那一句（与界面同一个取文口 `copyText`），脚本里不钉原文：改表值不撞，改键 / 删键撞。
 *
 * 用法：`tsx tests/e2e/copy-text.mts <键> [名=值 …]` ⇒ stdout 是插好值的那一句（不带换行）。
 */
import { copyText, type CopyKey } from "../../src/frontend/ui/copy-table.ts";

const [key, ...pairs] = process.argv.slice(2);
if (!key) {
  process.stderr.write("用法：copy-text.mts <键> [名=值 …]\n");
  process.exit(2);
}
const args: Record<string, string> = {};
for (const p of pairs) {
  const i = p.indexOf("=");
  if (i <= 0) {
    process.stderr.write(`参数不是 名=值：${p}\n`);
    process.exit(2);
  }
  args[p.slice(0, i)] = p.slice(i + 1);
}
process.stdout.write(copyText(key as CopyKey, args));
