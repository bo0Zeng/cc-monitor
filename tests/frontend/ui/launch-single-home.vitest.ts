/**
 * **起会话的几件事各只有一个家**（生产段全集里数发送点，不是登记表；别处新写一处 ⇒ 集合多一个 ⇒ 红，报文点名那个文件）。
 *
 * | # | 性质 | 形状 |
 * |---|---|---|
 * | L1 | 界面起会话 · 恢复 · 接回只有一个家 | 生产段发 `session-open`（`callTyped(…, "session-open", …)`）的文件集合 == `{session-open.ts}`（两向；菜单置灰按命令名查能力那一处不是发送点） |
 * | L2 | 「这个会话在哪个 tmux 会话里」只问那台、一个问口 | 发 `sessions-where` 的文件 == `{sessions-where.ts}`；界面零处拿 tmux 名单自己判 |
 * | L3 | 起会话挑号界面零处判 | 先前那四个挑号函数名零命中（带正控）；线上 `named` 只从 `launch-account.ts` 出 |
 */
import { describe, it, expect } from "vitest";

import { productionTsFiles } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

/** 生成的 typed 命令表（命令名 ⇒ 入参 · 回包的类型）：里面也写着命令名，可它是类型，不是发送点。 */
const TYPED_MAP = "src/frontend/ui/generated/TypedCommands.ts";

/** 生产段（剥注释）里匹配 `re` 的文件集合（typed 命令表不算）。 */
function filesMatching(re: RegExp): string[] {
  return productionTsFiles()
    .filter((f) => f.file !== TYPED_MAP)
    .filter((f) => re.test(stripComments(f.text, "ts")))
    .map((f) => f.file)
    .sort();
}

describe("L1 · 界面起会话只有一个家", () => {
  it("★ 生产段发 `session-open` 的文件 == {session-open.ts}", () => {
    expect(
      filesMatching(/\bcallTyped\(\s*\w+\s*,\s*["']session-open["']/),
      "有别的地方自己去问那台起会话了 —— 发请求 · 按码挑按钮 · 开那一行 · 交等报到只许住 `src/frontend/ui/session-open.ts`",
    ).toEqual(["src/frontend/ui/session-open.ts"]);
  });
});

describe("L2 · 在哪个 tmux 会话里只问那台", () => {
  it("★ 发 `sessions-where` 的只有一个问口（两向）；界面零处拿 tmux 名单自己判", () => {
    expect(filesMatching(/["']sessions-where["']/), "有别的地方自己去问那台了").toEqual(["src/frontend/ui/sessions-where.ts"]);
    expect(
      filesMatching(/\b(?:listTmux|list_(?:local|remote)_tmux)\s*\(/),
      "又长出一处拿 tmux 名单自己判的 —— 判定只在那台后端（`sessions-where`）",
    ).toEqual([]);
  });
});

describe("L3 · 起会话挑号界面零处判", () => {
  const GONE = ["withAccount", "resolveLaunchAccount", "localFollowPlan", "followRecordName"];
  const hits = (text: string): string[] => GONE.filter((n) => new RegExp(`\\b${n}\\b`).test(text));

  it("★ 那四个函数名在生产段零命中（正控：同一把尺子认得出一段写着它们的文本）", () => {
    expect(hits("await withAccount(origin, null, run); localFollowPlan(sid)"), "尺子是瞎的").toEqual(["withAccount", "localFollowPlan"]);
    const found = productionTsFiles()
      .map((f) => ({ file: f.file, names: hits(f.text) }))
      .filter((f) => f.names.length > 0);
    expect(found, "界面又长出了起会话挑号的判定 —— 判号只在那台后端").toEqual([]);
  });

  it("★ 线上 `named`（用户点名）只从 `launch-account.ts` 出（两向）", () => {
    expect(filesMatching(/kind:\s*"named"/)).toEqual(["src/frontend/ui/launch-account.ts"]);
  });
});
