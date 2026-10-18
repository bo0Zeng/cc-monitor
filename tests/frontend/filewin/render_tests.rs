use super::*;

fn seen(api: Api, kind: Kind, name: &str) -> Seen {
    Seen {
        api,
        kind,
        name: name.into(),
    }
}

/// 🔴 挑显卡的次序：DX12 真显卡 → OpenGL 真显卡 → DX12 软件（WARP）→ OpenGL 软件。
/// 每一档都摆一个比它低的在前面（枚举次序不能替规则做主）。
#[test]
fn real_gpus_come_first_and_software_rendering_last() {
    let warp = seen(Api::Dx12, Kind::Software, "Microsoft Basic Render Driver");
    let gl_soft = seen(Api::Gl, Kind::Software, "llvmpipe (LLVM 19.1.1, 256 bits)");
    let gl_hw = seen(Api::Gl, Kind::Other, "Intel(R) HD Graphics 3000");
    let dx_hw = seen(Api::Dx12, Kind::Integrated, "AMD Radeon(TM) Graphics");
    assert_eq!(
        choose(&[gl_soft.clone(), warp.clone()]),
        Some(1),
        "两样都是软件：WARP 在 OpenGL 软件之前"
    );
    assert_eq!(
        choose(&[warp.clone(), gl_hw.clone()]),
        Some(1),
        "没有 DX12 的老核显：用它自己的 OpenGL，不掉到 WARP"
    );
    assert_eq!(
        choose(&[warp.clone(), gl_hw.clone(), dx_hw.clone()]),
        Some(2),
        "有 DX12 真显卡就用它"
    );
    assert_eq!(
        choose(std::slice::from_ref(&warp)),
        Some(0),
        "只剩 WARP：开得了，只是慢"
    );
    assert_eq!(choose(&[]), None, "一块都没有 ⇒ 不编一个");
}

/// 同一档里独立显卡在核显前；再同档按枚举次序（不洗牌）。
#[test]
fn within_a_tier_the_discrete_gpu_wins_and_order_breaks_ties() {
    let igpu = seen(Api::Dx12, Kind::Integrated, "igpu");
    let dgpu = seen(Api::Dx12, Kind::Discrete, "dgpu");
    let other = seen(Api::Dx12, Kind::Other, "other");
    assert_eq!(choose(&[igpu.clone(), dgpu.clone()]), Some(1));
    assert_eq!(
        choose(&[other.clone(), igpu.clone()]),
        Some(1),
        "说得出是核显的在说不出是什么的前面"
    );
    assert_eq!(
        choose(&[igpu.clone(), igpu.clone()]),
        Some(0),
        "同档同类取先列出的那块"
    );
}

/// 软件渲染器认名字：驱动报成「真显卡」的几样软件实现（OpenGL 那一支 wgpu 只能按渲染器名字猜，
/// 微软的 GL-on-D3D12 落在 WARP 上时名字里带 `Basic Render Driver`）照样垫底。
#[test]
fn software_renderers_are_recognised_by_name() {
    for name in [
        "Microsoft Basic Render Driver",
        "D3D12 (Microsoft Basic Render Driver)",
        "llvmpipe (LLVM 19.1.1, 256 bits)",
        "softpipe",
        "SwiftShader Device (Subzero)",
        "GDI Generic",
    ] {
        assert!(looks_software(name), "{name} 该认成软件渲染");
    }
    for name in [
        "AMD Radeon(TM) Graphics",
        "Intel(R) HD Graphics 3000",
        "NVIDIA GeForce RTX 4060",
    ] {
        assert!(!looks_software(name), "{name} 不该认成软件渲染");
    }
}

