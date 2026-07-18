/**
 * F88b（#52）：模型 context 上限表 + 模型串归一化。**纯模块**（零 import，node 可测）。
 *
 * 只做「上限」（给 context 占用% 用）；**不做费用/$**（用户 2026-07-17 拍板只显 token，F88c 砍掉）。
 *
 * 上限现实：Claude 标准 context = **200k**；**`[1m]` 后缀变体 = 1M**（本项目自己的模型就是
 * `claude-opus-4-8[1m]`）。故两档：含 `[1m]` → 1M；其余已知 Claude 家族 → 200k；**未知 → null → UI 显 `?`**
 * （宁标未知，不显错的 %）。硬编码默认；用户可覆盖表 = 后续项（先内置默认够用）。
 */

/** 模型上限用户覆盖表：模型串**子串**（大小写不敏感）→ 上限 tokens。让用户为**标准模型串**
 *  （无本项目 `[1m]` 标记、但实际开了 1M beta context 的模型，如别的机器上的 `claude-sonnet-4-5-…`）
 *  纠正上限，避免被默认 200k 除出**误报 ctx≥80%**。存 config.json `contextLimits` 字段。纯模块——由调用方注入。 */
export type ContextLimitOverrides = Record<string, number>;

/** context 占用%用的模型上限（tokens）。未知返 null → 调用方显 `?` 不显错 %。`overrides` 优先（子串匹配）。 */
export function contextLimit(
  model: string | null | undefined,
  overrides?: ContextLimitOverrides,
): number | null {
  if (!model) return null;
  const m = model.toLowerCase();
  // 业务二审 gap#2：用户覆盖优先——纠正「标准 1M 模型串在默认表里被当 200k → 误报预警」。
  if (overrides) {
    for (const [sub, lim] of Object.entries(overrides)) {
      if (sub && lim > 0 && m.includes(sub.toLowerCase())) return lim;
    }
  }
  if (m.includes("[1m]")) return 1_000_000;
  // 已知 Claude 家族（opus/sonnet/haiku/fable/3.x/4.x）标准上限 200k。
  if (/claude-(opus|sonnet|haiku|fable|\d)/.test(m)) return 200_000;
  return null;
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

/** 某会话最新一轮 prompt 的 context 占用近似（input+cache ÷ 上限）。上限未知 → null。`overrides` 见 contextLimit。 */
export function contextPercent(
  model: string | null | undefined,
  latestPromptTokens: number,
  overrides?: ContextLimitOverrides,
): number | null {
  const lim = contextLimit(model, overrides);
  if (lim == null || lim <= 0) return null;
  return (latestPromptTokens / lim) * 100;
}
