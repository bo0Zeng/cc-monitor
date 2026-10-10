/**
 * COPY · 生产代码不许按原文认话（判是哪种失败靠码，不靠句子）。
 *
 * 要求：「线上契约 | 不变：`Reply {code, message}`，`code` 仍是命令级粗码」——
 * 判「是哪一种失败」靠 `code` / 结构化字段；句子进了表就会被改写，谁按原文认它，改一个字就静默失灵
 *（CP2c 待办表的理由列：「refuse write:」等前缀被按原文认，先把判定换成码再抽）。
 * 判法：生产段（`src/backend` · `src/frontend/shell/src` · `src/frontend/filewin/src` · `src/common` · `src/comms` 的 `.rs` ＋ `src` 的 `.ts`，剥注释）里
 * 字符串匹配调用的字面量参数带汉字或 `refuse write/delete` 的地方 == `RECOGNIZE_BY_TEXT`（两向，按「文件 · 串」比）。
 *
 * 原先与 CP2c 待办表住同一个文件；CP1 · CP2b · CP2c 三条并进 `copy-ledgers.vitest.ts`（一趟普查）时拆出来，判法一字没动。
 */
import { describe, expect, it } from "vitest";

import { productionRsFiles, productionTsFiles, SCAN_TIMEOUT_MS } from "../test-support/production-sources.ts";
import { stripComments } from "../test-support/strip-comments.ts";

/**
 * 登记的例外：
 * - cc_bus 三条认的是 cc-bus 脚本自己的输出行（cc-bus 的文本不归文案表），不是表里的句子；
 * - `accounts.ts::deriveUi` 按「过旧」「不支持账号」认的那两条删了（少了两条）：「需更新」改认结构化的 `AccountsState.oldBackend`（`chan-caller.ts::isOldBackend`）；
 * - `tab-drop.ts::defaultGroupName` 认默认组名「组 N」续号那一条删了：改成照 `tabDrop.group.defaultName` 现取模板认（`tab-drop-default-name.vitest.ts`）。
 * - `plan/product.rs::why_code` 两条认的是 pb dump 里 `why` 的原话（「里面 d/m 做完了」，pb 的协议值，不归文案表），拆成 `whyCode` 交界面；
 *   拆 pb 原话只住这一处（pb 给了结构化的原因就删）。
 */
const RECOGNIZE_BY_TEXT = [
  "src/backend/control/cc_bus.rs · 已杀会话",
  "src/backend/control/cc_bus.rs · 已摘掉",
  "src/backend/control/cc_bus.rs · 已 spawn:",
  "src/backend/plan/product.rs · 里面 ",
  "src/backend/plan/product.rs ·  做完了",
];

const MATCH_CALL =
  /\.(?:contains|starts_with|ends_with|strip_prefix|strip_suffix|find|rfind|split_once|rsplit_once|matches|includes|startsWith|endsWith|indexOf|lastIndexOf)\(\s*b?(?:"((?:[^"\\\n]|\\.)*)"|'((?:[^'\\\n]|\\.)*)'|`([^`]*)`)/g;
const EQ_LIT = /(?:==|!=)\s*"((?:[^"\\\n]|\\.)*)"/g;
const RE_LIT = /\/((?:[^/\\\n]|\\.)+)\/[a-z]*\.(?:test|exec)\(/g;
const OUR_TEXT = /[\u4e00-\u9fff]|refuse (?:write|delete)/i;

/** 一段源码里按原文认话的串（剥过注释之后）。 */
function textRecognizers(src: string, lang: "rust" | "ts"): string[] {
  const code = stripComments(src, lang);
  const out: string[] = [];
  for (const rx of [MATCH_CALL, EQ_LIT, RE_LIT]) {
    for (const m of code.matchAll(rx)) {
      const lit = m[1] ?? m[2] ?? m[3] ?? "";
      if (OUR_TEXT.test(lit)) out.push(lit);
    }
  }
  return out;
}

describe("COPY · 生产代码不许按原文认话（判是哪种失败靠码，不靠句子）", () => {
  it("正控：现造的源码里认得出三种写法，注释里的不算", () => {
    const rs = 'fn f(e: &str) { if e.contains("超时") {} }\n// e.starts_with("指纹")\nlet x = m == "所有地址连接失败";';
    const ts = 'if (msg.startsWith("refuse write:")) {}\nif (/握手超时/.test(m)) {}';
    expect(textRecognizers(rs, "rust")).toEqual(["超时", "所有地址连接失败"]);
    expect(textRecognizers(ts, "ts")).toEqual(["refuse write:", "握手超时"]);
  });

  it("★ 生产段里按原文认话的地方 == 登记的例外（两向）", () => {
    const rs = [
      ...productionRsFiles("src/backend"),
      ...productionRsFiles("src/frontend/shell/src"),
      ...productionRsFiles("src/frontend/filewin/src"), // 文件窗口独立成包
      ...productionRsFiles("src/common"),
      ...productionRsFiles("src/comms"), // 通信层那两个 crate（面 A · 面 B）
    ].map((f) => ({ ...f, lang: "rust" as const }));
    const ts = productionTsFiles("src").map((f) => ({ ...f, lang: "ts" as const }));
    expect(rs.length, "Rust 生产文件一份都没扫到").toBeGreaterThan(100);
    expect(ts.length, "TS 生产文件一份都没扫到").toBeGreaterThan(100);
    const found = [...rs, ...ts].flatMap((f) => textRecognizers(f.text, f.lang).map((lit) => `${f.file} · ${lit}`));
    expect(
      [...new Set(found)].sort(),
      "有生产代码按句子原文判断是哪种失败 —— 句子会被改写（文案表），改成认 code / 结构化字段",
    ).toEqual([...RECOGNIZE_BY_TEXT].sort());
  }, SCAN_TIMEOUT_MS);
});
