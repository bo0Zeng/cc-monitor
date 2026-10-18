// 主界面 `bindEvents` 的活卡订阅清单（读 `src/frontend/ui/main.ts` 的接线；`events-tap.vitest.ts` 管订阅本身）。
// 守的要求（住址）：「远端常驻、本机远端同形」—— 远端的活卡住远端常驻后端，沿那台的流回 monitor。

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect } from "vitest";

import { REPO_ROOT } from "../../test-support/repo-root.ts";

describe("〔HOST〕主界面的活卡订阅覆盖每台机器", () => {
  it("liveCards 与 streams 出自同一份每台的订法（订的那几台跟着机器表走，每台两样都订）", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src", "frontend", "ui", "main.ts"), "utf8");
    const plan = /^\s*plan:\s*\((\w+)\)\s*=>\s*\(\{([\s\S]*?)\n\s*\}\),\n/m.exec(src);
    expect(plan, "main.ts 里找不到每台的订法 `plan: (origin) => ({ … })`").not.toBeNull();
    const [, who, body] = plan!;
    expect(/^\s*liveCards:\s*\[(\w+)\],/m.exec(body!)?.[1]).toBe(who);
    expect(/^\s*streams:\s*\[\{\s*origin:\s*(\w+),\s*kind:\s*"session-lines"\s*\}\],/m.exec(body!)?.[1] ?? /^\s*streams:\s*\[\{\s*(\w+),\s*kind:\s*"session-lines"\s*\}\],/m.exec(body!)?.[1]).toBe(who);
  });
});
