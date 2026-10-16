use super::*;

/// 标准 FNV-1a 的公开测试向量（与本文件的算法异源）。
#[test]
fn standard_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    assert_eq!(fnv1a32(b""), 0x811c_9dc5);
    assert_eq!(fnv1a32(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
}

/// 分段喂与一次喂同值。
#[test]
fn feeding_in_parts_is_feeding_once() {
    assert_eq!(fnv1a64_from(fnv1a64(b"foo"), b"bar"), fnv1a64(b"foobar"));
}

/// 宽乘数那一种：钉一个另行独立算出的数（目录锁的互斥量名、旧版口号靠它，换了就和别的进程对不上）。
#[test]
fn wide_prime_value_is_pinned() {
    assert_eq!(
        fnv1a64_wide(b"c:\\users\\a\\.cc-monitor"),
        0x674a_df81_a80d_ede8
    );
}
