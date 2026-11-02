//! typed 命令的形状（入参 · 回包的 TS 声明 ＋ 码表）与它的指纹：测试档才要（本包的测试档 · 上层经 `test-support`）。
//! 本包登记出来的那一份不带形状（[`super::Typed`] 的形状格是 `None`）：TS 界只在 [`shape_of`] 这一处要，
//! 登记那一行在主 crate 的测试档经 [`shaped`] 补上（`stream/inbound/typed.rs`）。
//! 读形状的判据（形状表 · 界面命令表 · 协议参考）挂主 crate（`typed_tests.rs` · `protocol_doc_gen.rs`）。

use super::{Codes, Command, How, Typed};
use crate::wire::spec::CommandSpec;

impl Typed {
    /// 开跑之后打不打得断（阻塞档打不断）：读它的只有形状指纹与协议文档（撤不动的算进指纹，`cmd_wire::shapes::UNCANCELLABLE` 由它写）。
    pub fn blocking(&self) -> bool {
        matches!(self.how, How::Blocking | How::FrameBlocking)
    }

    /// 这条命令的形状。登记那一行没经主 crate 测试档的补形状那一层 ⇒ 当场说出来（不当成空形状）。
    pub fn shape(&self) -> Shape {
        match self.shaped {
            Some(f) => f(),
            None => panic!("这一行登记没补形状：登记行要经主 crate 的 `stream/inbound/typed.rs`"),
        }
    }
}

/// 给一行登记补上形状（主 crate 测试档的登记那一层调它）。
pub const fn shaped(mut spec: CommandSpec, shape: fn() -> Shape) -> CommandSpec {
    spec.run.shaped = Some(shape);
    spec
}

/// 一条 typed 命令的形状（测试档）：入参 · 回包的 TS 名，连同它们引到的每个具名类型的 TS 声明，＋ 码表。
pub struct Shape {
    pub req: String,
    pub reply: String,
    pub codes: &'static [&'static str],
    /// `(TS 名, 声明)`，按名排、去重：入参与回包本身 ＋ 它们递归引到的每个具名类型。
    pub decls: Vec<(String, String)>,
    /// 开跑之后撤不动（阻塞档）。登记那一档定，读形状表的那一处填；算进指纹：两端指纹对得上 ⇒ 撤不撤得动也一致。
    pub uncancellable: bool,
    /// 带 `data` 的那几个码 ⇒ `data` 的 TS 名（码表次序）；算进指纹：两端读 `data` 的形状对不上 ⇒ 指纹就对不上。
    pub data: Vec<(&'static str, String)>,
    /// 一份入参照这条命令的入参类型解（与分派同一个解码 [`super::take`]）；解不开 ⇒ 那一句。
    pub decode: fn(&serde_json::Value) -> Result<(), String>,
}

fn decode_of<C: Command>(args: &serde_json::Value) -> Result<(), String> {
    super::take::<C::Req>(args)
        .map(drop)
        .map_err(|f| format!("{}：{}", f.said, f.raw.unwrap_or_default()))
}

pub use cmd_wire::fingerprint::{ty_of, Ty};

/// [`super::Codes::Data`] 的那一份 TS 形状：元组逐位一个；[`super::Each`] ＝ 每个码同一形（`n` ＝ 码数）。
pub trait DataTys {
    fn tys(n: usize) -> Vec<Ty>;
}

impl<T: ts_rs::TS + 'static> DataTys for super::Each<T> {
    fn tys(n: usize) -> Vec<Ty> {
        (0..n).map(|_| ty_of::<T>()).collect()
    }
}

macro_rules! data_tys {
    ($($t:ident),*) => {
        impl<$($t: ts_rs::TS + 'static),*> DataTys for ($($t,)*) {
            fn tys(_: usize) -> Vec<Ty> {
                vec![$(ty_of::<$t>()),*]
            }
        }
    };
}
data_tys!();
data_tys!(A);
data_tys!(A, B);
data_tys!(A, B, C);
data_tys!(A, B, C, D);
data_tys!(A, B, C, D, E);
data_tys!(A, B, C, D, E, F);

/// 一条命令的形状。TS 那道界只写在这里：入参 · 回包 · 码的 `data` 都得算得出 TS 形状。
pub fn shape_of<C: Command>() -> Shape
where
    C::Req: ts_rs::TS,
    C::Reply: ts_rs::TS,
    <C::Err as Codes>::Data: DataTys,
{
    let (req, reply) = (ty_of::<C::Req>(), ty_of::<C::Reply>());
    let mut decls = req.decls;
    decls.extend(reply.decls);
    let codes = <C::Err as Codes>::DATA;
    let tys = <<C::Err as Codes>::Data as DataTys>::tys(codes.len());
    assert_eq!(
        tys.len(),
        codes.len(),
        "`{}`：带 `data` 的码与 `data` 的类型对不上位",
        C::NAME
    );
    let mut data = Vec::new();
    for (code, ty) in codes.iter().zip(tys) {
        decls.extend(ty.decls);
        data.push((*code, ty.name));
    }
    Shape {
        req: req.name,
        reply: reply.name,
        codes: <C::Err as Codes>::ALL,
        decls: decls.into_iter().collect(),
        uncancellable: false,
        data,
        decode: decode_of::<C>,
    }
}

impl Shape {
    /// 形状指纹：名 · 声明 · 码表逐行拼起来的 FNV-1a 32 位，8 位十六进制（两端比的就是它）。
    pub fn fingerprint(&self) -> String {
        let mut text = format!(
            "req {}\nreply {}\ncodes {}\ncancel {}\n",
            self.req,
            self.reply,
            self.codes.join(","),
            if self.uncancellable { "no" } else { "yes" }
        );
        // 带 `data` 的码：没有就不写这一行（不带 `data` 的命令指纹不变）。
        if !self.data.is_empty() {
            let d: Vec<String> = self.data.iter().map(|(c, t)| format!("{c}={t}")).collect();
            text.push_str(&format!("data {}\n", d.join(",")));
        }
        for (name, decl) in &self.decls {
            text.push_str(&format!("{name}\n{decl}\n"));
        }
        cmd_wire::fingerprint::fnv32(&text)
    }
}
