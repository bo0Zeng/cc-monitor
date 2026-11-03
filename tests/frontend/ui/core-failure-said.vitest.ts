/**
 * **核心写的失败那几句，两端出口零取**。
 *
 * 一问没答成（通道那一跳 · 对方版本 · ↗ 浮层那一张）给人看的那一句只住核心：通道那一份（`comms/inward/chan/said.rs`）·
 * 因对方版本说不成的那几句（`common/copy-core/src/peer.rs`）· ↗ 浮层那一张（`common/face-core/src/front.rs`）。
 * 桌面界面与手机都照核心交来的成品摆，不按结局自己取那几把键。
 *
 * - 键：上面三份里 `copy_text("…")` 的全集（现扫，不手写）。
 * - 出口：界面生产 TS（`src` 下）＋ 手机各模块 `src/main` 下的 Kotlin（现扫）；剥注释后 `copyText("<键>"` 零处。
 * - 例外逐条写理由（[`NOT_FAILURES`] · [`ONE_MOUTH`]）。
 *
 * 买不到：键先存进变量、拼出来再取的认不出；同一句换一把键另写的认不出（「同一句只住一个键」那一条管）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { productionKtFiles, productionTsFiles } from "../../test-support/production-sources.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

/** 核心写失败那一句的三份。 */
const HOMES = ["src/comms/inward/chan/said.rs", "src/common/copy-core/src/peer.rs", "src/common/face-core/src/front.rs"] as const;

/** 三份里取、却不是失败那一句的键：出口照常取。 */
const NOT_FAILURES: Record<string, string> = {
  "detail.label.local": "本机的称呼（名字，不是失败那一句）",
  "control.machine.peer": "不知道名字时称呼那台的词",
  "terminalFront.unavailable.short": "↗ 此刻点不点得了那一句（能不能点是另一件事，不在这张表里）",
  "reason.io.unknown": "「原因不明」这一格是通用的原因词，↗ 出错那一张借来当正文；别处照常用",
};

/** 界面自己知道的事、两端各一个取句口（判定登记表里登成镜像：取的是表里同一把键）：键 ⇒ 允许的那几个文件。 */
const ONE_MOUTH: Record<string, readonly string[]> = {
  "peerVersion.said.unreadable": ["src/frontend/ui/ipc/chan-caller.ts", "src/mobile/app/src/main/kotlin/com/ccmonitor/mobile/link/Problem.kt"],
};

function coreKeys(): Set<string> {
  const keys = new Set<string>();
  for (const f of HOMES) {
    const code = stripComments(readFileSync(resolve(REPO_ROOT, f), "utf8"), "rust");
    for (const m of code.matchAll(/copy_text\(\s*"([^"]+)"/g)) keys.add(m[1]);
  }
  return keys;
}

const keys = coreKeys();
const exits = [
  ...productionTsFiles().map((s) => ({ file: s.file, code: stripComments(s.text, "ts") })),
  ...productionKtFiles().map((s) => ({ file: s.file, code: stripComments(s.text, "kotlin") })),
];

/** 出口里取了某把键的每一处：`文件 · 键`。 */
function taken(key: string): string[] {
  const needle = `copyText("${key}"`;
  return exits.flatMap(({ file, code }) => Array(code.split(needle).length - 1).fill(`${file} · ${key}`) as string[]);
}

describe("核心写的失败那几句，两端出口零取", () => {
  it("正控：键与出口都扫到了", () => {
    expect(keys.has("rsChanSaid.hop.overrun"), "通道那一句").toBe(true);
    expect(keys.has("front.body.timeout"), "↗ 浮层那一句").toBe(true);
    expect(keys.has("peerVersion.said.unreadable"), "认不出那一句").toBe(true);
    for (const k of [...Object.keys(NOT_FAILURES), ...Object.keys(ONE_MOUTH)]) expect(keys.has(k), `例外 ${k} 还在核心里`).toBe(true);
    expect(exits.some((e) => e.file.endsWith("/link/Problem.kt")), "手机人群").toBe(true);
    expect(exits.some((e) => e.file === "src/frontend/ui/front-result.ts"), "界面人群").toBe(true);
  });

  it("★ 出口不取核心写的那几把键", () => {
    const hits = [...keys].filter((k) => !(k in NOT_FAILURES) && !(k in ONE_MOUTH)).flatMap(taken);
    expect(hits).toEqual([]);
  });

  it("★ 认不出那一句两端各恰好一个取句口", () => {
    for (const [k, files] of Object.entries(ONE_MOUTH)) {
      expect(taken(k), k).toEqual(files.map((f) => `${f} · ${k}`));
    }
  });
});
