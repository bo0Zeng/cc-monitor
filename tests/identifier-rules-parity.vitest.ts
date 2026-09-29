/**
 * 〔DUP1 · 第四波 4D〕**标识符放行判定：TS 读的生成物与 Rust 那一份对同一份金样。**
 *
 * 守的要求（逐字）：
 * - 主会话 09-26（TL3 交接三格之①）：「**模型名**字符集 …… 定一个两侧同一份的规则（住共享 crate），真实模型名都放行。」
 * - `设计/90 §3` 判据 2：「凡是有对应 `*-core` crate 的判定，TS 侧零实现」—— 前端写入点那一句不手抄规则，
 *   读 monitor 从 `shell-quote-core` 现生成的 `src/generated/judgment-rules.ts`。
 *
 * 金样 `tests/__fixtures__/identifier-rules.golden.json` 的期望是**手写**的；Rust 那一侧
 * （`tests/frontend/shell/backend/control/payload_judgment_rules.rs::the_shared_golden_agrees_with_the_one_rule`）拿同一份喂
 * `shell_quote_core::model_name_ok`。两侧各自对金样，不是彼此对拍 —— 两侧同错才会一起绿（金样那一格就是挡这个的）。
 * 〔DUP2 · J18〕账号名同一形：新建账号表单读生成的 `accountNameOk`，Rust 那一份是 `shell_quote_core::account_name_ok`，两侧对金样的 `account_name`。
 * 买不到：金样之外的串两侧是否一致（式子 vs 循环两种写法），见 Rust 那份头注。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { ACCOUNT_NAME_PATTERN, accountNameOk, MODEL_NAME_PATTERN, modelNameOk } from "../src/generated/judgment-rules.ts";
import { REPO_ROOT } from "./test-support/repo-root.ts";

const golden = JSON.parse(
  readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/identifier-rules.golden.json"), "utf8"),
) as Record<string, { ok: string[]; bad: string[] }>;

describe("DUP1 模型名：生成物 == 共用金样（两向）", () => {
  it("金样读到了东西（反空真）", () => {
    expect(golden.model_name.ok.length).toBeGreaterThan(5);
    expect(golden.model_name.bad.length).toBeGreaterThan(5);
    expect(MODEL_NAME_PATTERN.startsWith("^")).toBe(true);
  });

  it("ok 那一栏全过、bad 那一栏全拒", () => {
    const wrong = [
      ...golden.model_name.ok.filter((m) => !modelNameOk(m)).map((m) => `该过没过：${JSON.stringify(m)}`),
      ...golden.model_name.bad.filter((m) => modelNameOk(m)).map((m) => `该拒没拒：${JSON.stringify(m)}`),
    ];
    expect(wrong).toEqual([]);
  });
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
