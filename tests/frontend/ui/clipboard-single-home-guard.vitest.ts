/**
 * 剪贴板只一个口：界面经壳（`src/frontend/ui/clipboard.ts` → 壳命令 `clipboard_write` · `clipboard_read`），拿系统接口的真成败。
 *
 * - 生产段零处网页自己的剪贴板口（`navigator.clipboard` · `.writeText(`）：WebView2 在剪贴板被占着时它也回成功（WIN5 · 10-08），
 *   用了它界面就会说「已复制」而什么都没写进去。
 * - `commands.clipboard_write` · `commands.clipboard_read` 只在 `clipboard.ts` 里调（别处一律经 `writeClipboard` · `readClipboard`）。
 * 正控：合成串认得出那几形。
 */
import { describe, it, expect } from "vitest";
import { productionTsFiles } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

/** 网页自己的剪贴板口（含解构出来再调的 `.writeText(`）。 */
const WEB_CLIPBOARD = /\bnavigator\s*\??\.\s*clipboard\b|\.(writeText|readText)\s*\(/;
/** 直呼那条壳命令。 */
const SHELL_CALL = /\bclipboard_(write|read)\s*\(/;

describe("写剪贴板只一个口", () => {
  it(
    "生产段零处 navigator.clipboard / .writeText(；壳命令只在 clipboard.ts 里调",
    () => {
      const code = productionTsFiles("src").map((f) => ({ file: f.file, code: stripComments(f.text, "ts") }));
      expect(code.length).toBeGreaterThan(100);
      expect(code.filter((f) => WEB_CLIPBOARD.test(f.code)).map((f) => f.file)).toEqual([]);
      expect(code.filter((f) => SHELL_CALL.test(f.code)).map((f) => f.file)).toEqual(["src/frontend/ui/clipboard.ts"]);
    },
  );

  it("正控：认得出那几形", () => {
    expect(WEB_CLIPBOARD.test("await navigator.clipboard.writeText(x)")).toBe(true);
    expect(WEB_CLIPBOARD.test("navigator.clipboard?.writeText(x)")).toBe(true);
    expect(WEB_CLIPBOARD.test("const clip = navigator.clipboard; clip.writeText(v)")).toBe(true);
    expect(WEB_CLIPBOARD.test("await writeClipboard(x)")).toBe(false);
    expect(SHELL_CALL.test("commands.clipboard_write({ text })")).toBe(true);
    expect(SHELL_CALL.test("commands.clipboard_read()")).toBe(true);
    expect(WEB_CLIPBOARD.test("await clip.readText()")).toBe(true);
  });
});
