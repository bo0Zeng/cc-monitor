/**
 * ↗ 浮层那一张由 `face_core::front` 写好交来（壳的回包 · 本机后端 `session-window` 的 `face`，A16-1-8），出口照摆、不各自判：
 * - ↗ 那几处出口（`front-result.ts` · 调它的几处）零出现壳的窗口结局 · 会话在哪的原因的词，与三个结局类型的名字；
 * - 给不给［在终端里打开］不由出口照会话的 tmux 那一格自己判（`session-window` 带 `tmux`，原样交壳）。
 * 正控：同一把尺在 `face-core/src/front.rs` 量得到那几把键（没量到 ⇒ 本条在空转）。
 * 同 `exit-said-acts.vitest.ts` 的做法；只读一张写死的文件名单，不遍历目录。注释照扫（「零出现」就是零出现）。
 */
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = resolve(__dirname, "../../..");

/** ↗ 的出口：排版 · 问的那一条 · 浮层 · 两个入口（主窗口 tab · 记录窗口顶栏）。 */
const EXITS = ["front-result.ts", "tab-session-actions.ts", "front-pop.ts", "tabs.ts", "entry-viewer.ts"].map((f) => `src/frontend/ui/${f}`);

function hits(files: readonly string[], needles: readonly string[]): string[] {
  return files.flatMap((f) => {
    const code = readFileSync(join(ROOT, f), "utf8");
    return needles.filter((n) => code.includes(n)).map((n) => `${f} · ${n}`);
  });
}

describe("↗ 浮层：出口照摆写好的那一张", () => {
  it("★ 出口不按壳的窗口结局 · session-window 的原因挑字", () => {
    // 结局与原因的词里不与别的词撞的那几个（`refused` · `unclear` · `elsewhere` · `unreadable` 别处另有义项，不当针）。
    const needles = [
      '"several"',
      '"switched-window"',
      '"window-gone"',
      '"no-window"',
      '"desktop-wont-switch"',
      '"detached"',
      '"no-terminal"',
      '"not-ssh"',
      '"mismatch"',
      '"not-live"',
      "FrontOutcome",
      "SeveralWhy",
      "TabMiss",
    ] as const;
    expect(hits(EXITS, needles)).toEqual([]);
    const be = readFileSync(join(ROOT, "src/common/face-core/src/front.rs"), "utf8");
    for (const k of ["front.title.unsure", "front.body.severalUnregistered", "front.title.gone", "front.body.detached", "front.body.notSsh"]) {
      // 按调用认、不按排版：带参数的那几处 rustfmt 折成多行（`copy_text(` 换行再写键）。
      expect(be, `正控：${k} 由 face_core::front 写`).toMatch(new RegExp(`copy_text\\(\\s*"${k.replace(/\./g, "\\.")}"`));
    }
  });

  it("★ 给不给［在终端里打开］不由出口照会话的 tmux 那一格判（session-window 带来、原样交壳）", () => {
    expect(hits(EXITS, ["inTmux("])).toEqual([]);
    expect(readFileSync(join(ROOT, "src/frontend/ui/tab-session-actions.ts"), "utf8"), "正控：tmux 那一格原样交壳").toContain("tmux: placed.tmux");
  });
});
