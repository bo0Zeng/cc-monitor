// A6：cc-acct-iso 部署命令纯构建器单测——校验白名单 + 各 step 命令串精确 + 引号/拒双引号。
import { describe, it, expect } from "vitest";
import { validateAcctName, buildAcctIsoCmd } from "./acct-deploy";

describe("validateAcctName", () => {
  it("合法名放行", () => {
    for (const n of ["z", "b", "work", "z.edu", "a_b-c", "A1"]) {
      expect(validateAcctName(n)).toEqual({ ok: true });
    }
  });
  it("空 / 过长 / 非法字符 / 前导符 → 拒并给原因", () => {
    expect(validateAcctName("")).toMatchObject({ ok: false });
    expect(validateAcctName("a".repeat(65))).toMatchObject({ ok: false });
    expect(validateAcctName("a b")).toMatchObject({ ok: false }); // 空格
    expect(validateAcctName("a;rm")).toMatchObject({ ok: false }); // 元字符
    expect(validateAcctName("a$x")).toMatchObject({ ok: false });
    expect(validateAcctName("a'b")).toMatchObject({ ok: false }); // 单引号
    expect(validateAcctName('a"b')).toMatchObject({ ok: false }); // 双引号
    expect(validateAcctName("-x")).toMatchObject({ ok: false }); // 前导 -
    expect(validateAcctName(".x")).toMatchObject({ ok: false }); // 前导 .
  });
});

describe("buildAcctIsoCmd", () => {
  it("init 预览（dry-run，无 --apply）", () => {
    expect(buildAcctIsoCmd({ kind: "init-preview", name: "z" })).toEqual({
      ok: true,
      cmd: "cc-acct-iso init 'z'",
    });
  });
  it("init 迁移（--apply）", () => {
    expect(buildAcctIsoCmd({ kind: "init-apply", name: "z" })).toEqual({
      ok: true,
      cmd: "cc-acct-iso init 'z' --apply",
    });
  });
  it("verify / shellinit / sync 固定命令", () => {
    expect(buildAcctIsoCmd({ kind: "verify" })).toEqual({ ok: true, cmd: "cc-acct-iso verify" });
    expect(buildAcctIsoCmd({ kind: "shellinit" })).toEqual({
      ok: true,
      cmd: "cc-acct-iso shellinit",
    });
    expect(buildAcctIsoCmd({ kind: "sync-apply" })).toEqual({
      ok: true,
      cmd: "cc-acct-iso sync --apply",
    });
  });
  it("add（无凭据快照）", () => {
    expect(buildAcctIsoCmd({ kind: "add-apply", name: "b" })).toEqual({
      ok: true,
      cmd: "cc-acct-iso add 'b' --apply",
    });
  });
  it("add（带凭据快照路径，单引号包裹）", () => {
    expect(
      buildAcctIsoCmd({ kind: "add-apply", name: "b", credFile: "/home/z/.claude/accounts/b.json" }),
    ).toEqual({
      ok: true,
      cmd: "cc-acct-iso add 'b' --from-credentials '/home/z/.claude/accounts/b.json' --apply",
    });
  });
  it("login → cc-acct-iso run <名>", () => {
    expect(buildAcctIsoCmd({ kind: "login", name: "z" })).toEqual({
      ok: true,
      cmd: "cc-acct-iso run 'z'",
    });
  });
  it("非法账号名 → ok:false（各 step 都拦）", () => {
    expect(buildAcctIsoCmd({ kind: "init-apply", name: "a;rm" })).toMatchObject({ ok: false });
    expect(buildAcctIsoCmd({ kind: "add-apply", name: "" })).toMatchObject({ ok: false });
    expect(buildAcctIsoCmd({ kind: "login", name: "-x" })).toMatchObject({ ok: false });
  });
  it("凭据路径含双引号/控制字符 → 拒（与 launch.rs 双层防线）", () => {
    expect(
      buildAcctIsoCmd({ kind: "add-apply", name: "b", credFile: 'a"b' }),
    ).toMatchObject({ ok: false });
    expect(
      buildAcctIsoCmd({ kind: "add-apply", name: "b", credFile: "a\u0007b" }),
    ).toMatchObject({ ok: false });
  });
  it("凭据路径以 - 开头 → 拒（防被误当命令选项）", () => {
    expect(
      buildAcctIsoCmd({ kind: "add-apply", name: "b", credFile: "--apply" }),
    ).toMatchObject({ ok: false });
  });
  it("含单引号的路径 → 正确转义（结果不含双引号，launch.rs 可接受）", () => {
    const r = buildAcctIsoCmd({ kind: "add-apply", name: "b", credFile: "/p/it's/x.json" });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.cmd).toContain("'/p/it'\\''s/x.json'");
      expect(r.cmd).not.toContain('"');
    }
  });
});
