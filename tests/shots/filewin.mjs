/**
 * 文件窗口（egui）那一半：私有 Xvfb ＋ `workspace_tests.rs` 末尾那一格截图测试，一张图一个进程
 * （winit 一个进程只许建一个事件循环）。测试二进制先 `cargo test --no-run` 编出来，再逐场景按名字拉起。
 */
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import path from "node:path";
import { sleep } from "./cdp.mjs";

/** 截图测试的全名（`cargo test` 的过滤串；改名要同拍改这里）。 */
const TEST = "workspace::tests::screenshot_for_the_shots_tool";

export const FILEWIN_SCENES = [
  { id: "filewin-main", scene: "main", title: "文件窗口 · 单栏 ＋ 预览", desc: "远端 devbox 上的一个项目目录，选中 main.rs、右侧预览（代码高亮）；左栏书签。左栏「家目录」灰着、预览头上是沙箱路径：都是合成后端的痕迹" },
  { id: "filewin-split", scene: "split", title: "文件窗口 · 双栏", desc: "开了双栏：两栏同一个目录" },
  { id: "filewin-empty", scene: "empty", title: "文件窗口 · 空目录", desc: "进到一个空目录" },
  { id: "filewin-missing", scene: "missing", title: "文件窗口 · 目录不存在", desc: "进到一个不存在的目录：列表那里报错" },
  { id: "filewin-pull", scene: "pull", title: "文件窗口 · 下载在路上", desc: "一趟下载在路上：「进度」表收着，状态栏右端「进度 1」带一小条合计进度（点开是表里那一行）" },
  { id: "filewin-mkdir-error", scene: "mkdir-error", title: "文件窗口 · 新建文件夹名字填错", desc: "就地新建文件夹那一格把名字清空了就回车：红边 ＋ 格子下面挂一句「名称为空」，那一格留着" },
  { id: "filewin-search", scene: "search", title: "文件窗口 · 按名字搜", desc: "工具条搜索框里输「retry」：全家目录里名字带它的文件。索引那一行的「扫描间隔 4242 秒」是合成后端故意给的怪数、命中路径是沙箱里的长路径" },
  { id: "filewin-progress", scene: "progress", title: "文件窗口 · 进度", desc: "窗口底部「进度」表：上传一摞与下载在跑（可停）· 删除文件夹在跑（那台撤不动，「停」灰着）· 上传失败（重试 2 个）· 复制到另一台完成；状态栏「进度 3」带红点" },
  { id: "filewin-stale", scene: "stale", title: "文件窗口 · 断线过期", desc: "和 devbox 断了：工具条下一条「devbox 离线 · 采样 13:40」＋［重新连接］；列表照常摆着上次那一屏；写类按钮灰（悬停「离线 · 只读」）" },
  { id: "filewin-unreadable", scene: "unreadable", title: "文件窗口 · 无权限目录清单", desc: "按名字搜，状态行「3 个目录无权限［查看］」点开：列后端交来的那两个目录（相对搜索起点）＋「另外 1 个」；点一行复制那条绝对路径（稿里没画，待认）" },
  { id: "filewin-edit-page", scene: "edit-page", title: "文件窗口 · 编辑页", desc: "目录页上开 main.rs ⇒ 同一栏一个编辑页（标签「✎ main.rs」、未保存点）：头条面包屑 · 未保存 · 查找 · 保存 · ⋯；行号槽 ＋ 正文在面里滚；查找条浮在右上；状态栏是底条（行 · 列 · UTF-8 · LF · 大小 / 上限）" },
  { id: "filewin-edit-stale", scene: "edit-stale", title: "文件窗口 · 编辑页 · 盘上被改过", desc: "保存时盘上那份在打开之后被改过：编辑面顶一条「main.rs 已在盘上被修改」［覆盖］［放弃改动并重开］，字一个不动" },
  { id: "filewin-edit-close", scene: "edit-close", title: "文件窗口 · 关编辑页 · 没保存", desc: "改了没存点 ×：「关闭 main.rs · 未保存」「不保存 = 丢弃改动」［取消］［不保存］［保存］（焦点在保存）" },
  { id: "filewin-close-window", scene: "close-window", title: "文件窗口 · 关窗那一问", desc: "有没保存的编辑页与一趟在下的：「关闭文件窗口 · devbox」按族列出（未保存 · 传输中）；［取消］［全部保存后关闭］［仍然关闭］" },
  { id: "filewin-delete-ask", scene: "delete-ask", title: "文件窗口 · 删除那一问", desc: "选中一个文件夹 ＋ 两个文件按 Delete（稿 09）：「删除 3 项」· 一块清单（图标 ＋ 名字 ｜ 右端「文件夹」/ 大小）· 不可恢复 · 取消（焦点）· 删除 3 项（危险）" },
  { id: "filewin-rename-error", scene: "rename-error", title: "文件窗口 · 就地改名 · 填错", desc: "main.rs 上 F2：名字那一格成了输入框，改成 Cargo.toml 回车 ⇒ 后端回「已存在」：红边 ＋ 格子下面挂一句「Cargo.toml 已存在」，那一格留着（稿 07）" },
  { id: "filewin-new-folder", scene: "new-folder", title: "文件窗口 · 就地新建文件夹 ＋ 改名回执", desc: "「新建 ▾ → 文件夹」：文件夹那一段之后冒出一行「新建文件夹」、名字整个选中；右下角是上一次改名的回执「已改名为 retry.rs［撤销］」（稿 08）" },
  { id: "filewin-chmod", scene: "chmod", title: "文件窗口 · 改权限", desc: "「⋯ → 权限」：「改权限 · main.rs」·「当前 644」· 3 × 3 勾（勾了所有者 / 同组 / 其他人的执行）·「数字写法 755」两边联动 ·［取消］［改权限］（稿 10）" },
  { id: "filewin-upload-clash", scene: "upload-clash", title: "文件窗口 · 上传遇到同名", desc: "上传三个文件、两个同名：「同名 2 · devbox」一张表（☐ 名字 · 本机 · devbox，各写大小 · 时间）·「勾选的覆盖 · 未勾的跳过 · 不重名的 1 个照传」·［取消整批］［全部跳过］［覆盖勾选的 n 个］（稿 12）" },
  { id: "filewin-cross-copy", scene: "cross-copy", title: "文件窗口 · 复制到另一台", desc: "report.pdf「复制到另一台」：「复制到」下拉 gpu-01 ·「放到」那台的面包屑 ＋ 文件夹（选中 inbox）· 右上「新建文件夹」·「目标 gpu-01:…/inbox」·［取消］［复制到 gpu-01］（稿 13；合成后端一台答所有机器，那台的主目录是沙箱路径）" },
  { id: "filewin-drag-in", scene: "drag-in", title: "文件窗口 · 从桌面拖进来", desc: "拖着两个文件经过列表：列表区一层淡强调色底 ＋ 虚线框「松开上传 → orders-service（2 个文件）」（稿 20）" },
  { id: "filewin-narrow-drawer", scene: "narrow-drawer", width: 720, title: "文件窗口 · 窄档 720 · 左栏抽屉", desc: "窗宽 720：左栏收成盖在列表上的抽屉（不挤列表），开关打开着（稿 23）" },
  { id: "filewin-peek", scene: "peek", width: 720, title: "文件窗口 · 窄档 · 空格看一眼", desc: "窗宽 720、main.rs 上按空格：窗口正中一块「看一眼」浮层（代码高亮）；Esc / 空格 / 点别处收（稿 24）" },
  { id: "filewin-focus", scene: "focus", title: "文件窗口 · 键盘焦点环", desc: "按 Tab 走到搜索框：2px 强调色外环（鼠标点出来的不画）" },
].map((s) => ({ dir: "文件窗口", width: 1280, height: 800, ...s }));

