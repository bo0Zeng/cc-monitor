/**
 * 〔C4a〕判据里假扮 monitor 那一跳（包装层 `chan_call`）的几样小工具。
 *
 * 生产上 `chan.call` ⇒ `commands.chan_call` ⇒ `invoke("chan_call", { origin, op, payload, leftMs })`，
 * monitor 回原样字节（`ArrayBuffer`）或 `{ err, body }`（`wire::err_to_wire` 的线上形状）。
 * 各判据 mock 的是 `@tauri-apps/api/core` 的 `invoke` —— 本文件只帮它们造「那一跳会回什么」。
 */

/** `chan_call` 的实参（判据按 `op` 分派）。 */
export interface ChanCallArgs {
  origin: string;
  op: string;
  payload: number[];
  leftMs: number;
}

/** 这一发 `invoke` 是不是通道那一跳、问的是不是这条帧命令。 */
export function isChanCall(cmd: string, args: unknown, op: string): args is ChanCallArgs {
  return cmd === "chan_call" && (args as ChanCallArgs | undefined)?.op === op;
}

/** 一个 JSON 值 ⇒ monitor 交回的原样字节。 */
export function chanReply(v: unknown): ArrayBuffer {
  const u = new TextEncoder().encode(JSON.stringify(v));
  return u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength) as ArrayBuffer;
}

/** 按行那一族（`{"lines": [...]}`）的应答。 */
export function linesReply(rows: unknown[]): ArrayBuffer {
  return chanReply({ lines: rows.map((r) => (typeof r === "string" ? r : JSON.stringify(r))) });
}

/** 「那台没有控制通道」（`Hop{1 open, NotSent, Unreachable}`）—— monitor 那一跳会拒的那个值。 */
export const NO_CHANNEL = {
  err: { Hop: { idx: 1, tag: "open", reach: "NotSent", why: "Unreachable" } },
  body: [] as number[],
};

/** 一发 `chan_call` 的请求体解回 JSON。 */
export function chanArgsJson(args: ChanCallArgs): unknown {
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(args.payload)));
}
