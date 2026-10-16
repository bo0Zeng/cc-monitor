//! 形状指纹的算法（命令表 [`crate::shapes`] · 帧表 [`crate::frame_shapes`] 同一个）：一个类型的 TS 声明连同它递归引到的每个具名类型，
//! 逐行拼起来的 FNV-1a 32 位，8 位十六进制。只在测试档（或开 `ts`）里有：两张表由判据从类型现算写出。

use std::collections::BTreeMap;

/// 一个类型的 TS 名，连同它与它递归引到的每个具名类型的声明（按名排、去重）。
pub struct Ty {
    pub name: String,
    pub decls: BTreeMap<String, String>,
}

struct Collect<'a>(&'a ts_rs::Config, BTreeMap<String, String>);

impl ts_rs::TypeVisitor for Collect<'_> {
    fn visit<T: ts_rs::TS + 'static + ?Sized>(&mut self) {
        if T::output_path().is_none() {
            return; // 内联的（基本类型 · 容器）：形状在引它的那份声明里
        }
        let name = T::name(self.0);
        if self.1.contains_key(&name) {
            return;
        }
        self.1.insert(name, T::decl(self.0));
        T::visit_dependencies(self);
    }
}

/// `T` 的 [`Ty`]。
pub fn ty_of<T: ts_rs::TS + 'static>() -> Ty {
    let cfg = ts_rs::Config::default();
    let mut c = Collect(&cfg, Default::default());
    ts_rs::TypeVisitor::visit::<T>(&mut c);
    // 是 `()` 之类内联类型 ⇒ 上面那一下没记；它自己的依赖照样走一遍。
    T::visit_dependencies(&mut c);
    Ty {
        name: T::name(&cfg),
        decls: c.1,
    }
}

/// 一段文字的 FNV-1a 32 位，8 位十六进制。
pub fn fnv32(text: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in text.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("{h:08x}")
}

/// 一个类型的形状指纹：名 ＋ 每份声明逐行拼起来。
pub(crate) fn of_ty(ty: &Ty) -> String {
    let mut text = format!("{}\n", ty.name);
    for (name, decl) in &ty.decls {
        text.push_str(&format!("{name}\n{decl}\n"));
    }
    fnv32(&text)
}