export async function shootFilewin({ repo, sandbox, out, scenes, env, buildEnv, children }) {
  const results = [];
  const problems = [];
  const crate = path.join(repo, "src/frontend/filewin");
  console.log("文件窗口：编测试二进制（第一次要几分钟）…");
  const build = spawnSync("cargo", ["test", "--lib", "--no-run", "--message-format=json"], {
    cwd: crate,
    // 编译要真 HOME（rustup / cargo 的家），别的照摘
    env: { ...buildEnv, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "3" },
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
  const exe = build.stdout
    .split("\n")
    .filter((l) => l.startsWith("{"))
    .map((l) => JSON.parse(l))
    .find((m) => m.reason === "compiler-artifact" && m.profile?.test && m.target?.name === "cc_monitor_filewin" && m.executable)?.executable;
  if (build.status !== 0 || !exe) {
    problems.push(`文件窗口：测试二进制没编出来（cargo 退出码 ${build.status}）`);
    return { results, problems };
  }

  const display = freeDisplay();
  if (display === null) {
    problems.push("文件窗口：找不到空闲的 X 显示号");
    return { results, problems };
  }
  const xvfb = spawn("Xvfb", [`:${display}`, "-screen", "0", "1600x1000x24", "-nolisten", "tcp", "-noreset"], { env, stdio: "ignore" });
  children.push(xvfb);
  for (let i = 0; i < 100 && !existsSync(`/tmp/.X11-unix/X${display}`); i++) await sleep(100);

  for (const s of scenes) {
    const file = path.join(out, s.dir, `${s.id}.png`);
    mkdirSync(path.dirname(file), { recursive: true });
    // 合成的家目录：地址栏末几段读起来像一台真机器（…/home/user/work/orders-service）
    const home = path.join(sandbox, "filewin", s.scene, "home", "user");
    layFixture(home);
    const runEnv = {
      ...env,
      DISPLAY: `:${display}`,
      CCM_SHOTS_FILEWIN_SCENE: s.scene,
      CCM_SHOTS_FILEWIN_OUT: file,
      CCM_SHOTS_FILEWIN_DIR: home,
      CCM_SHOTS_FILEWIN_PREVIEW: CODE,
      CCM_SHOTS_FILEWIN_W: String(s.width),
    };
    delete runEnv.WAYLAND_DISPLAY;
    const r = spawnSync(exe, [TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"], { env: runEnv, encoding: "utf8", timeout: 120_000 });
    const ok = r.status === 0 && existsSync(file);
    if (!ok) {
      const why = `${r.stderr ?? ""}\n${r.stdout ?? ""}`.split("\n").filter((l) => /panicked|error/i.test(l)).slice(0, 2).join(" | ");
      problems.push(`${s.id}：没截成（退出码 ${r.status}）${why}`);
    }
    results.push({ ...s, file: path.relative(out, file), ok });
    console.log(`${ok ? "✓" : "✗"} ${s.dir}/${s.id}`);
  }
  try {
    process.kill(xvfb.pid, "SIGTERM");
  } catch {
    // 已经退了
  }
  return { results, problems };
}

const CODE = `//! 库存客户端：统一的重试与超时。
use std::time::Duration;

/// 一次调用最多重试几次、首次退避多久；之后每次翻倍。
pub struct Retry {
    pub times: u32,
    pub backoff: Duration,
}

impl Retry {
    pub fn delay(&self, attempt: u32) -> Duration {
        // 第 n 次重试前等 backoff × 2^n
        self.backoff * 2u32.pow(attempt)
    }
}

fn main() {
    let r = Retry { times: 3, backoff: Duration::from_millis(200) };
    for i in 0..r.times {
        println!("第 {} 次重试前等 {:?}", i + 1, r.delay(i));
    }
}
`;

/** 铺一个合成的项目目录（占位文件；修改时间钉死，每次截出来一样）。 */
function layFixture(home) {
  rmSync(home, { recursive: true, force: true });
  const dir = path.join(home, "work", "orders-service");
  const at = (iso) => new Date(iso);
  const dirs = [["src", "2026-10-01T09:12:00"], ["docs", "2026-09-28T17:40:00"], ["tests", "2026-10-01T09:20:00"], [".git", "2026-10-01T09:31:00"], ["node_modules", "2026-09-02T11:05:00"], ["empty-dir", "2026-09-30T08:00:00"]];
  for (const [d] of dirs) mkdirSync(path.join(dir, d), { recursive: true });
  const files = [
    ["README.md", "# orders-service\n\n订单服务。\n", "2026-09-15T10:00:00"],
    [".env", "A=1\n", "2026-08-01T09:00:00"],
    [".gitignore", "target\n", "2026-08-01T09:00:00"],
    ["Cargo.toml", '[package]\nname = "orders"\nversion = "0.1.0"\n', "2026-09-20T14:22:00"],
    ["notes.txt", "todo\n".repeat(200), "2026-09-29T18:03:00"],
    ["report.pdf", Buffer.alloc(182_000), "2025-12-31T23:59:00"],
    ["release.tar.gz", Buffer.alloc(3_400_000), "2026-09-30T20:15:00"],
    ["logo.png", Buffer.alloc(24_000), "2026-07-11T08:30:00"],
    ["main.rs", CODE, "2026-10-01T09:18:00"],
    ["src/retry.rs", "pub fn backoff() {}\n", "2026-10-01T09:12:00"],
    ["src/clients.rs", "pub struct InventoryClient;\n", "2026-10-01T09:11:00"],
    ["tests/retry_test.rs", "#[test]\nfn retries() {}\n", "2026-10-01T09:20:00"],
    ["docs/retry.md", "# 重试\n", "2026-09-28T17:40:00"],
  ];
  for (const [name, body, t] of files) {
    writeFileSync(path.join(dir, name), body);
    utimesSync(path.join(dir, name), at(t), at(t));
  }
  // 目录的修改时间最后钉（往里写文件会把它刷成现在）
  for (const [d, t] of dirs) utimesSync(path.join(dir, d), at(t), at(t));
  // 「复制到另一台」那一张：那台主目录底下的几个文件夹（合成后端一台答所有机器）。
  for (const d of ["datasets", "inbox", "runs", "ranker"]) mkdirSync(path.join(home, d), { recursive: true });
  // 「上传遇到同名」那一张：本机那一侧的三份（两份与目录里同名）。
  const local = path.join(home, "..", "..", "local");
  mkdirSync(local, { recursive: true });
  for (const [name, n, t] of [["main.rs", 612, "2026-10-01T13:50:00"], ["README.md", 34, "2026-09-12T10:00:00"], ["new.txt", 9, "2026-10-01T13:50:00"]]) {
    writeFileSync(path.join(local, name), Buffer.alloc(n, 0x61));
    utimesSync(path.join(local, name), at(t), at(t));
  }
}

/** 挑一个没人用的显示号（没有套接字、也没有锁文件）。 */
function freeDisplay() {
  for (let n = 150; n < 200; n++) {
    if (!existsSync(`/tmp/.X11-unix/X${n}`) && !existsSync(`/tmp/.X${n}-lock`)) return n;
  }
  return null;
}
