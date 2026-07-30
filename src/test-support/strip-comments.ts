/**
 * 剥注释——**守卫用**（Rust 与 TS 共用一个扫描器，方言差异由 `lang` 参数决定）。
 *
 * ## 为什么抽出来
 *
 * C01/C02/C03 三个功能里，「拿注释当代码判据」这个形状**栽了四次**：
 * ① 生成物的 JSDoc 里含有 Rust doc comment 搬过来的字，被当成类型判据；
 * ② 剥了 TS 侧、忘了剥 Rust 侧，「钉住显式决定」那条断言被自己的注释喂饱；
 * ③ 裸 `grep -c bigint src/generated/*.ts` 数到 3 个文件，全在 JSDoc 散文里；
 * ④ 数 `#[tauri::command]` 时把**注释里提到**它的地方也计入，报出一个不存在的「7 个命令未注册」。
 *
 * C04a 起有**第二个守卫文件**要用它。两份手抄的剥注释语义一旦漂移，就是**静默削弱守卫**
 * ——而那正是 `rust-ts-boundary` 这个工作区在治的病。所以按 ≥2 那把尺子抽成单一实现。
 *
 * **只被 `*.vitest.ts` 引用 ⇒ 进不了 bundle**：vite 的入口是 `index.html` 里的 `/src/main.ts`，
 * 没有任何生产文件 import 任何 `.vitest.ts` ⇒ 本文件压根不在入口的模块图里
 * （**不是** tree-shaking 摇掉的——C04a Phase D 审计订正：结论对、原来写的机制不对）。
 * 这一条由 `generated-boundary-guard.vitest.ts` 的「生产代码不许 import test-support」机检。
 *
 * ## 为什么是个状态机，而不是两条正则
 *
 * 原来的实现是 `src.replace(/\/\*[\s\S]*?\*\//g,"").replace(/(^|[^:])\/\/.*$/gm,"$1")`。
 * **C04a Phase D 审计用变异实测把它打掉了**：`src-tauri/src/config_surface.rs:775` 有一行
 * `"~/.local/*\/bin",` —— 字符串字面量里的 `/*` 被当成块注释开始，非贪婪匹配一路吃到
 * `:1296` 的下一个 `*\/`，**521 行真 Rust 代码对所有守卫隐形**。
 *
 * 当时计数还全对纯属运气：该文件唯一的 `#[tauri::command]` 在 596 行，被吞的区间整段
 * 落在 `#[cfg(test)]`（631 行起）里。危险程度按断言方向分两档：
 * - 对**肯定式**断言（差集、等号计数）过剥 ⇒ **假红**，会叫，能发现；
 * - 对**否定式**断言（`.not.toContain("bigint")` · `.not.toMatch(/import .*invoke/)`）
 *   过剥 ⇒ **假绿**，永远不叫。
 *
 * 全仓另有 20+ 处字符串里含注释起始符（`"//"` · `"~/.local/bin/*-*"` · `"*\/subagents/*"`），
 * 13 个 `.rs` 文件的 `/*` 与 `*\/` 计数不配对。所以这不是理论洞，是**已实测的现存 bug**。
 *
 * ## 三条刻意的取舍（都由「等号计数」兜底，不是无声的假设）
 *
 * 1. **块注释不认嵌套**（第一个 `*\/` 收尾）。Rust 语法允许嵌套、TS 不允许；
 *    按嵌套处理会让 TS 的 `/** 匹配 src/*.ts *\/` 这种 JSDoc 过剥。选 TS 语义 ⇒
 *    Rust 的嵌套注释会**欠剥**（留下 `*\/` 当代码），方向是假红、会叫。
 * 2. **模板字符串整体当不透明字符串**，不回到代码模式解析 `${…}`。
 *    代价：`` `${await invoke("x")}` `` 里的命令名扫不到。兜底是
 *    `commands.vitest.ts` 把字面量命令名数钉成 `toBe(112)` 等号——真藏掉一个就红。
 * 3. **不处理 TS 正则字面量**。`/[/*]/` 这种字符类里含 `/*` 的写法会被误当注释开始
 *    （本仓今天 0 处）。同样由等号计数兜。
 *
 * **Rust 侧刻意不把 `'` 当字符串定界符**：本仓有 85 处 `&'static`、16 处 `&'a`、13 处 `'a>`，
 * 把 `'` 当定界符会从生命周期一路吃到下一个 `'`，**吃掉真代码**。
 * 而字符字面量对本函数无害——`'/'`、`'*'` 各自被 `'` 隔开，凑不出 `//` 或 `/*` 的相邻。
 */

