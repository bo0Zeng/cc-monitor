/**
 * 测试夹具里的 user 记录补上后端解析时填的那一格成品（`userText`：谁说的 ＋ 要显示的正文）。
 *
 * 生产里这一格只由后端判（`agents/claudecode/text.rs::user_text`）；前端零实现。夹具里的 user 记录**只有三种**：
 * 全是工具结果的（⇒ 工具结果）· 带 `isCompactSummary` 字段的（⇒ 压缩摘要）· 人说的、不含注入噪声的
 * （⇒ 人）；后两种的正文是 text 块以 `\n` 拼起来再 trim。
 * 这句话由 Rust 那一侧拿真规则把语料过一遍钉住（`schema_tests.rs::the_ts_fixture_user_records_carry_no_injected_noise`）；
 * 要测别的来源的夹具显式写 `userText`。已有 `userText` 的原样不动。
 */
export function withUserText<T>(rec: T): T {
  const r = rec as { type?: unknown; userText?: unknown; isCompactSummary?: unknown; message?: { content?: unknown } };
  if (r === null || typeof r !== "object" || r.type !== "user" || r.userText !== undefined) return rec;
  const c = r.message?.content;
  const blocks = Array.isArray(c) ? (c as Array<{ type?: unknown; text?: unknown }>) : [];
  if (blocks.length > 0 && blocks.every((b) => b && b.type === "tool_result")) {
    return { ...(rec as object), userText: { speaker: { kind: "toolResult" }, text: "" } } as T;
  }
  let text = "";
  if (typeof c === "string") text = c;
  else text = blocks.filter((b) => b && b.type === "text" && typeof b.text === "string").map((b) => b.text as string).join("\n");
  const kind = r.isCompactSummary === true ? "compactSummary" : "human";
  return { ...(rec as object), userText: { speaker: { kind }, text: text.trim() } } as T;
}
