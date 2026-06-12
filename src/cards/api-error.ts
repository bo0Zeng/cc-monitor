/**
 * issue #21：API 报错的**显眼**渲染。两种真实形态（实测 163 个 session 调研）：
 *
 * 1. `assistant` + `isApiErrorMessage:true` —— 重试耗尽/不可重试的**最终失败**，
 *    CLI 合成的 assistant 消息（model="<synthetic>"，文本在 content[0].text，多以
 *    "API Error: " 开头）。此前被当普通 Claude 回复渲染 → 用户误以为还在跑。
 *    → 红色报错卡 `buildApiErrorCard`。
 * 2. `system` + `subtype:"api_error"` —— 单次调用失败、CLI **将要重试**的中间态
 *    （带 retryAttempt/maxRetries）。此前 system 一律 skip → 完全不可见。
 *    → 细条提示 `buildApiRetryCard`。
 *
 * error 对象 shape 随 CLI 版本变化（新版有现成 `.formatted` 一行文案；旧版嵌套
 * error.error.message），全部防御性取字段，取不到就降级到通用文案。
 *
 * **零 import 刻意**（同 diff.ts DOM-free 上半惯例）：api-error.test.ts 用 node
 * 直跑本模块，无扩展名 import 会解析失败。时间戳由调用方格式化好传入（timeLabel）。
 */

/** 形态 1：最终失败 → 红色报错卡（kind:"card"，不进工具组）。 */
export function buildApiErrorCard(args: {
  /** 已格式化的展示时间（调用方 formatTimestampShort 的结果） */
  timeLabel: string;
  text: string;
  category?: string;
  status?: number;
}): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-api-error";

  const head = document.createElement("div");
  head.className = "api-error-head";
  const icon = document.createElement("span");
  icon.className = "api-error-icon";
  icon.textContent = "⛔";
  head.appendChild(icon);
  const label = document.createElement("span");
  label.className = "api-error-label";
  label.textContent = "API 错误 — 本轮已中止";
  head.appendChild(label);
  if (args.category) {
    const chip = document.createElement("span");
    chip.className = "api-error-chip";
    chip.textContent = args.status ? `${args.category} · ${args.status}` : args.category;
    head.appendChild(chip);
  }
  const ts = document.createElement("span");
  ts.className = "api-error-ts";
  ts.textContent = args.timeLabel;
  head.appendChild(ts);
  card.appendChild(head);

  const body = document.createElement("div");
  body.className = "api-error-body";
  body.textContent = args.text || "(无错误详情)";
  card.appendChild(body);
  return card;
}

/** 形态 2：重试中间态 → 单行细条（可见但不喧宾夺主）。 */
export function buildApiRetryCard(args: {
  /** 已格式化的展示时间（调用方 formatTimestampShort 的结果） */
  timeLabel: string;
  retryAttempt?: number;
  maxRetries?: number;
  error?: unknown;
}): HTMLElement {
  const line = document.createElement("div");
  line.className = "card card-api-retry";

  // typeof 而非 !== undefined：serde 把 Option::None 序列化成显式 null，
  // null 会穿过 undefined 判定渲染出"重试 null/null"。
  const retry =
    typeof args.retryAttempt === "number" && typeof args.maxRetries === "number"
      ? ` · 重试 ${args.retryAttempt}/${args.maxRetries}`
      : "";
  line.textContent = `⚠ API 调用失败：${describeRetryError(args.error)}${retry} · ${args.timeLabel}`;
  return line;
}

/**
 * 从两种 shape 的 error 对象里挤一行人类可读文案。
 * export 供 api-error.test.ts 直测（双 shape 随 CLI 版本漂移，纯逻辑值得锁）。
 */
export function describeRetryError(error: unknown): string {
  if (!error || typeof error !== "object") return "网络/服务异常";
  const e = error as {
    formatted?: unknown;
    status?: unknown;
    connection?: { code?: unknown; message?: unknown } | null;
    error?: { error?: { message?: unknown } } | null;
  };
  // 新 shape（CLI ≥v2.1.156）：现成一行文案
  if (typeof e.formatted === "string" && e.formatted) return e.formatted;
  // 新 shape 连接错误：无 status，connection 非空
  if (e.connection && typeof e.connection === "object") {
    const code = typeof e.connection.code === "string" ? e.connection.code : "";
    const msg = typeof e.connection.message === "string" ? e.connection.message : "";
    const s = [code, msg].filter(Boolean).join(" ");
    if (s) return s;
  }
  // 旧 shape（≤v2.1.150）：status + 嵌套 message
  const nestedMsg = e.error?.error?.message;
  const status = typeof e.status === "number" ? String(e.status) : "";
  const msg = typeof nestedMsg === "string" ? nestedMsg : "";
  const s = [status, msg].filter(Boolean).join(" ");
  return s || "网络/服务异常";
}
