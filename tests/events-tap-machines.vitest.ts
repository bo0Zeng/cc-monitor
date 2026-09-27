// 〔HOST · H8〕主界面 `bindEvents` 的 tap 订阅清单（读 `src/main.ts` 的接线；`events-tap.vitest.ts` 管订阅本身）。
// 守的要求（住址）：`99 §1` V139「远端常驻、本机远端同形」—— 远端中转住远端常驻后端，tap 沿那台的流回 monitor。

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect } from "vitest";

import { REPO_ROOT } from "./test-support/repo-root.ts";

describe("〔HOST〕主界面的 tap 订阅覆盖每台机器", () => {
  it("taps 与 streams 同一份 machines（相等，不是子集）", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src", "main.ts"), "utf8");
    expect(/^\s*taps:\s*([^,\n]+),/m.exec(src)?.[1]).toBe("machines");
    expect(/^\s*streams:\s*(\w+)\.map/m.exec(src)?.[1]).toBe("machines");
  });
});
