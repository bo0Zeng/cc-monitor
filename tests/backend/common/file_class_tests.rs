//! 按名字判一项是哪一种：扩展名不分大小写 · 无扩展名的代码名 · 只有前导点的不算扩展名 · 目录先于名字。

use super::*;
use FileClass as C;

#[test]
fn names_map_to_their_class() {
    for (name, want) in [
        ("login.CSS", C::Code),
        ("Makefile", C::Code),
        ("notes.md", C::Text),
        ("shot.png", C::Image),
        ("a.tar.gz", C::Archive),
        ("spec.pdf", C::Pdf),
        (".bashrc", C::Other),
        ("blob", C::Other),
    ] {
        assert_eq!(class_of_name(name.as_bytes(), false), want, "{name}");
    }
    assert_eq!(class_of_name(b"shot.png", true), C::Folder);
    assert_eq!(ext_of(b"A.Tar.GZ").as_deref(), Some("gz"));
    assert_eq!(ext_of(b"\xff.\xfe"), None);
}
