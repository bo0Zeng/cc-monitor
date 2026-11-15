/**
 * 截图台架把一条会话的预读演成「一直没读完」（`real/pool.mjs::primeHoldSh`）：宣告那一帧与它折好的那一格照过，之后那条会话的格一律不往壳那一跳交。
 *
 * 起因：壳收的是折好的成品（attach 行要了 `folded`：每帧会话帧之后紧跟一帧 `session_seats`，壳那一格照它换）。
 * 只压 `session_status` 不压 `session_seats` ⇒ 读完之后那一格照样把判出来的那一态交到壳，界面接上时拿到的就不是「读记录中」。
 *
 * 人群：本机那一台的流经台架那一跳（沙箱开场 sh 里那一条管道）。
 * 买不到：后端真发哪几帧、什么先后不在这里造（几行样帧，字段只留认帧要的那几格）。
 */
import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";

// 池子是不带类型的 .mjs（vite 插件那一侧直接跑它）：按路径取那一个函数。
const POOL = "./real/pool.mjs";
const { primeHoldSh } = (await import(POOL)) as { primeHoldSh: (held: string[]) => string };

const HELD = "019a0000-0000-7000-8000-00000000c0de";
const OTHER = "5eed0001-0000-4000-8000-000000000001";

const seats = (sid: string, word: string): string => JSON.stringify({ kind: "session_seats", seats: [{ sid, seat: { live: { face: { key: word } } } }], listed: false });

/** 一条流上的几行，照后端的先后。 */
const STREAM = [
  JSON.stringify({ kind: "session_added", sid: HELD, face: { key: "宣告" } }),
  seats(HELD, "宣告"),
  JSON.stringify({ kind: "session_added", sid: OTHER, face: { key: "宣告" } }),
  seats(OTHER, "宣告"),
  JSON.stringify({ kind: "session_primed", sid: HELD }),
  JSON.stringify({ kind: "session_status", sid: HELD, face: { key: "读完" } }),
  seats(HELD, "读完"),
  JSON.stringify({ kind: "session_status", sid: OTHER, face: { key: "读完" } }),
  seats(OTHER, "读完"),
];

function through(held: string[]): string[] {
  const out = execFileSync("sh", ["-c", `cat | ${primeHoldSh(held)}`], { input: STREAM.join("\n") + "\n", encoding: "utf8" });
  return out.split("\n").filter((l) => l !== "");
}

describe.skipIf(process.platform === "win32")("预读演成一直没读完", () => {
  it("★ 压住的那条：宣告那一帧与它那一格照过，之后的状态 · 读完了 · 折好的那一格都不过；别的会话原样过", () => {
    expect(through([HELD])).toEqual([STREAM[0], STREAM[1], STREAM[2], STREAM[3], STREAM[7], STREAM[8]]);
  });

  it("sid 形状不对 ⇒ 当场拒（拼进 sh 的只许十六进制与连字符）", () => {
    expect(() => primeHoldSh(["x'; rm -rf /"])).toThrow();
  });
});
