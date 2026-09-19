/// ★★ **写侧只许有一个出口**〔audit-0805 08-08，Phase G 第 61 件，SS-14〕。
///
/// 本文件头注与 `mcp_json_path` 的注释都写着「写侧唯一出口，硬编码 `.mcp.json`，
/// 杜绝误写 `~/.claude.json` / `settings.json`」，并且逐字提到
/// 「**grep 门禁：本文件写路径只此一处**」。
///
/// ⇒ **那条门禁不存在**。08-08 实测：在本文件加一个
/// `fn settings_json_path(dir) -> …join("settings.json")`，**全仓 989 条判据一条不红**。
/// 与 F58 同族（散文指着一条并不存在／已被删掉的机检），只是这次它从未存在过。
///
/// # 钉法
///
/// 人群 = 生产段里**每一处把文件名拼进项目目录**的地方（`join("…")` / `format!("{d}/…")`），
/// 逐个要求那个文件名是 `.mcp.json`。**读侧那一处也算**（它同样只许碰 `.mcp.json`；
/// 头注第 2 行「读：项目 `<dir>/.mcp.json`」说的就是它）。
#[test]
fn every_project_file_name_this_module_builds_is_mcp_json() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/mcp.rs"));
    // 运行时拼，免得命中本条自己的诊断文案。
    let want = format!(".mcp{}", ".json");
    let mut sites: Vec<(usize, String)> = Vec::new();
    for (i, line) in prod.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        // ⚠ **判准是「接收者是不是项目目录」**，不是「有没有 join」。
        //   第一版只看 `join("…")`，于是把读用户配置那三处
        //   （`home.join(".claude.json")` 等）也圈了进来 —— 而头注第 2 行逐字写着
        //   「读：用户 `~/.claude.json` 顶层」，那是**合法的读**。
        //   人群取宽会逼人把合法写法加进豁免，那正是把判据变成废纸的路。
        //   项目侧的形态只有两种：`Path::new(<项目目录变量>).join("…")` 与 `format!("{d}/…")`。
        if let Some(a) = t.find("Path::new(") {
            let after = &t[a..];
            if let Some(j) = after.find(".join(\"") {
                let rest = &after[j + ".join(\"".len()..];
                if let Some(b) = rest.find('"') {
                    sites.push((i + 1, rest[..b].to_string()));
                }
            }
        }
        if let Some(a) = t.find("format!(\"{d}/") {
            let rest = &t[a + "format!(\"{d}/".len()..];
            if let Some(b) = rest.find('"') {
                sites.push((i + 1, rest[..b].to_string()));
            }
        }
    }
    // 抽取器自检：一处都没抓到 ⇒ 下面整条空转。
    assert!(
        sites.len() >= 3,
        "只抓到 {} 处「往项目目录里拼文件名」（08-08 实测 3：读侧 1 + 本机写侧 1 + 远端写侧 1）\
             —— 抽取器坏了，本条此刻无效：{sites:?}",
        sites.len()
    );
    for (line, name) in &sites {
        assert_eq!(
            name, &want,
            "第 {line} 行往项目目录里拼的是 {name:?}，不是 {want:?}。\n\
                 ⚠ SS-14 铁律：本模块**只**碰 `<dir>/.mcp.json`，绝不写 `~/.claude.json` / \
                 `settings.json`。\n\
                 头注里那句「grep 门禁：本文件写路径只此一处」**在 08-08 之前是不存在的** —— \
                 本条就是那条门禁。真要碰别的文件，先改头注那条铁律，别悄悄加一个出口。"
        );
    }
}
use super::*;
use serde_json::json;

#[test]
fn collect_merges_three_scopes() {
    let claude = json!({
        "mcpServers": { "user-srv": { "command": "u" } },
        "projects": { "/proj": { "mcpServers": { "local-srv": { "command": "l" } } } }
    });
    let proj = json!({ "mcpServers": { "proj-srv": { "type": "http", "url": "x" } } });
    let out = collect_entries(Some(&claude), "cj", Some(&proj), "mcp", Some("/proj"));
    assert_eq!(out.len(), 3);
    let scopes: Vec<&str> = out.iter().map(|e| e.scope.as_str()).collect();
    assert!(scopes.contains(&"user") && scopes.contains(&"local") && scopes.contains(&"project"));
    // server 原样保留
    let proj_e = out.iter().find(|e| e.scope == "project").unwrap();
    assert_eq!(proj_e.server["url"], json!("x"));
}

#[test]
fn remote_read_takes_user_scope_only() {
    // F87b③：跨机读的契约——`collect_entries(cj, src, None, "", None)` 只出 user scope
    // （顶层 mcpServers = 机器全局 MCP）。有 projects（local scope 候选）也不取（project_dir=None）。
    let remote_claude = json!({
        "mcpServers": { "global-a": { "command": "a" }, "global-b": { "type": "http", "url": "u" } },
        "projects": { "/remote/proj": { "mcpServers": { "local-x": { "command": "x" } } } }
    });
    let out = collect_entries(Some(&remote_claude), "[pi] ~/.claude.json", None, "", None);
    assert_eq!(out.len(), 2, "只取 user scope 两条，不含 local/project");
    assert!(out.iter().all(|e| e.scope == "user"));
    let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"global-a") && names.contains(&"global-b"));
    assert!(!names.contains(&"local-x"), "local scope 不该跨机取");
    // 来源路径标了 origin（前端据此显只读远端来源）
    assert!(out.iter().all(|e| e.source_path == "[pi] ~/.claude.json"));
}

