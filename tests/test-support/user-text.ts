/**
 * 〔RENDER2 · J10〕测试夹具里的 user 记录补上 monitor 解析时填的那一格成品（`userText`）。
 *
 * 生产里这一格只由 monitor 按 `search-core::user_text` 填；前端零实现。夹具**不含 CLI 注入噪声**，
 * 那时规则的输出就是「正文抽出来 trim」—— 本助手只做这一步（抽法同 `search_core::extract_text_blocks`：text 块以 `\n` 拼）。
 * 夹具不含噪声这件事由 Rust 那一侧钉（`tests/frontend/shell/messages_tests.rs` 「TS 夹具里的 user 记录都不含注入噪声」）；
 * 要测噪声的夹具显式写 `userText`，规则本身的判据住 search-core 的 `lib_tests`。已有 `userText` 的原样不动。
 */
export function withUserText<T>(rec: T): T {
  const r = rec as { type?: unknown; userText?: unknown; message?: { content?: unknown } };
  if (r === null || typeof r !== "object" || r.type !== "user" || r.userText !== undefined) return rec;
  const c = r.message?.content;
  let text = "";
  if (typeof c === "string") text = c;
  else if (Array.isArray(c)) {
    text = c
      .filter((b) => b && typeof b === "object" && (b as { type?: unknown }).type === "text")
      .map((b) => (b as { text?: unknown }).text)
      .filter((t): t is string => typeof t === "string")
      .join("\n");
  }
  return { ...(rec as object), userText: { clean: text.trim(), interrupt: false } } as T;
}