/** 源码方言。**必填**：给默认值会让调用方静默拿到错的方言（Rust 的 `'a` vs TS 的 `'…'`）。 */
export type SourceLang = "rust" | "ts";

/**
 * 去掉块注释与行注释，**逐字符保留行结构**（注释里的字符换成空格，换行原样留下）。
 *
 * 「保留行结构」是有意的，而且现在是**真的**：C04a Phase D 审计指出旧实现这句是假声称
 * （块注释被连换行一起删，`src/generated/DataPathInfo.ts` 64 行剥成 16 行），
 * 而 `generated-boundary-guard.vitest.ts` 里「往上收属性要跳空行」那段循环的设计理由
 * 正建立在这个前提上，且那个形状按计划要复制 127 次 ⇒ 不能留一个假前提在根上。
 * 现在改成等量空格替换，行号与相邻关系逐行对齐。
 */
export function stripComments(src: string, lang: SourceLang): string {
  const out = src.split("");
  const n = src.length;

  /** 把 [from, to) 里的非换行字符换成空格——这就是「保留行结构」的实现。 */
  const blank = (from: number, to: number): void => {
    for (let k = from; k < to && k < n; k++) if (out[k] !== "\n") out[k] = " ";
  };

  const isIdent = (ch: string | undefined): boolean => ch !== undefined && /[A-Za-z0-9_]/.test(ch);

  /** 跳过一个带 `\` 转义的定界字符串，返回结束位置（定界符之后）。 */
  const skipQuoted = (start: number, quote: string): number => {
    let k = start + 1;
    while (k < n) {
      if (src[k] === "\\") k += 2;
      else if (src[k] === quote) return k + 1;
      else k++;
    }
    return n; // 未闭合：吃到文件尾（源码本身有语法错，守卫会以别的方式叫）
  };

  let i = 0;
  while (i < n) {
    const c = src[i];

    // ---- 行注释 ----
    if (c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      const end = nl < 0 ? n : nl;
      blank(i, end);
      i = end;
      continue;
    }

    // ---- 块注释（不认嵌套，见头注取舍 1）----
    if (c === "/" && src[i + 1] === "*") {
      const close = src.indexOf("*/", i + 2);
      const end = close < 0 ? n : close + 2;
      blank(i, end);
      i = end;
      continue;
    }

    // ---- Rust 原始字符串 r"…" / r#"…"# / br##"…"## ----
    if (lang === "rust" && (c === "r" || c === "b") && !isIdent(src[i - 1])) {
      const m = /^(?:br|rb|r|b)(#*)"/.exec(src.slice(i, i + 12));
      if (m && (m[1].length > 0 || m[0].startsWith("r") || m[0].startsWith("br"))) {
        const terminator = `"${m[1]}`;
        const bodyStart = i + m[0].length;
        const close = src.indexOf(terminator, bodyStart);
        i = close < 0 ? n : close + terminator.length;
        continue;
      }
    }

    // ---- 普通字符串 ----
    if (c === '"') {
      i = skipQuoted(i, '"');
      continue;
    }
    // TS 的 '…' 与 `…`。**Rust 的 ' 不在此列**（生命周期，见头注）。
    if (lang === "ts" && (c === "'" || c === "`")) {
      i = skipQuoted(i, c);
      continue;
    }

    i++;
  }

  return out.join("");
}