#[test]
fn collect_tolerant_missing_and_bad_shapes() {
    // 全 None → 空
    assert!(collect_entries(None, "", None, "", None).is_empty());
    // mcpServers 非对象 → 跳过不崩
    let claude = json!({ "mcpServers": "not-an-object" });
    assert!(collect_entries(Some(&claude), "cj", None, "", Some("/p")).is_empty());
    // 有 user 无 local（projects 缺该 dir）
    let claude2 = json!({ "mcpServers": { "u": {} }, "projects": {} });
    let out = collect_entries(Some(&claude2), "cj", None, "", Some("/p"));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].scope, "user");
}

#[test]
fn write_and_remove_only_touch_mcp_json() {
    let tmp = std::env::temp_dir().join(format!("ccm-mcp-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let dir = tmp.to_string_lossy().into_owned();
    // 放一个假 .claude.json 在临时目录，断言写后它不变
    let fake_claude = tmp.join(".claude.json");
    std::fs::write(&fake_claude, "{\"mcpServers\":{\"keep\":{}}}").unwrap();

    // 写：建骨架 + 加条目（测同步 _impl；命令是 async 薄封装）
    write_project_mcp_server_impl(dir.clone(), "srv".into(), json!({ "command": "c" })).unwrap();
    let mcp = tmp.join(".mcp.json");
    assert!(mcp.is_file());
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
    assert_eq!(v["mcpServers"]["srv"]["command"], json!("c"));
    // .claude.json 一字未动
    assert_eq!(
        std::fs::read_to_string(&fake_claude).unwrap(),
        "{\"mcpServers\":{\"keep\":{}}}"
    );

    // 删
    remove_project_mcp_server_impl(dir.clone(), "srv".into()).unwrap();
    let v2: Value = serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
    assert!(v2["mcpServers"].get("srv").is_none());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn write_rejects_empty_project_dir_and_nonexistent() {
    assert!(write_project_mcp_server_impl("  ".into(), "n".into(), json!({})).is_err());
    assert!(mcp_json_path("").is_err());
    // 项目目录不存在 → 拒写（不 create_dir_all typo 路径）
    let ghost = std::env::temp_dir().join(format!("ccm-mcp-ghost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ghost);
    let r = write_project_mcp_server_impl(
        ghost.to_string_lossy().into_owned(),
        "n".into(),
        json!({ "command": "c" }),
    );
    assert!(r.is_err(), "不存在的项目目录应拒写");
    assert!(!ghost.exists(), "拒写后不该建出 typo 目录");
}

#[test]
fn project_dirs_from_sorted_and_tolerant() {
    let cj = json!({ "projects": { "/b": {}, "/a": {} } });
    assert_eq!(
        project_dirs_from(&cj),
        vec!["/a".to_string(), "/b".to_string()]
    );
    assert!(project_dirs_from(&json!({})).is_empty()); // 缺 projects
    assert!(project_dirs_from(&json!({ "projects": "bad" })).is_empty()); // 非对象
}

#[test]
fn remote_mcp_path_guard_rejects_traversal_and_nonabsolute() {
    // F89a：远端写路径守卫——只接受 绝对 + 尾 /.mcp.json + 无 .. 的非裸路径。
    assert_eq!(
        remote_mcp_json_path("/home/pi/proj").unwrap(),
        "/home/pi/proj/.mcp.json"
    );
    assert_eq!(
        remote_mcp_json_path("/home/pi/proj/").unwrap(), // 尾斜杠归一
        "/home/pi/proj/.mcp.json"
    );
    assert!(remote_mcp_json_path("relative/proj").is_err()); // 非绝对
    assert!(remote_mcp_json_path("").is_err()); // 空
    assert!(remote_mcp_json_path("/a/../../etc").is_err()); // 路径穿越 → 拼后含 ..
    assert!(remote_mcp_json_path("/").is_err()); // 根 → 拼成 /.mcp.json 被守卫拒（裸）
                                                 // 守卫本体：只 /.mcp.json 结尾、无 .. 、绝对、非裸
    assert!(is_safe_remote_mcp_json("/x/y/.mcp.json"));
    assert!(!is_safe_remote_mcp_json("/.mcp.json")); // 裸（无项目段）
    assert!(!is_safe_remote_mcp_json("/x/.claude.json")); // 非 .mcp.json（SS-14 铁）
    assert!(!is_safe_remote_mcp_json("/x/settings.json")); // 非 .mcp.json
    assert!(!is_safe_remote_mcp_json("/x/../y/.mcp.json")); // 穿越
    assert!(!is_safe_remote_mcp_json("x/.mcp.json")); // 非绝对
}

#[test]
fn upsert_and_remove_value_cores() {
    // F89a：本机/远端复用的纯核心——upsert/remove Value 变换。
    let mut root = json!({ "mcpServers": { "a": { "command": "x" } } });
    upsert_mcp_server_value(&mut root, "b".into(), json!({ "type": "http", "url": "u" })).unwrap();
    assert_eq!(root["mcpServers"]["a"]["command"], json!("x")); // 原有不动
    assert_eq!(root["mcpServers"]["b"]["url"], json!("u")); // 新增
    upsert_mcp_server_value(&mut root, "a".into(), json!({ "command": "y" })).unwrap();
    assert_eq!(root["mcpServers"]["a"]["command"], json!("y")); // 同名覆盖
    assert!(upsert_mcp_server_value(&mut root, "  ".into(), json!({})).is_err()); // 名空拒
                                                                                  // 骨架缺 mcpServers → upsert 自建
    let mut skel = json!({});
    upsert_mcp_server_value(&mut skel, "c".into(), json!({})).unwrap();
    assert!(skel["mcpServers"]["c"].is_object());
    // remove
    assert!(remove_mcp_server_value(&mut root, "a").unwrap()); // 真删
    assert!(root["mcpServers"].get("a").is_none());
    assert!(!remove_mcp_server_value(&mut root, "nope").unwrap()); // 不存在 → false
}
