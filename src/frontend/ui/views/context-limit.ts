/**
 * 上下文占用的排版那一半。**纯模块**（零 import，node 可测）。
 *
 * 上限由后端定（`history-facts` 的 `usage.limit`：中转看见的请求 ＞ 设置里的上限表 ＞ 模型名带 `[1m]` ＞ 见过超过 200k 的一轮；
 * 判不出时 `limitFrom` 是 `assumed`，界面不算百分比），状态栏与监控板读同一个数；这里只把「用了多少 ÷ 上限」排成百分比，并读设置里那张上限表交给后端。
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

/** 用了多少 token 的短写（上限判不出时只写它）：`350k` · `1.2M` · `800`。 */
export function contextTokensText(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}k`;
  return String(tokens);
}

/** 最新一轮 prompt 占上限的百分比（上限是后端给的）。没有上限 ⇒ `null`；永远不超过 100。 */
export function contextPercentOf(promptTokens: number, limit: number | null): number | null {
  if (limit === null || limit <= 0) return null;
  return Math.min(100, (promptTokens / limit) * 100);
}