/// 日志那一行说挑了哪块、是哪一套接口哪一类、一共看见了哪几块；一块都没有时那一句带着契约里那个记号
/// （monitor 认它，见 `filewin_contract::NO_GRAPHICS_ADAPTER`）。
#[test]
fn the_log_line_names_the_pick_and_what_was_seen() {
    let all = [
        seen(Api::Dx12, Kind::Software, "Microsoft Basic Render Driver"),
        seen(Api::Gl, Kind::Other, "Intel(R) HD Graphics 3000"),
    ];
    let line = picked_line(&all, 1);
    for part in [
        "Intel(R) HD Graphics 3000",
        "gl",
        "hardware",
        "Microsoft Basic Render Driver",
        "2 seen",
    ] {
        assert!(line.contains(part), "那一行没说到 {part}：{line}");
    }
    let none = none_line(&[]);
    assert!(
        none.contains(filewin_contract::NO_GRAPHICS_ADAPTER),
        "{none}"
    );
}

/// 🔴 **虚拟机判据：没有 OpenGL 2.0 的 Windows 上，走生产开窗那一条，窗口照样画出第一帧。**
///
/// 那一形是没装显卡驱动的虚拟机、多数远程桌面会话：系统只给 OpenGL 1.1，从前的 glow 渲染器在这里开不出窗
/// （`egui_glow requires opengl 2.0+`）。Windows 改走 wgpu 之后，这里该落到系统自带的 WARP 软件光栅器，慢但开得了。
///
/// **只在虚拟机上跑**（`#[ignore]`，登记在 `test_tiers_tests.rs` 的真机层，手动）：
/// - CI 的 Windows 机器能不能开真窗口没核实：本仓从没有一条在 Windows 上开真窗口的测试，不推送就验不了；
///   公开资料说法不一（托管机器以服务身份跑、没有交互桌面 ⇒ 有的窗口测试开不出来）。真开不出来时这条会红在台架上，不是产品上。
/// - 那台机器有没有 OpenGL 2.0 也不由我们定；这条要的恰好是「没有」，Win11 虚拟机（QXL，无显卡驱动）是确定的那一形。
///
/// 跑法：交叉编出本包的 lib 测试程序（`cargo test -p cc-monitor-filewin --lib --no-run --target x86_64-pc-windows-gnu`），
/// 拷到那台 Windows，在登录用户的桌面会话里（不是 ssh 那个 session 0）跑
/// `<测试程序> --ignored --exact render::tests::without_opengl_2_the_window_still_draws_its_first_frame --nocapture`。
/// 绿 = 第一帧画出来了；红 = 原话（开窗那条线程回的错）。窗口不关，进程随测试程序一起退。
#[cfg(windows)]
#[test]
#[ignore = "虚拟机判据：要一台没有 OpenGL 2.0 的 Windows 桌面会话（跑法住它自己的头注）"]
fn without_opengl_2_the_window_still_draws_its_first_frame() {
    let log =
        std::env::temp_dir().join(format!("ccm-filewin-render-{}.frames", std::process::id()));
    let _ = std::fs::remove_file(&log);
    std::env::set_var(crate::frame_log::ENV, &log);
    let rows: Vec<crate::source::Row> = (0..200)
        .map(|i| crate::source::Row {
            name: format!("f{i:04}.rs"),
            path: format!("/srv/render/f{i:04}.rs"),
            is_dir: false,
            size: i,
            lossy_name: false,
        })
        .collect();
    let rows = crate::find::testing::shown_of(&rows);
    let h = crate::shell::open_detached_seeded(
        crate::source::Source::remote("render-judge".into()),
        "/srv/render".into(),
        None,
        None,
        rows,
        None,
        None,
        None,
        Vec::new(),
        None,
        None,
    );
    let t0 = std::time::Instant::now();
    let frames = || {
        std::fs::read_to_string(&log)
            .map(|s| s.lines().count())
            .unwrap_or(0)
    };
    while frames() == 0 && !h.is_finished() && t0.elapsed() < guard_core::wall::CEILING {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if frames() == 0 && h.is_finished() {
        let why = match h.join() {
            Ok(Ok(())) => "窗口没画一帧就关了".to_string(),
            Ok(Err(e)) => e.logged(),
            Err(_) => "开窗那条线程 panic 了".to_string(),
        };
        panic!("没有 OpenGL 2.0 的 Windows 上窗口没开出来：{why}");
    }
    assert!(frames() > 0, "等了 {:?} 也没画出第一帧", t0.elapsed());
    println!("render.first_frame_ms={}", t0.elapsed().as_millis());
}
