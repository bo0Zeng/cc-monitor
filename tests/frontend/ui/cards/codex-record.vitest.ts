// Codex 那一家的通用记录：界面照同一套画法画，不按 `agent` 分支。夹具是核心金样 `tests/__fixtures__/record-codex.golden.jsonl`
// （造的中性 rollout 走核心读页那一遍写出来的；手机端解同一份）。逐条照记录里的格对：人话 · 系统注入不建卡 · 推理 · 工具名与入参 ·
// 工具结果回灌进对应的那一条 · 回复正文 · 认不出的一行。
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../../test-support/repo-root";
import { renderMessage, type RenderContext } from "../../../../src/frontend/ui/cards/index";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";

const golden = readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/record-codex.golden.jsonl"), "utf8")
  .split("\n")
  .filter((l) => l.trim() !== "")
  .map((l) => JSON.parse(l) as { case: string; record: LineRecord });

function renderAll() {
  const ctx: RenderContext = {
    parentPath: "/p/rollout.jsonl",
    origin: LOCAL_ORIGIN,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  };
  const out = golden.map((g) => ({ ...g, result: renderMessage(g.record, ctx) }));
  return { ctx, out };
}

const textOf = (r: ReturnType<typeof renderMessage>): string => {
  if (r.kind === "card") return r.element.textContent ?? "";
  if (r.kind === "steps") return r.units.map((u) => u.textContent ?? "").join("\n");
  return "";
};

describe("Codex 的通用记录", () => {
  it("全是 codex 那一家的，金样里每一类都有", () => {
    expect(new Set(golden.map((g) => g.record.agent))).toEqual(new Set(["codex"]));
    expect(new Set(golden.map((g) => g.record.t))).toEqual(new Set(["said", "reply", "unread"]));
  });

  it("逐条照记录里的格画", () => {
    const { ctx, out } = renderAll();
    for (const { case: name, record: r, result } of out) {
      if (r.t === "said") {
        const kind = r.who.speaker.kind;
        if (kind === "system") expect(result.kind, name).toBe("skip");
        else if (kind === "human") {
          expect(result.kind, name).toBe("card");
          expect(textOf(result), name).toContain(r.who.text);
        } else if (kind === "toolResult") {
          // 结果回灌进前面那条 tool_use 的折叠条里：自己不另出东西，那一条的正文里有它。
          expect(result.kind, name).toBe("skip");
          for (const b of r.blocks) {
            if (b.type !== "tool_result") continue;
            const host = ctx.toolUseElements.get(b.for);
            expect(host, `${name} ⇒ ${b.for}`).toBeDefined();
            const inline = host?.querySelector<HTMLDetailsElement>(".block-tool-result-inline");
            expect(inline, name).not.toBeNull();
            // 结果正文展开才画：打开那一层再看。
            if (inline) {
              inline.open = true;
              inline.dispatchEvent(new Event("toggle"));
            }
            for (const c of b.content) if (c.type === "text") expect(inline?.textContent, name).toContain(c.text);
          }
        } else throw new Error(`${name}：金样里出现了没对的说话人 ${kind}`);
      } else if (r.t === "reply") {
        const blocks = r.blocks.filter((b) => b.type !== "text" && b.type !== "thinking" ? true : b.text.trim() !== "");
        if (blocks.length === 0) {
          expect(result.kind, name).toBe("skip");
          continue;
        }
        const text = textOf(result);
        for (const b of blocks) {
          if (b.type === "text" || b.type === "thinking") expect(text, name).toContain(b.text);
          else if (b.type === "tool_use") {
            expect(ctx.toolUseNames.get(b.id)?.name, name).toBe(b.name);
            expect(text, name).toContain(b.name);
          }
        }
      } else if (r.t === "unread") {
        expect(result.kind, name).toBe("card");
        expect(textOf(result), name).toContain(r.text);
      }
    }
  });

  it("function_call 的入参（arguments 那个 JSON 串）由核心解成了对象", () => {
    const call = golden.flatMap((g) => (g.record.t === "reply" ? g.record.blocks : [])).find((b) => b.type === "tool_use" && b.id === "call-1");
    expect(call?.type === "tool_use" ? call.input : null).toEqual({ command: ["ls"], workdir: "/work/demo" });
  });
});
