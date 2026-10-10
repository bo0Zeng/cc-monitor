// 认不出的那一行（通用记录 `unread`）：照抄核心写好的一句与摘录画一行细条，默认折起，展开有原文摘录 ＋［复制详情］；
// 不按 `type` / `why` 取字（两种原因画法一样）。夹具是核心金样里的那两条（`tests/__fixtures__/record.golden.jsonl`）。
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../../test-support/repo-root";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";

const ctx = () => ({ parentPath: "/p/s.jsonl", origin: LOCAL_ORIGIN, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy: false });
const golden = readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/record.golden.jsonl"), "utf8")
  .split("\n")
  .filter((l) => l.trim() !== "")
  .map((l) => JSON.parse(l) as { case: string; record: LineRecord });

describe("认不出的那一行", () => {
  const cases = golden.filter((g) => g.record.t === "unread");
  it("金样里两种原因各一条", () => expect(cases.map((c) => c.case)).toEqual(["unread-unknown", "unread-parse-failed"]));
  for (const { case: name, record } of cases) {
    it(`${name}：一行细条照抄 text，默认折起，展开是摘录与［复制详情］`, () => {
      if (record.t !== "unread") throw new Error(record.t);
      const r = renderMessage(record, ctx());
      if (r.kind !== "card") throw new Error(r.kind);
      const el = r.element as HTMLDetailsElement;
      expect(el.classList.contains("card-unread")).toBe(true);
      expect(el.tagName).toBe("DETAILS");
      expect(el.open).toBe(false);
      expect(el.querySelector("summary span")?.textContent).toBe(record.text);
      expect(el.querySelector(".card-unread-excerpt")?.textContent).toBe(record.excerpt);
      expect(el.querySelector("[data-part=copy-detail]")).not.toBeNull();
    });
  }
});
