/**
 * 因对方版本说不成：那一句 monitor 写（在 `ChanError.message`），界面只判形（`isOldBackend`）；
 * 回的认不出那一句是界面自己知道的事，界面说（`ReplyUnreadable`），不提版本。
 */
import { describe, expect, it } from "vitest";
import { ChanError } from "../../../../src/comms/inward/chan";
import { ReplyUnreadable, isOldBackend } from "../../../../src/frontend/ui/ipc/chan-caller";
import TABLE from "../../../../src/shared/copy/table.json";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const zh = (k: string): string => (TABLE.entries as Record<string, { zh: string }>)[k].zh;

describe("界面只说认不出、只判版本旧那一形", () => {
  it("回的形状不对 ⇒ 认不出那一句（带机器名，本机说「本机」），不提版本、细目不上屏", () => {
    const e = new ReplyUnreadable("devbox", "x reply shape");
    expect(e.message).toBe(zh("peerVersion.said.unreadable").replace("{machine}", "devbox"));
    expect(e.message).not.toMatch(/版本|更新/);
    expect(e.message).not.toContain("x reply shape");
    expect(e.detail).toBe("");
    expect(new ReplyUnreadable("<local>", "y").message).toBe(
      copyText("peerVersion.said.unreadable", { machine: copyText("control.machine.local") }),
    );
  });

  it("只有对端事前说不认这条命令才算版本旧；够不着 · 被拒 · 别的错都不算", () => {
    expect(isOldBackend(new ChanError({ layer: "peer", why: "unsupported" }))).toBe(true);
    const hop = new ChanError({ layer: "hop", at: { idx: 0, tag: "open" }, reach: "NotSent", why: "Unreachable" });
    const refused = new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode('{"code":"x","message":"那台说的话"}') });
    for (const e of [hop, refused, new ReplyUnreadable("devbox", "z"), new Error("plain")]) expect(isOldBackend(e)).toBe(false);
  });
});

describe("文案表里这一族只剩两句", () => {
  // 不在这一族的：部署换版本、文件格式版本、cc-bus / ccm / tmux 自己的版本 —— 它们不是「这条命令因对方版本说不成」。
  const FAMILY =
    /后端要更新|后端不支持此操作|后端(版本)?(太旧|过旧|旧了|版本旧|版本不对|版本对不上)|版本(不配|过旧|可能不对)|可能过旧|多半是.{0,6}版本|可能是.{0,6}版本|版本可能对不上|两边版本不一样|多半太旧/;
  it("新增一条自拼的「后端太旧 / 多半是版本不对」⇒ 红", () => {
    const hits = Object.entries(TABLE.entries as Record<string, { zh: string }>)
      .filter(([k, v]) => FAMILY.test(v.zh) && !k.startsWith("peerVersion."))
      .map(([k]) => k)
      .sort();
    expect(hits).toEqual(EXPECTED_REMAINING);
  });
});

/** 状态词：那台处在「要更新」状态时的标签（账号徽标 · 远端健康），不是命令被拒；机器状态成品那一刀统一。 */
const EXPECTED_REMAINING: string[] = [
  "accountChip.label.backendOld",
  "remoteHealth.head.backendOld",
];
