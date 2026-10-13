/**
 * **标识符放行判定：TS 读的生成物与 Rust 那一份对同一份金样。**
 *
 * 守的要求（逐字）：
 * - 要求：「**模型名**字符集 …… 定一个两侧同一份的规则（住共享 crate），真实模型名都放行。」
 * -：「凡是有对应 `*-core` crate 的判定，TS 侧零实现」—— 前端写入点那一句不手抄规则，
 *   读 monitor 从 `shell-quote-core` 现生成的 `src/frontend/ui/generated/judgment-rules.ts`。
 *
 * 金样 `tests/__fixtures__/identifier-rules.golden.json` 的期望是**手写**的；Rust 那一侧
 * （`tests/frontend/shell/payload_judgment_rules.rs::the_shared_golden_agrees_with_the_one_rule`）拿同一份喂
 * `shell_quote_core::account_name_ok`。两侧各自对金样，不是彼此对拍 —— 两侧同错才会一起绿（金样那一格就是挡这个的）。
 * 新建账号表单读生成的 `accountNameOk`。模型名界面不判（只在后端写口），生成物里没有它。
 * 买不到：金样之外的串两侧是否一致（式子 vs 循环两种写法），见 Rust 那份头注。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { ACCOUNT_NAME_PATTERN, accountNameOk } from "../../../src/frontend/ui/generated/judgment-rules.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const golden = JSON.parse(
  readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/identifier-rules.golden.json"), "utf8"),
) as Record<string, { ok: string[]; bad: string[] }>;

// 模型名（DUP1）不再生成到界面：界面不判，只在那台后端写口（`prefs-set`）与起会话时判；Rust 那一侧照旧对金样的 `model_name`。
it("模型名的规则不再生成到界面（界面不判）", () => {
  const gen = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/generated/judgment-rules.ts"), "utf8");
  expect(gen).not.toMatch(/MODEL_NAME|modelNameOk/);
  expect(gen, "空真自检：读到的是那份生成物").toMatch(/accountNameOk/);
});

describe("DUP2 账号名：生成物 == 共用金样（两向）", () => {
  it("金样读到了东西（反空真）", () => {
    expect(golden.account_name.ok.length).toBeGreaterThan(3);
    expect(golden.account_name.bad.length).toBeGreaterThan(3);
    expect(ACCOUNT_NAME_PATTERN.startsWith("^")).toBe(true);
  });

  it("ok 那一栏全过、bad 那一栏全拒", () => {
    const wrong = [
      ...golden.account_name.ok.filter((m) => !accountNameOk(m)).map((m) => `该过没过：${JSON.stringify(m)}`),
      ...golden.account_name.bad.filter((m) => accountNameOk(m)).map((m) => `该拒没拒：${JSON.stringify(m)}`),
    ];
    expect(wrong).toEqual([]);
  });
});
