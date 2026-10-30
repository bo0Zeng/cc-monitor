/**
 * 那台后端写的句子只点它知道的对象（各条命令自己的「码 → 句」表：`control/terminals_wire.rs` · `control/cc_bus_wire.rs`）：读画面 · 查在线 · 读名册 / 收件箱 · 新开 · 广播 这几条不带对象。
 * 用户定的条件：这些句子出现的每一处，对象都在旁边看得见（区块头 / toast 灰字 / 行内所在行）。
 * 本条把调用点一处不漏列进登记表，每处写清对象在哪；新长一处调用 / 调用点没了 ⇒ 两向红。
 */
import { describe, it, expect } from "vitest";
import { productionTsFiles } from "../../test-support/production-sources";
import { nonEmpty } from "../../test-support/non-empty";

const CALLS = ["previewShot", "sendToTerminal", "agentOnline", "readState", "readInbox", "spawnAgent", "broadcast"];
/** 不算调用点的家：定义它们的那两份。 */
const HOMES = new Set(["src/frontend/ui/terminal-reads.ts", "src/frontend/ui/cc-bus-control.ts"]);

/** `文件::函数` ⇒ 对象在哪（失败那一句旁边看得见的那个对象）。 */
const WHERE: Record<string, string> = {
  "src/frontend/ui/terminal-page.ts::previewShot": "终端页：错误条就在页头下面，页头写着会话 · 目录 · 机器",
  "src/frontend/ui/terminal-page.ts::sendToTerminal": "终端页：送字那一行写着「送往 {会话} · {机器}」",
  "src/frontend/ui/views/cc-bus-section.ts::agentOnline": "设置 → cc-bus：结果写在那个 agent 那一行里（行首是它的 id）",
  "src/frontend/ui/views/cc-bus-section.ts::readInbox": "设置 → cc-bus：结果写在那个 agent 那一行展开的收件箱里",
};


describe("不带对象的那几句：每个出处对象都看得见（登记两向）", () => {
  it("★ 调用点 == 登记表", () => {
    const got = new Set<string>();
    for (const { file: rel, text } of productionTsFiles("src/frontend/ui")) {
      if (HOMES.has(rel)) continue;
      // 去掉 import 那几行再找（当值传的 `send: sendToTerminal` 也算一处）。
      const code = text.replace(/^import [^;]*;$/gms, "");
      for (const c of CALLS) if (new RegExp(`(?<![A-Za-z0-9_$.])${c}\\b`).test(code)) got.add(`${rel}::${c}`);
    }
    nonEmpty(got);
    expect([...got].sort(), "多出来的：先写清那一句旁边对象在哪再登记；少了的：调用没了就删那一行").toEqual(Object.keys(WHERE).sort());
  });
});
