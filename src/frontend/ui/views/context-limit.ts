/**
 * 上下文那一格界面这边剩下的两件：读设置里那张上限表（交给后端）· 模型名的展示归一化。**纯模块**（零 import，node 可测）。
 *
 * 上限、百分比、写好的字全由后端定（`history-facts` 的 `usage`：中转看见的请求 ＞ 设置里的上限表 ＞ 模型名带 `[1m]` ＞ 见过超过 200k 的一轮；
 * 判不出时只写用了多少），状态栏与监控板照抄同一份。
 */

/** 模型上限用户覆盖表：模型串**子串**（大小写不敏感）→ 上限 tokens。存 config.json `contextLimits`，随 `history-facts` 交给后端应用。 */
export type ContextLimitOverrides = Record<string, number>;

/** config.json `contextLimits` 那一格 ⇒ 覆盖表（读的唯一一份：交后端的那张与设置页都走它）。
 *  不是对象 ⇒ 空表；非正数 / 非数字的那几行丢掉（手改坏了的盘不许把表整个带歪）。 */
export function readContextLimits(raw: unknown): ContextLimitOverrides {
  const out: ContextLimitOverrides = {};
  if (raw && typeof raw === "object" && !Array.isArray(raw)) {
    for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
      if (k.trim() !== "" && typeof v === "number" && Number.isInteger(v) && v > 0) out[k] = v;
    }
  }
  return out;
}

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
