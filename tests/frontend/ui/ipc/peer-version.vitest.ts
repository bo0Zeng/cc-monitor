/**
 * 因对方版本说不成的两个码：版本旧 / 回的认不出各一句，按码取（`chan-caller.ts::saidFrom`）。
 * 钉三件事：两个码各落一句、认不出那一句不提版本、其余失败不落进这两个码。
 */
import { describe, expect, it } from "vitest";
import { ChanError } from "../../../../src/comms/inward/chan";
import * as chanCaller from "../../../../src/frontend/ui/ipc/chan-caller";
import { ReplyUnreadable, peerVersionCodeOf, saidFrom } from "../../../../src/frontend/ui/ipc/chan-caller";
import TABLE from "../../../../src/shared/copy/table.json";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const zh = (k: string): string => (TABLE.entries as Record<string, { zh: string }>)[k].zh;

describe("两个码", () => {
  it("对端不认这条命令 ⇒ 版本旧那一句（带机器名）", () => {
    const e = new ChanError({ layer: "peer", why: "unsupported" });
    expect(peerVersionCodeOf(e)).toBe("backend_old");
    expect(saidFrom(e, "devbox")).toBe(zh("peerVersion.said.old").replace("{machine}", "devbox"));
  });

  it("回的形状不对 / 不是 JSON ⇒ 认不出那一句，不提版本", () => {
    for (const e of [new ReplyUnreadable("x reply shape"), new SyntaxError("Unexpected token")]) {
      expect(peerVersionCodeOf(e)).toBe("reply_unreadable");
      const said = saidFrom(e, "devbox");
      expect(said).toBe(zh("peerVersion.said.unreadable").replace("{machine}", "devbox"));
      expect(said).not.toMatch(/版本|更新/);
      expect(said).not.toContain("x reply shape");
    }
  });

  it("本机说「本机」", () => {
    expect(saidFrom(new ChanError({ layer: "peer", why: "unsupported" }), "<local>")).toBe(copyText("peerVersion.said.old", { machine: copyText("control.machine.local") }));
  });

  it("够不着 / 对端说不行 / 别的错 ⇒ 不落进这两个码", () => {
    const hop = new ChanError({ layer: "hop", at: { idx: 0, tag: "open" }, reach: "NotSent", why: "Unreachable" });
    const refused = new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode('{"code":"x","message":"那台说的话"}') });
    expect(peerVersionCodeOf(hop)).toBeNull();
    expect(peerVersionCodeOf(refused)).toBeNull();
    expect(peerVersionCodeOf(new Error("plain"))).toBeNull();
    expect(saidFrom(refused, "devbox")).toBe("那台说的话");
  });
});

describe("调用方不再自拼「版本旧」那一句", () => {
  it("chan-caller 不再导出带一句旧话的 saidOf（各处一律 saidFrom 按码取）", () => {
    expect(Object.keys(chanCaller)).not.toContain("saidOf");
  });
});

describe("文案表里这一族只剩两句", () => {
  // 不在这一族的：机器状态词（要更新 / 版本较新）、部署换版本、文件格式版本、cc-bus / ccm 自己的版本 —— 它们不是「这条命令因对方版本说不成」。
  const FAMILY = /后端(版本)?(太旧|过旧|旧了|版本旧|版本不对|版本对不上)|多半是.{0,6}版本|可能是.{0,6}版本|版本可能对不上|两边版本不一样|多半太旧/;
  it("新增一条自拼的「后端太旧 / 多半是版本不对」⇒ 红", () => {
    const hits = Object.entries(TABLE.entries as Record<string, { zh: string }>)
      .filter(([k, v]) => FAMILY.test(v.zh) && !k.startsWith("peerVersion."))
      .map(([k]) => k)
      .sort();
    expect(hits).toEqual(EXPECTED_REMAINING);
  });
});

/** 还没收进两句、照实登记的那几条（后端 / 壳 / 文件窗口里还没换的；换一条删一条）。 */
const EXPECTED_REMAINING: string[] = [
  "accountOps.said.contract",
  "beInbound.dispatch.unknown",
  "beProbe.test.noControl",
  "beRemoteAsk.json.unreadable",
  "beRemoteAsk.run.tooOld",
  "rsBackendRoute.layer.unsupported",
  "rsCcmLegacy.verdict.unreadable",
  "rsChanHost.terminal.badReply",
  "rsFilewinCopy.remote.noBytes",
  "rsFilewinCopy.remote.noCount",
  "rsFilewinEditor.reply.noDigest",
  "rsFilewinEditor.reply.noText",
  "rsFilewinFind.reply.missingField",
  "rsFilewinGrep.reply.badShape",
  "rsFilewinProc.ready.unreadable",
  "rsFilewinSize.reply.missingField",
  "rsFilewinSource.said.unknownCmd",
  "rsFilewinTransfer.reply.missingField",
  "rsInboundClient.error.unsupported",
  "rsLinkMux.data.noCredit",
  "rsRemoteResident.ensure.tooOld",
  "rsSftp.plan.badProduct",
  "rsSshLink.dial.tooOld",
];
