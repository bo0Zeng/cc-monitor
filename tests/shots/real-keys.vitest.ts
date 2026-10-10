/**
 * 截图场景按键走真按键（CDP `Input.dispatchKeyEvent`），不在页里合成 `KeyboardEvent`。
 *
 * 为什么有这一格：页里 `dispatchEvent(new KeyboardEvent(...))` 是不受信任的合成事件 —— 浏览器不因它把焦点算成「键盘来的」，
 * `:focus-visible` 不亮；按钮上的 Enter 不触发点击；字不进框。截图于是证明不了样式表里焦点环那一条，
 * 场景里还得照着样式表手描一圈（标签页分组第四批 10-09）。改成场景里的 `key()` 经页上的绑定（`__shotsKey`）
 * 请截图工具从调试口发真按键，与性能台架同一条路（`cdp.mjs` 的 `Page.key`）。
 *
 * 人群：`tests/shots/scenes/` 下全部场景源码（新加一份场景也自动进来）。
 * 买不到：读的是源码文本；绕着写（把构造器存进别名再 new）认不出。
 */
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, it, expect } from "vitest";

const dir = path.join(__dirname, "scenes");
const files = readdirSync(dir).filter((n) => n.endsWith(".ts"));

describe("截图场景的按键是真按键", () => {
  it("场景里不合成 KeyboardEvent（按键一律走 helpers 的 key()）", () => {
    const hits: string[] = [];
    for (const f of files) {
      readFileSync(path.join(dir, f), "utf8")
        .split("\n")
        .forEach((l, i) => {
          if (/new\s+KeyboardEvent\s*\(/.test(l)) hits.push(`${f}:${i + 1}`);
        });
    }
    expect(hits, "合成的键盘事件不亮 :focus-visible、Enter 不点按钮").toEqual([]);
  });

  it("场景里不手描焦点环（真按键下样式表那一条自己会亮）", () => {
    const hits: string[] = [];
    for (const f of files) {
      readFileSync(path.join(dir, f), "utf8")
        .split("\n")
        .forEach((l, i) => {
          // 照样式表焦点那一条描（用焦点环的令牌）；诊断用的红框橙框（大折叠「标出没折的」）不算。
          if (/--focus-ring|--state-focus/.test(l)) hits.push(`${f}:${i + 1}`);
        });
    }
    expect(hits).toEqual([]);
  });

  it("key() 经页上的绑定请截图工具发真按键；截图工具开每一页都装这条绑定", () => {
    const helpers = readFileSync(path.join(dir, "helpers.ts"), "utf8");
    expect(helpers).toMatch(/__shotsKey\b/);
    const cdp = readFileSync(path.join(__dirname, "cdp.mjs"), "utf8");
    expect(cdp).toMatch(/Runtime\.addBinding[^\n]*__shotsKey/);
    expect(cdp).toMatch(/Input\.dispatchKeyEvent/);
  });
});
