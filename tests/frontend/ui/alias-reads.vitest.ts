/**
 * 「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；线上形状由一份跨语言金样钉住」。
 *
 * 别名六问（`aliases-*`）改走通道：解码器读后端测试对拍过的同一份金样（`aliases.golden.json`）；
 * 请求问对那台、说对那条、参数原样。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { peerVersionSaid, ReplyUnreadable } from "../../../src/frontend/ui/ipc/chan-caller";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  AliasesStale,
  aliasFromForm,
  aliasToForm,
  decodeAliasForm,
  decodeAliasInstallReport,
  decodeAliasListing,
  decodeAliasRender,
  emptyForm,
  installAliasBlock,
  installAliases,
  readAliases,
  removeAliasBlock,
  renderAliasBlock,
  renderAliases,
} from "../../../src/frontend/ui/alias-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const G = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/aliases.golden.json"), "utf8")) as Record<
  string,
  Record<string, unknown>
>;
const FORM_CASES = G.formCases as unknown as Array<{ alias: unknown; form: Record<string, unknown> }>;

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，TS 读得懂", () => {
  it("aliases-render · aliases-install · aliases-read", () => {
    expect(decodeAliasRender(G.renderReply).problems.map((p) => p.name)).toEqual(["bad name"]);
    expect(decodeAliasInstallReport(G.installReply).wroteAliasFile).toBe(true);
    const l = decodeAliasListing(G.readReply);
    expect(l.aliases).toEqual([{ name: "alphacc", args: ["--", "--account", "z"], restTo: "agent" }]);
    expect(l.groups).toEqual([{ account: "z", tmux: false }]);
    expect(l.accounts).toEqual(["z", "b"]);
    expect(l.missing.map((m) => m.alias.name)).toEqual(["alphacct", "betacc", "betacct"]);
    expect(typeof l.fingerprint).toBe("string");
    expect(l.rcCandidates.map((c) => c.block.present)).toEqual([false]);
  });
});

describe("金样：表单两向（后端现算过的那几条）", () => {
  it("每条表单都读得懂；「＋ 新增」那张空表单与后端摊开一条空白别名逐格相等", () => {
    expect(FORM_CASES.length).toBeGreaterThanOrEqual(3);
    for (const c of FORM_CASES) expect(decodeAliasForm(c.form)).toEqual(c.form);
    expect(FORM_CASES[0].alias).toEqual({ name: "", args: [], restTo: "agent" });
    expect(emptyForm()).toEqual(decodeAliasForm(FORM_CASES[0].form));
  });
  it("表单严格收：多一格 / 缺一格 / 类型不对 / 认不得的「在哪起」⇒ 抛", () => {
    const f = FORM_CASES[1].form;
    const bad = ReplyUnreadable;
    expect(() => decodeAliasForm({ ...f, extra: 1 })).toThrow(bad);
    const { ccmOther: _c, ...short } = f;
    expect(() => decodeAliasForm(short)).toThrow(bad);
    expect(() => decodeAliasForm({ ...f, base: "no" })).toThrow(bad);
    expect(() => decodeAliasForm({ ...f, tmux: "screen" })).toThrow(bad);
    expect(() => decodeAliasForm({ ...f, cwdIf: [{ at: "~" }] })).toThrow(bad);
  });
});

describe("严格收", () => {
  it("多一格 / 缺一格 / 类型不对 ⇒ 抛（每一层都查）", () => {
    const r = G.readReply as Record<string, unknown>;
    expect(() => decodeAliasListing({ ...r, boundTerminals: 2 })).toThrow(ReplyUnreadable);
    const { otherRc: _o, ...short } = r;
    expect(() => decodeAliasListing(short)).toThrow(ReplyUnreadable);
    const cands = r.rcCandidates as Record<string, unknown>[];
    const c0 = cands[0] as Record<string, unknown>;
    expect(() => decodeAliasListing({ ...r, rcCandidates: [{ ...c0, block: { ...(c0.block as object), extra: 1 } }] })).toThrow(
      ReplyUnreadable,
    );
    expect(() => decodeAliasRender({ ...G.renderReply, collisions: [1] })).toThrow(ReplyUnreadable);
    expect(() => decodeAliasInstallReport({ ...G.installReply, wroteAliasFile: "yes" })).toThrow(ReplyUnreadable);
    // 归组与清单不等长 ⇒ 对不上哪一条是哪一组，不猜。
    expect(() => decodeAliasListing({ ...r, groups: [] })).toThrow(ReplyUnreadable);
    const a0 = (r.aliases as Record<string, unknown>[])[0];
    expect(() => decodeAliasListing({ ...r, aliases: [{ ...a0, restTo: "claude" }] })).toThrow(ReplyUnreadable);
  });
  it("存的时候那台说 `stale`（盘上被别处改过）⇒ 抛 AliasesStale（界面据此重读）；别的拒绝照旧是普通的错", async () => {
    invokeMock.mockRejectedValueOnce(refusedReply("stale", "被别处改过"));
    await expect(installAliases("devbox", [], "posix", "fp")).rejects.toBeInstanceOf(AliasesStale);
    invokeMock.mockRejectedValueOnce(refusedReply("refused", "有一条不合格"));
    const e = await installAliases("devbox", [], "posix", "fp").catch((x: unknown) => x);
    expect(e).not.toBeInstanceOf(AliasesStale);
    expect((e as Error).message).toContain("有一条不合格");
  });
  it("块那三口：预览只收 `{text}`；装 / 卸只收 `{}`", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ text: "x", more: 1 }));
    await expect(renderAliasBlock("devbox", "~/.bashrc")).rejects.toThrow(peerVersionSaid("reply_unreadable", "devbox"));
    invokeMock.mockResolvedValueOnce(chanReply({ ok: true }));
    await expect(removeAliasBlock("devbox", "~/.bashrc")).rejects.toThrow(peerVersionSaid("reply_unreadable", "devbox"));
  });
});

describe("请求：问对那台、说对那条、参数原样", () => {
  it("六问各一发", async () => {
    const a = [{ name: "alphacc", args: ["--account", "z"], restTo: "agent" as const }];
    invokeMock.mockResolvedValueOnce(chanReply(G.renderReply));
    await renderAliases("devbox", a, "posix");
    invokeMock.mockResolvedValueOnce(chanReply(G.readReply));
    await readAliases("<local>", "powershell", null);
    invokeMock.mockResolvedValueOnce(chanReply(G.installReply));
    await installAliases("devbox", a, "posix", "fp-1");
    invokeMock.mockResolvedValueOnce(chanReply({ text: "c" }));
    expect(await renderAliasBlock("devbox", "~/.bashrc")).toBe("c");
    invokeMock.mockResolvedValueOnce(chanReply({}));
    await installAliasBlock("devbox", "~/.bashrc");
    invokeMock.mockResolvedValueOnce(chanReply({}));
    await removeAliasBlock("devbox", "~/.bashrc");
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((x) => [x.origin, x.op, chanArgsJson(x)])).toEqual([
      ["devbox", "aliases-render", { aliases: a, shell: "posix" }],
      ["<local>", "aliases-read", { shell: "powershell", rcPath: null }],
      ["devbox", "aliases-install", { aliases: a, shell: "posix", fingerprint: "fp-1" }],
      ["devbox", "aliases-block-render", { rcPath: "~/.bashrc" }],
      ["devbox", "aliases-block-install", { rcPath: "~/.bashrc" }],
      ["devbox", "aliases-block-remove", { rcPath: "~/.bashrc" }],
    ]);
  });
  it("表单两向：摊开只交那一条；拼回交表单与原来那一条（新增 ⇒ null）；回的形状严格收", async () => {
    const c = FORM_CASES[1];
    invokeMock.mockResolvedValueOnce(chanReply({ form: c.form }));
    expect(await aliasToForm("devbox", c.alias as never)).toEqual(c.form);
    invokeMock.mockResolvedValueOnce(chanReply({ alias: c.alias }));
    expect(await aliasFromForm("devbox", c.form as never, c.alias as never)).toEqual(c.alias);
    invokeMock.mockResolvedValueOnce(chanReply({ alias: c.alias }));
    await aliasFromForm("<local>", emptyForm(), null);
    const calls = invokeMock.mock.calls.map((x) => x[1] as ChanCallArgs);
    expect(calls.map((x) => [x.origin, x.op, chanArgsJson(x)])).toEqual([
      ["devbox", "aliases-to-form", { alias: c.alias }],
      ["devbox", "aliases-from-form", { form: c.form, orig: c.alias }],
      ["<local>", "aliases-from-form", { form: emptyForm(), orig: null }],
    ]);
    invokeMock.mockResolvedValueOnce(chanReply({ form: c.form, more: 1 }));
    await expect(aliasToForm("devbox", c.alias as never)).rejects.toThrow(peerVersionSaid("reply_unreadable", "devbox"));
    invokeMock.mockRejectedValueOnce(refusedReply("refused", "交给 agent 的参数里有引号没配对。"));
    await expect(aliasFromForm("devbox", c.form as never, null)).rejects.toThrow("交给 agent 的参数里有引号没配对。");
  });
});
