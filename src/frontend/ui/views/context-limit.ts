/**
 * 上下文那一格界面这边剩下的一件：模型名的展示归一化。**纯模块**（零 import，node 可测）。
 *
 * 上限、百分比、写好的字全由后端定（`history-facts` 的 `usage`：中转看见的请求 ＞ 那台 `prefs.json` 的上限表 ＞ 模型名带 `[1m]` ＞ 见过超过 200k 的一轮；
 * 判不出时只写用了多少），状态栏与监控板照抄同一份。
 */

/** 展示用归一化：剥 `[1m]` / `-fast` / 尾部 8 位日期快照，留干净 model 名。 */
export function normalizeModel(id: string | null | undefined): string {
  if (!id) return "unknown";
  return (
    id
      .replace(/\[1m\]/gi, "")
      .replace(/-fast\b/gi, "")
      .replace(/-\d{8}$/, "")
      .trim() || "unknown"
  );
}
