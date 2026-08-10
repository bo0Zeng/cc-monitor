/**
 * F03b：收件箱 overlay 的两条性质判据。
 *
 * # 为什么钉这两条，而不是「渲染出来长什么样」
 *
 * DOM 长什么样是 UI 细节，会随排版改；而这两条一旦退化，**后果是静默的**：
 *
 * 1. **`missingReason` 必须原样显示**（定框 C6）。它是后端给的**带身份**缺席原因
 *    （哪个 skill 的哪条前提没满足）。一旦有人把它折成「不可用」，
 *    skill 改版后用户看到的就是「按钮在、点了没反应、不知道为什么」—— 那正是 C6 禁的形状。
 *    ⇒ 判据扫源码：不许出现把它替换成固定文案的写法。
 *
 * 2. **前端不许做路径判断**。写面围栏的真相源只有 Rust 侧 `skill_host::resolve_editable`
 *    一处（三道：`canonicalize` 后集合判定 · Claude 数据保护 · 目标必须已存在）。
 *    前端若自己判一遍，就有了第二个住址 —— 而两个住址迟早分叉，
 *    且**前端那份必然是较弱的那份**（它拿不到真实文件系统）。
 *
 * ⚠ 它们都是**源码形态判据**，判不了运行时行为（同本仓其它约定型守卫一档）。
 * 比没有强，别读成证明。
 */
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "../test-support/repo-root.ts";

const SRC = readFileSync(`${REPO_ROOT}/src/views/inbox-view.ts`, "utf8");

/** 生产段：剥掉块注释与整行 `//`（免得头注里的说明被当成代码命中）。 */
function production(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .split("\n")
    .filter((l) => !l.trim().startsWith("//"))
    .join("\n");
}

describe("F03b 收件箱 overlay", () => {
  it("missing_reason 原样显示，不许折成固定文案（C6）", () => {
    const code = production(SRC);

    // 抽取器自检：它必须真的读了那个字段，否则本条整个空转。
    expect(
      code.includes("missing_reason"),
      "源码里找不到 `missing_reason` —— 字段改名了？改了就把本条一起改，别让它零命中地绿",
    ).toBe(true);

    // 病灶形态：把后端给的原因替换成一句自己编的话。
    // ⚠ 这里查的是**赋值给 UI 的那一侧**：`?? "…"` / `|| "…"` 落一个固定串就是折叠。
    const collapsed = [...code.matchAll(/missing_reason\s*(\?\?|\|\|)\s*"([^"]*)"/g)].map(
      (m) => m[2],
    );
    expect(
      collapsed.filter((s) => s.length > 0 && !s.includes("在场")),
      "`missing_reason` 被折成固定文案了：" +
        JSON.stringify(collapsed) +
        "\n\n它是后端给的**带身份**缺席原因（哪个 skill 的哪条前提没满足，定框 C6）。\n" +
        "折成「不可用」这类话之后，skill 改版时用户看到的就是「按钮在、点了没反应、\n" +
        "不知道为什么」—— 那正是 C6 禁的形状。\n" +
        "⇒ 只许在 `missing_reason === null`（在场）那一支填自己的话。",
    ).toEqual([]);
  });

  it("前端不许自己做路径判断（围栏的真相源只有 Rust 侧一处）", () => {
    const code = production(SRC);
    const forbidden = [
      "startsWith(",
      "includes(\"..\")",
      "normalize(",
      "resolve(",
      "canonicalize",
    ];
    const hits = forbidden.filter((f) => code.includes(f));
    expect(
      hits,
      "本文件出现了路径判断的痕迹：" +
        JSON.stringify(hits) +
        "\n\n写面围栏的真相源只有 Rust 侧 `skill_host::resolve_editable` 一处\n" +
        "（三道：路径 canonicalize 之后做集合判定 · 过 Claude 数据保护守卫 · 目标必须已存在）。\n" +
        "前端再判一遍就有了第二个住址 —— 两个住址迟早分叉，而**前端那份必然更弱**\n" +
        "（它拿不到真实文件系统，判不了符号链接）。\n" +
        "⇒ 前端的职责只是把后端算好的路径原样送回去。",
    ).toEqual([]);
  });
});
