// auto-e2e F-E3:`./error-toast` 的 e2e shim（测试 fixture）。真身 showActionFailureToast 操作
// DOM（headless node 无 document → 会抛）。这里换成把 toast 标题写进 $CCM_TOAST_LOG,既避免
// DOM 崩溃,又让套件能断言失败语义 toast（如"重启已中止"）。导出名与真身一致,否则 import 失败。
import { appendFileSync } from "node:fs";

const LOG = process.env.CCM_TOAST_LOG;

export function showActionFailureToast(title, _body, _opts) {
  if (LOG) appendFileSync(LOG, "TOAST " + String(title) + "\n");
}
export function bindErrorToast() {}
