//! F08a：daemon 只读机器护栏（主计划红线 I7 的机器化守护）。
//!
//! daemon 对被观测文件系统（`~/.claude` 等）**必须只读**——只 watch/scan/read，绝不写。
//! 唯一合法的「写」是把 wire 帧写 **stdout**（`main.rs` 的 `AsyncWriteExt::write_all`，非 FS）。
//! 本护栏遍历 daemon 生产源码，剥掉 `#[cfg(test)]` 块（测试夹具可用 temp 目录）后，断言不含任何
//! **文件系统变更**调用。加只读测试是红线 I7 明确允许的（「daemon 只准加只读测试/门禁」）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销、不改 daemon 行为。

#[cfg(test)]
mod tests {
    /// 剥掉所有 `#[cfg(test)]` 属性修饰的花括号块（按括号配平跳过其后第一个 `{...}`）。
    /// 不能简单「从首个 `#[cfg(test)]` 截断到 EOF」——`main.rs` 的测试模块在文件**中部**，
    /// 其后仍有生产代码（`main`/`writer_task`/`write_frame`）。按块剥除才不误伤生产段。
    /// 字节索引均落在 `#`/`{`/`}` 这些 ASCII 边界上，切片对 UTF-8（中文注释）安全。
    ///
    /// **已知局限**（本护栏是纵深防御、非严格证明，不值当为它塞个 Rust 词法器）：括号配平不识别
    /// 字符串/注释里的 `{`/`}`，若某 `#[cfg(test)]` 块内有含不配对花括号的字符串字面量，剥除边界会
    /// 偏。偏向**保守**（少剥）→ 残留测试代码进扫描 → 顶多假阳性（CI 红、人一看是测试代码即排除，
    /// fail-closed 安全）；真正危险的假阴性（多剥、吞掉生产 `fs::write`）需要生产 `fs::write` 紧跟在
    /// 一个花括号不配对的 cfg(test) 块之后——现有 daemon 源无此形态，且真加生产写操作时该模式亦罕见。
    fn strip_cfg_test(src: &str) -> String {
        let mut out = String::new();
        let mut rest = src;
        while let Some(pos) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..pos]);
            let after = &rest[pos..];
            match after.find('{') {
                Some(brace) => {
                    let bytes = after.as_bytes();
                    let mut depth: i32 = 0;
                    let mut end = brace;
                    while end < after.len() {
                        match bytes[end] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end += 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        end += 1;
                    }
                    rest = &after[end..]; // 跳过整个 cfg(test) 块
                }
                None => {
                    // `#[cfg(test)]` 修饰的不是块（如 `use`）——只跳过属性本身，保留其余。
                    rest = &after["#[cfg(test)]".len()..];
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// 文件系统**变更**模式。用 `fs::`/`File::`/`OpenOptions` 命名空间锚定，故 stdout 的
    /// `AsyncWriteExt::write_all`（trait 方法、非 `fs::`）天然不匹配 = 合法放行。
    const FS_MUTATION_PATTERNS: &[&str] = &[
        "fs::write",
        "fs::create_dir",
        "fs::remove_file",
        "fs::remove_dir",
        "fs::rename",
        "fs::copy",
        "fs::hard_link",
        "fs::soft_link",
        "File::create",
        "File::options",
        "OpenOptions",
    ];

    #[test]
    fn daemon_production_code_is_filesystem_read_only() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = 0usize;
        for entry in std::fs::read_dir(&src_dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // 跳过本护栏文件自身——它的 FS_MUTATION_PATTERNS 字面量数组含这些子串。
            if path.file_name().and_then(|n| n.to_str()) == Some("readonly_guard.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("read rs file");
            let prod = strip_cfg_test(&src);
            for pat in FS_MUTATION_PATTERNS {
                assert!(
                    !prod.contains(pat),
                    "daemon 只读护栏违规（红线 I7）：生产代码 {} 含文件系统写操作 `{}`。\n\
                     daemon 必须只读；如确需临时文件，放进 #[cfg(test)] 块内。",
                    path.display(),
                    pat
                );
            }
            scanned += 1;
        }
        assert!(
            scanned >= 5,
            "扫描到的 daemon 源文件过少（{scanned}），护栏可能没生效"
        );
    }
}
