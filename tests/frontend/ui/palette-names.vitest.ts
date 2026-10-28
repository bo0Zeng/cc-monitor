/**
 * 命令面板里没有两条同名的项（A2-17-12-2）：抽屉终端页那一条叫「看它的终端」（同右键），终端弹层那一条叫「终端名单」。
 */
import { describe, it, expect } from "vitest";
import table from "../../../src/shared/copy/table.json";
import { copyText } from "../../../src/frontend/ui/copy-table";

const entries = (table as { entries: Record<string, { zh?: string }> }).entries;

describe("命令面板项名", () => {
  it("main.cmd.* 的标题两两不同（Keywords 那几条是搜索词，不算）", () => {
    const seen = new Map<string, string[]>();
    for (const [k, v] of Object.entries(entries)) {
      if (!k.startsWith("main.cmd.") || k.endsWith("Keywords")) continue;
      seen.set(v.zh ?? "", [...(seen.get(v.zh ?? "") ?? []), k]);
    }
    expect([...seen.values()].filter((ks) => ks.length > 1)).toEqual([]);
  });

  it("两个「终端」：一条同右键「看它的终端」、一条「终端名单」", () => {
    expect(copyText("main.cmd.terminal")).toBe(copyText("tabMenu.open.drawer"));
    expect(copyText("main.cmd.openTerminals")).toBe("终端名单");
  });
});
