/**
 * 设置窗「别名与配置文件」五问（`profiles-*`）：解码器读后端现算对拍过的同一份金样（`profiles.golden.json`），按形状严格收；
 * 写那一口那台说 `stale` ⇒ 抛 ProfilesStale（界面据此重读、表单留着）。
 */
import { describe, it, expect, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { ReplyUnreadable } from "../../../src/frontend/ui/ipc/chan-caller";
import {
  decodeBases,
  decodeBook,
  decodeImpact,
  decodeResolved,
  decodeWriteDone,
  PROFILES_CHANGED_KIND,
  ProfilesStale,
  writeProfiles,
} from "../../../src/frontend/ui/profiles-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { refusedReply } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const G = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/profiles.golden.json"), "utf8")) as Record<string, unknown>;

describe("profiles-* 五问的成品按金样严格收", () => {
  it("五口的金样都收得下，收下来逐字不变", () => {
    expect(decodeBook(G.readReply)).toEqual(G.readReply);
    expect(decodeResolved(G.resolveReply)).toEqual(G.resolveReply);
    expect({ affected: decodeImpact(G.impactReply) }).toEqual(G.impactReply);
    expect({ bases: decodeBases(G.basesReply) }).toEqual(G.basesReply);
  });

  it("写那一口收得下；「手改过」那一格是后端写好的时刻串或 null，给数不收", () => {
    expect(decodeWriteDone(G.writeReply)).toEqual(G.writeReply);
    const book = G.readReply as Record<string, unknown>;
    expect(decodeBook({ ...book, editedAt: "14:20" }).editedAt).toBe("14:20");
    expect(() => decodeBook({ ...book, editedAt: 1791380000 })).toThrow(ReplyUnreadable);
  });

  it("多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜", () => {
    const book = G.readReply as Record<string, unknown>;
    expect(() => decodeBook({ ...book, extra: 1 })).toThrow(ReplyUnreadable);
    const { accounts: _gone, ...short } = book;
    expect(() => decodeBook(short)).toThrow(ReplyUnreadable);
    const rows = book.profiles as Record<string, unknown>[];
    expect(() => decodeBook({ ...book, profiles: [{ ...rows[0], kind: "alias" }] })).toThrow(ReplyUnreadable);
    const form = rows[0].form as Record<string, unknown>;
    expect(() => decodeBook({ ...book, profiles: [{ ...rows[0], form: { ...form, account: { kind: "account" } } }] })).toThrow(ReplyUnreadable);
  });

  it("那台说 stale ⇒ ProfilesStale；别的拒绝是普通的错", async () => {
    invokeMock.mockRejectedValueOnce(refusedReply("stale", "被别处改过"));
    await expect(writeProfiles("devbox", [], "fp")).rejects.toBeInstanceOf(ProfilesStale);
    invokeMock.mockRejectedValueOnce(refusedReply("refused", "多出坏处"));
    const e = await writeProfiles("devbox", [], "fp").catch((x: unknown) => x);
    expect(e).not.toBeInstanceOf(ProfilesStale);
  });

  it("流名与壳那一侧同一个串", () => {
    const rs = readFileSync(resolve(REPO_ROOT, "src/frontend/shell/src/event_replay.rs"), "utf8");
    expect(rs).toContain(`pub const PROFILES_CHANGED_KIND: &str = "${PROFILES_CHANGED_KIND}";`);
  });
});
