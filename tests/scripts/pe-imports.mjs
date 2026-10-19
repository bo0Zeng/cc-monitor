#!/usr/bin/env node
// Windows 程序导入表检查：发出去的 exe（含内嵌进 monitor 的本机后端 · 文件窗口）不许动态链 C 运行库。
// 没装 VC++ 运行库的 Windows 上，导入了 `VCRUNTIME140.dll` 的程序一起就弹「找不到 VCRUNTIME140.dll」；
// `MSVCP*` 同理；`api-ms-win-crt-*`（UCRT 的转发层）系统自带，但静态链 C 运行库之后它也不该出现 ——
// 出现了就说明那一份没按静态链编。读的是普通导入表与延迟导入表里的 DLL 名，自己解 PE，不靠外部工具。
//
// 跑法：
//     node tests/scripts/pe-imports.mjs <exe>...
// 退出码 0 = 每一份都只导入系统 DLL；1 = 有一份导入了 C 运行库 / 读不懂 / 一个 DLL 都没读出来 / 没给文件。
// 调它的：`release.yml` Windows 那一 job（msvc 编的三份）· 门禁 `winlink` 那一格（本机交叉编的 windows-gnu 那两份）。

import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { fileURLToPath } from "node:url";

const CRT = /^(vcruntime|msvcp|api-ms-win-crt-)/i;

/** 名字里属于 C 运行库那几族的。 */
export function crtImports(dlls) {
  return dlls.filter((d) => CRT.test(d));
}

/** 一份 PE 的普通导入 ＋ 延迟导入的 DLL 名（按表里的顺序）。读不懂就抛。 */
export function importedDlls(buf) {
  const u16 = (o) => buf.readUInt16LE(o);
  const u32 = (o) => buf.readUInt32LE(o);
  if (buf.length < 0x40 || buf.toString("latin1", 0, 2) !== "MZ") throw new Error("没有 MZ 头，不是 PE");
  const pe = u32(0x3c);
  if (pe + 24 > buf.length || buf.toString("latin1", pe, pe + 4) !== "PE\0\0") throw new Error("没有 PE 签名");
  const nSect = u16(pe + 6);
  const optSize = u16(pe + 20);
  const opt = pe + 24;
  const magic = u16(opt);
  const pe32plus = magic === 0x20b;
  if (!pe32plus && magic !== 0x10b) throw new Error(`可选头的 magic 认不得（0x${magic.toString(16)}）`);
  const imageBase = pe32plus ? Number(buf.readBigUInt64LE(opt + 24)) : u32(opt + 28);
  const nDirs = u32(opt + (pe32plus ? 108 : 92));
  const dirs = opt + (pe32plus ? 112 : 96);
  const sects = [];
  for (let i = 0, s = opt + optSize; i < nSect; i++, s += 40) {
    sects.push({ va: u32(s + 12), vsize: Math.max(u32(s + 8), u32(s + 16)), raw: u32(s + 20) });
  }
  const off = (rva) => {
    const s = sects.find((x) => rva >= x.va && rva < x.va + x.vsize);
    if (!s) throw new Error(`RVA 0x${rva.toString(16)} 不落在任何节里`);
    return rva - s.va + s.raw;
  };
  const cstr = (rva) => {
    const at = off(rva);
    const end = buf.indexOf(0, at);
    if (end < 0) throw new Error("DLL 名没有结尾");
    return buf.toString("latin1", at, end);
  };
  const dir = (i) => (i < nDirs ? u32(dirs + 8 * i) : 0);

  const out = [];
  // 普通导入表（数据目录 1）：20 字节一个描述符，Name 的 RVA 在 +12，全零收尾。
  for (let d = dir(1) && off(dir(1)); dir(1); d += 20) {
    if (u32(d) === 0 && u32(d + 12) === 0 && u32(d + 16) === 0) break;
    out.push(cstr(u32(d + 12)));
  }
  // 延迟导入表（数据目录 13）：32 字节一个，DllName 在 +4；Attributes 第 0 位为 0 的老格式存的是 VA。
  for (let d = dir(13) && off(dir(13)); dir(13); d += 32) {
    const name = u32(d + 4);
    if (name === 0) break;
    out.push(cstr(u32(d) & 1 ? name : name - imageBase));
  }
  return out;
}

function main(files) {
  if (files.length === 0) {
    console.error("pe-imports: 没给要查的 exe（跑法：node tests/scripts/pe-imports.mjs <exe>...）");
    return 1;
  }
  let bad = 0;
  for (const f of files) {
    const name = basename(f);
    let dlls;
    try {
      dlls = importedDlls(readFileSync(f));
    } catch (e) {
      console.error(`pe-imports: ${name}：读不懂导入表 —— ${e.message}（${f}）`);
      bad++;
      continue;
    }
    if (dlls.length === 0) {
      console.error(`pe-imports: ${name}：一个导入的 DLL 都没读出来 —— 真程序至少导入 kernel32，读法坏了（${f}）`);
      bad++;
      continue;
    }
    const crt = crtImports(dlls);
    if (crt.length) {
      console.error(`pe-imports: ${name} 导入了 C 运行库：${crt.join(" · ")} —— 这一份没按静态链 C 运行库编（VCRUNTIME* · MSVCP* 在没装 VC++ 运行库的 Windows 上找不到）（${f}）`);
      bad++;
    } else {
      console.log(`pe-imports: ${name}：${dlls.length} 个 DLL，没有 C 运行库`);
    }
  }
  return bad ? 1 : 0;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) process.exit(main(process.argv.slice(2)));
