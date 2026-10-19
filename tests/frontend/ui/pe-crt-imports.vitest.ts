// @vitest-environment node
/**
 * Windows 程序导入表检查（`tests/scripts/pe-imports.mjs`）：发出去的 exe 不许动态链 C 运行库。
 * 没装 VC++ 运行库的 Windows 上，导入了 `VCRUNTIME140.dll` 的程序一起就弹「找不到 VCRUNTIME140.dll」。
 * 夹具是测试里现拼的最小 PE（一个节、导入表 ＋ 延迟导入表），不是真程序。
 */
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
// @ts-expect-error 纯 JS 脚本，没有类型声明
import { crtImports, importedDlls } from "../../scripts/pe-imports.mjs";

const SCRIPT = resolve(REPO_ROOT, "tests/scripts/pe-imports.mjs");

/** 拼一份 PE32+：`imports` 进普通导入表，`delayed` 进延迟导入表。 */
function pe(imports: string[], delayed: string[] = []): Buffer {
  const FILE_ALIGN = 0x200;
  const SECT_RVA = 0x1000;
  const peOff = 0x80;
  const optSize = 240; // PE32+：112 字节固定段 ＋ 16 个数据目录
  const sectHdr = peOff + 4 + 20 + optSize;
  const rawOff = FILE_ALIGN;

  // 节里的布局：导入描述符（20 字节一个，全零收尾）· 延迟导入描述符（32 字节一个，全零收尾）· 名字串
  const sect = Buffer.alloc(0x400);
  const impAt = 0;
  const delayAt = (imports.length + 1) * 20;
  let nameAt = delayAt + (delayed.length + 1) * 32;
  const putName = (n: string) => {
    const rva = SECT_RVA + nameAt;
    sect.write(n + "\0", nameAt, "latin1");
    nameAt += n.length + 1;
    return rva;
  };
  imports.forEach((n, i) => sect.writeUInt32LE(putName(n), impAt + i * 20 + 12));
  delayed.forEach((n, i) => {
    sect.writeUInt32LE(1, delayAt + i * 32); // Attributes：名字是 RVA
    sect.writeUInt32LE(putName(n), delayAt + i * 32 + 4);
  });

  const b = Buffer.alloc(rawOff + sect.length);
  b.write("MZ", 0, "latin1");
  b.writeUInt32LE(peOff, 0x3c);
  b.write("PE\0\0", peOff, "latin1");
  const coff = peOff + 4;
  b.writeUInt16LE(0x8664, coff); // Machine
  b.writeUInt16LE(1, coff + 2); // NumberOfSections
  b.writeUInt16LE(optSize, coff + 16);
  const opt = coff + 20;
  b.writeUInt16LE(0x20b, opt);
  b.writeUInt32LE(16, opt + 108); // NumberOfRvaAndSizes
  const dd = opt + 112;
  if (imports.length) b.writeUInt32LE(SECT_RVA + impAt, dd + 8 * 1);
  b.writeUInt32LE((imports.length + 1) * 20, dd + 8 * 1 + 4);
  if (delayed.length) b.writeUInt32LE(SECT_RVA + delayAt, dd + 8 * 13);
  b.writeUInt32LE((delayed.length + 1) * 32, dd + 8 * 13 + 4);
  b.write(".idata\0\0", sectHdr, "latin1");
  b.writeUInt32LE(sect.length, sectHdr + 8); // VirtualSize
  b.writeUInt32LE(SECT_RVA, sectHdr + 12);
  b.writeUInt32LE(sect.length, sectHdr + 16); // SizeOfRawData
  b.writeUInt32LE(rawOff, sectHdr + 20);
  sect.copy(b, rawOff);
  return b;
}

const boxes: string[] = [];
function file(name: string, bytes: Buffer): string {
  const d = mkdtempSync(join(tmpdir(), "pe-imports-"));
  boxes.push(d);
  const p = join(d, name);
  writeFileSync(p, bytes);
  return p;
}
afterEach(() => {
  for (const d of boxes.splice(0)) rmSync(d, { recursive: true, force: true });
});
const run = (...args: string[]) => spawnSync(process.execPath, [SCRIPT, ...args], { encoding: "utf8" });

describe("读导入表", () => {
  it("普通导入与延迟导入都读得出来", () => {
    expect(importedDlls(pe(["KERNEL32.dll", "USER32.dll"], ["shell32.dll"]))).toEqual([
      "KERNEL32.dll",
      "USER32.dll",
      "shell32.dll",
    ]);
  });

  it("只挑 C 运行库那几族，大小写不论", () => {
    expect(
      crtImports([
        "KERNEL32.dll",
        "VCRUNTIME140.dll",
        "vcruntime140_1.dll",
        "MSVCP140.dll",
        "api-ms-win-crt-runtime-l1-1-0.dll",
        "api-ms-win-core-synch-l1-2-0.dll",
        "msvcrt.dll",
        "ucrtbase.dll",
      ]),
    ).toEqual(["VCRUNTIME140.dll", "vcruntime140_1.dll", "MSVCP140.dll", "api-ms-win-crt-runtime-l1-1-0.dll"]);
  });
});

describe("pe-imports.mjs 命令行", () => {
  it("只导入系统 DLL ⇒ 过", () => {
    const r = run(file("ok.exe", pe(["KERNEL32.dll", "api-ms-win-core-synch-l1-2-0.dll", "msvcrt.dll"])));
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout).toContain("ok.exe");
  });

  it("★ 导入 VCRUNTIME140 ⇒ 红，说是哪一份导入了哪个", () => {
    const good = file("good.exe", pe(["KERNEL32.dll"]));
    const bad = file("ccm.exe", pe(["KERNEL32.dll", "VCRUNTIME140.dll", "api-ms-win-crt-heap-l1-1-0.dll"]));
    const r = run(good, bad);
    expect(r.status).toBe(1);
    expect(r.stderr).toContain("ccm.exe");
    expect(r.stderr).toContain("VCRUNTIME140.dll");
    expect(r.stderr).toContain("api-ms-win-crt-heap-l1-1-0.dll");
    expect(r.stderr).not.toContain("good.exe");
  });

  it("★ 延迟导入 MSVCP140 也算 ⇒ 红", () => {
    const r = run(file("filewin.exe", pe(["KERNEL32.dll"], ["MSVCP140.dll"])));
    expect(r.status).toBe(1);
    expect(r.stderr).toContain("MSVCP140.dll");
  });

  it("读不懂的不放过：不是 PE ⇒ 红；一个 DLL 都没读出来 ⇒ 红；没给文件 ⇒ 红", () => {
    expect(run(file("x.exe", Buffer.from("not a pe at all, just text"))).status).toBe(1);
    const empty = run(file("empty.exe", pe([])));
    expect(empty.status).toBe(1);
    expect(empty.stderr).toContain("empty.exe");
    expect(run().status).toBe(1);
    expect(run(join(tmpdir(), "pe-imports-no-such-file.exe")).status).toBe(1);
  });
});
