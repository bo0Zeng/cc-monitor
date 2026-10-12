//! 渲染器挑哪块显卡（只 Windows 用得上；规则是纯的，各平台都判）。

/// 哪一套图形接口。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    Dx12,
    Gl,
}

/// 这块是什么。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Discrete,
    Integrated,
    Other,
    Software,
}

/// 枚举出来的一块。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub api: Api,
    pub kind: Kind,
    pub name: String,
}

/// 🔴 **挑哪块：只住这一个函数。** 真显卡在前、软件渲染垫底：
/// DX12 真显卡 → OpenGL 真显卡（没有 DX12 的老核显用它自己的 OpenGL 驱动，不掉到 WARP 变慢）→
/// DX12 软件（WARP：没装驱动的虚拟机 · 远程桌面，开得了、慢）→ OpenGL 软件。
/// 同一档里独立显卡 → 核显 → 说不出是什么的；再相同取先列出的那块。一块都没有 ⇒ `None`。
pub fn choose(seen: &[Seen]) -> Option<usize> {
    let key = |s: &Seen| {
        let tier = match (s.api, s.kind == Kind::Software) {
            (Api::Dx12, false) => 0,
            (Api::Gl, false) => 1,
            (Api::Dx12, true) => 2,
            (Api::Gl, true) => 3,
        };
        let sub = match s.kind {
            Kind::Discrete => 0,
            Kind::Integrated => 1,
            Kind::Other | Kind::Software => 2,
        };
        (tier, sub)
    };
    (0..seen.len()).min_by_key(|&i| key(&seen[i]))
}

/// 名字说它是软件渲染的那几样（驱动自己报的类型不可信时按名字认：wgpu 的 OpenGL 那一支只能猜，
/// 微软的 GL-on-D3D12 落在 WARP 上时也报成「显卡」）。
pub fn looks_software(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "basic render driver",
        "llvmpipe",
        "softpipe",
        "swiftshader",
        "gdi generic",
    ]
    .iter()
    .any(|s| n.contains(s))
}

fn api_word(a: Api) -> &'static str {
    match a {
        Api::Dx12 => "dx12",
        Api::Gl => "gl",
    }
}

fn kind_word(k: Kind) -> &'static str {
    match k {
        Kind::Discrete => "hardware, discrete",
        Kind::Integrated => "hardware, integrated",
        Kind::Other => "hardware",
        Kind::Software => "software",
    }
}

fn listed(all: &[Seen]) -> String {
    all.iter()
        .map(|s| format!("{} ({}, {})", s.name, api_word(s.api), kind_word(s.kind)))
        .collect::<Vec<_>>()
        .join("; ")
}

/// 挑中之后日志里那一行（窗口进程的 stderr，monitor 接进它的日志）。
pub fn picked_line(all: &[Seen], i: usize) -> String {
    let s = &all[i];
    format!(
        "filewin renderer: picked {} ({}, {}); {} seen: {}",
        s.name,
        api_word(s.api),
        kind_word(s.kind),
        all.len(),
        listed(all)
    )
}

/// 一块都挑不出时那一句（经 eframe 成为开窗失败的原话，带着契约里那个记号）。
pub fn none_line(all: &[Seen]) -> String {
    format!(
        "{} (dx12, gl); {} seen: {}",
        filewin_contract::NO_GRAPHICS_ADAPTER,
        all.len(),
        listed(all)
    )
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/render_tests.rs"]
mod tests;
