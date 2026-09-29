# vendored: russh 0.61.1（补过 zlib 解压的一份）

用户裁决 **V118**〔选〕「打补丁版 russh，现在就开」：vendor 一份修好 zlib 解压的 russh（`[patch.crates-io]`），
SSH 压缩按已写好的判准开。缺陷的现打与判准住 `调研/第四波记录/NT1.md §1.1a`；本副本的设计与读数住 `调研/第四波记录/CZ1.md`。

## 来源

- **crates.io 上的 `russh 0.61.1`**，取自本机 cargo 缓存 `~/.cargo/registry/cache/index.crates.io-*/russh-0.61.1.crate`
  —— 它的 sha256 `f67013f080c226e5a34db1c71f2567f44d95a6300005bb6cd4e2c8fe3c326d1b` **等于**本仓 `src/backend/Cargo.lock`
  当时锁的 checksum（取的就是 lock 锁的那一份）。上游仓 `https://github.com/warp-tech/russh`，
  `.cargo_vcs_info.json` 记的上游提交 `a2ffd0fafd2522793761b12ebc9496007e4b287a`（`path_in_vcs = russh`）。
- `.crate` 里的**全部文件原样**（含 `tests/` `examples/` `benches/`），只少一份 `Cargo.lock`
  （被仓根 `.gitignore` 的 `src/vendor/**/Cargo.lock` 挡着；path 依赖的 lock 本来不参加解析）。
- 许可 **Apache-2.0**（`Cargo.toml` 的 `license`）。`.crate` 里没带许可全文 ⇒ 本目录的 `LICENSE-APACHE` 是副本特有的一份
  （Apache-2.0 §4(a)「随分发附一份许可」），正文取自本机缓存 `serde-1.0.228/LICENSE-APACHE`（同一份 Apache-2.0 条款）。

## 改了什么（只此一处：`src/compression.rs::Decompress::decompress`）

上游那一段（原样）：

```rust
loop {
    let n_in_ = z.total_in() as usize - n_in;      // ← 本轮调用之前的进度
    let n_out_ = z.total_out() as usize - n_out;
    let d = z.decompress(&input[n_in_..], &mut output[n_out_..], flush);
    match d? {
        flate2::Status::Ok | flate2::Status::BufError => {
            let consumed_all_input = n_in_ == input.len();   // ← 判断用的还是调用之前的
            let output_full = n_out_ == output.len();
            if !output_full && consumed_all_input { break; }
            ...
```

**缺陷**：收尾判断用的是本轮调用**之前**的进度。输出缓冲起步 = 包长 L：第 1 次调用吃光输入、填满 L，判断看的是 `(0, 0)` ⇒ 继续；
第 2 次调用没空位（`BufError`），判断看的是 `(全部输入, L)` ⇒ 满 ⇒ 扩成 2L；第 3 次调用填满 2L，判断看的还是 `(全部输入, L)`
⇒「没满、输入吃光」⇒ **收工**。一包最多交出 2L，余下的留在解压器里、拼进下一包 ⇒ 包流错位。
可压比 > 2 的包（会话 jsonl 3–5 倍）几乎每一包都中；真 sshd（`OpenSSH_10.2p1`，`zlib@openssh.com`）上一开压缩，
第一条通道的确认就被吃掉、会话卡死（`调研/第四波记录/NT1.md §1.1a` 现打）。`0.61.2` 的 `compression.rs` 与 `0.61.1` 逐字相同，升级不解决。

**补丁**（`git log --follow -p -- src/vendor/russh/src/compression.rs` 看得到逐行，共 +11 行、0 行删）：

1. 调用之后重取 `n_in_` / `n_out_`（遮蔽调用前那两个），收尾判断用它们；
2. 多一格「还有空位而这一轮一字未动 ⇒ 收工」防空转（输入被截断时解压器在等不存在的下文；原码在这一形上同样会空转，
   改用调用后的进度之后要自己挡）；
3. 改处挂 `CZ1 PATCH` 起头的醒目注释（Apache-2.0 §4(b)：被改过的文件要写明改过）。

`Cargo.toml` **不改**（`version` 仍 `0.61.1`：`[patch.crates-io]` 要求副本版本满足后端的依赖声明）。
上游自带的「解出来的一包不许超过 `MAXIMUM_DECOMPRESSED_PACKET_LEN`」那两道检查（扩缓冲封顶 · 收尾报 `PacketSize`）一字未动。

## 怎么接的

- 后端 `src/backend/Cargo.toml`：依赖那一行不动（`russh = { version = "0.61.1", default-features = false, features = ["ring", "flate2", "rsa"] }`），
  末尾 `[patch.crates-io] russh = { path = "../vendor/russh" }`。lock 的差只有 russh 那一块丢了 `source` / `checksum`（= path 来源），
  其余包一个版本都没动（`crypto-bigint 0.7.3` 那一形照旧：别让 cargo 重解析，见后端 `Cargo.toml` 头注）。
- 闸 `src/backend/dial/connect.rs::RUSSH_ZLIB_SOUND` 开（`true`）：判准（`compression_for`）的答案落到连接上。
- **不进任何 workspace**：monitor 的 `[workspace]` 不列它、也没有成员依赖它；它只作为后端（独立 crate）的 path 依赖被编。
  它自带的 `tests/` `examples/` `benches/` 原样留着、**不在任何门里跑**（跑它们要 dev 依赖，断网解析不动）。
- ⚠ path 依赖**不压 lint**（registry 来的依赖 cargo 会 `--cap-lints`，path 的不会）⇒ 编后端时会多看见一条上游自带的警告
  （`src/server/encrypted.rs` 的 `unused import: super::super::*`，服务端那半、我们不编它的用法）。**不在副本里修**：修了就是改出自己的版本。

## 判据（住 `tests/backend/dial_compress_tests.rs`，后端 `cargo test` 那一格跑）

| 判据 | 判什么 |
|---|---|
| `the_gate_matches_what_russh_really_does`（Z5） | 闸 == russh 自己的 zlib 一来一回对不对（两向）；单包 ＋ 同一对压 / 解器连走三包 |
| `the_vendored_russh_differs_from_the_crate_only_where_registered`（V1） | 盘上文件集合 == 下面三张表（两向）；每一份的 sha256 == 登记；改过的恰好 `{src/compression.rs}` |
| `the_russh_patch_is_really_wired`（V2） | `[patch.crates-io]` 恰好这一条 · 声明 / 副本 / lock 三处版本相等 · lock 那一块没有 `source` |
| `zr_real_sshd_…`（ZR，`#[ignore]`，`tests/evidence/NT1-net-loopback.py --compress` 触发） | 真 sshd 上强开压缩：载荷逐字节同、线上字节 < 不压那趟的一半 |

改补丁 ⇒ 同拍改下面「改过的文件」那一行的补后指纹（`sha256sum src/compression.rs`），否则 V1 红。

## 上游修好之后怎么撤

1. 核上游新版的 `Decompress::decompress` 真修了：先把后端清单的 russh 升到那一版、**暂不删补丁**，
   把 `[patch.crates-io]` 两行注释掉跑 Z5 —— 绿（闸开 == 解压对）才算上游修好；红就是没修，别撤。
2. 删 `[patch.crates-io]` 那两行（连同块头注释）与本目录 `src/vendor/russh/`；lock 跟着升级落回 registry 来源。
3. 删 V1 / V2 两条判据与后端 `[dev-dependencies]` 里只为 V1 加的 `sha2`（`readonly_guard` 签字表那一行同拍摘）。
4. Z5 与闸原样留着（它们守的是「闸 == russh 实况」，与 russh 从哪来无关）。

## 原样文件清单（sha256，逐份取自 `.crate`）

<!-- 原样清单 起 -->
```text
ef7a80da88391c6812e72c73d73f2840c38e857db19a8ba1ee7b160beb269f1c  .cargo_vcs_info.json
3fbc31c2518da5a746cc15f2f6551def835b0f03e74db194ca5ce2a97b9d0457  Cargo.toml
d0cca52f0c3e671d0bbe37cbd1f6caa2cefc2492ca4a0e0a85aa25a2669fab30  Cargo.toml.orig
fdeab9b31178d54977f8629bd32db7e6c3abe5a05e1229be05fcff9f8b1bb1cf  README.md
8eed524c8e6d71b62ef484a6f75ea7695a5d2bb0d1824e430f8793d41e7d0b6f  benches/ciphers.rs
485a4fa3bb6e0a590e5cd03985a51aba79d06e0088bda0f90c4a69ae92c7993f  examples/client_exec_interactive.rs
1e7158ea8a1fba454a960ca96bf9294d984ee1492a21e4a3450c321187910db2  examples/client_exec_simple.rs
54f2404bb607adb13bfaa227755a760fb7f4d24ef32b215c34b0262fc36c714a  examples/client_open_direct_tcpip.rs
eb4b1a7ba82c059e43d34eba6581f6129489a803b64d46f41edd5bcd0cb93816  examples/echoserver.rs
dc746002c3387f091a72ba198043664762c9e45b9fdd2aa580ead0dff73a3f65  examples/ratatui_app.rs
6859da0679e1397d2bb0f97992d6f0ec47f813f5b1a234d7c55040091f039b84  examples/ratatui_shared_app.rs
ac7a8bbcece3474cbc8ef4e41e4385b176b9ab2079655fc8d455e6ab06158d73  examples/sftp_client.rs
56f9e9c6e80246688e7bd91c05d6f7fb3827b348b0374d3449ab872fa51991b9  examples/sftp_server.rs
69a9c118b0bfb50720015735d398a5a3e9bb4778988ca2702a7a3a5709a47fc1  examples/test.rs
66f44c09f50aec4a46d8f85e0dbf493be8ee9db2e4a442cacf7837c2a9a10b1c  src/auth.rs
690241c97a426034cf774091b8b99d9b25c09964700f9a166bb5af434df3760c  src/cert.rs
8d6678c43cae06d095ae4cc242ae33ca1bf65efcdef9a5948d382cbffb101f06  src/channels/channel_ref.rs
990be739b58d69e18682575a66eee123d7055a228f71429b9fb1276ed5419b9a  src/channels/channel_stream.rs
d058ed66458c0d5a7da50c270cb6c6a9c067c68cc328cc91a990d3ff39be8c1d  src/channels/io/mod.rs
6e241986dcb5f75fdf9788000fa7e9f6c0dc7500c7062fbb16cbcba0c6e0d166  src/channels/io/rx.rs
8c62048b5176ba6b330c5b74148a328172920ca950bfe6bddd1ba5b1a2d16767  src/channels/io/tx.rs
27a7e4eb3fec2f75287160ee0e03b412a419ee685d749466e7b8e759e7d7b7ba  src/channels/mod.rs
1632912cbd81f39684fdc57ef49f240677fd72c9176fa548eb9944b5d66ffeb9  src/cipher/benchmark.rs
1abade93fa680ee57634f2287f6a3ad378aaf5557e21d12ed198a4620cfc33a4  src/cipher/block.rs
5349c7a22b070501fb237da568c8e4ccbf31268b393289355de17200cbac2523  src/cipher/cbc.rs
461cf944d4d2fab4e0d494ad5bd0923e44795d34e03dc6556122f1a28ba4e47f  src/cipher/chacha20poly1305.rs
0d3c514e38f353f5134e939a0912e72a3e6e01cccf7f134a2304253b56a4e238  src/cipher/clear.rs
f734b060f43e14aabfd8fc48891f57fab804d0f378374579b4bc3a5098a7b2e4  src/cipher/gcm.rs
5793fb7f08d3494517c74264179adf78872ae8bdc5c7cedc8ed69f4a6cce52ec  src/cipher/mod.rs
02c3dc16b7bd6d74cd56c42263c1305c28bba14fa6a8018b281c9f54c7d742bf  src/client/encrypted.rs
4f540d935df565b1a1994784cee88186ae7b9b7fe4cd8ff68b771599d8d8b460  src/client/kex.rs
0099f76621a59460e5611480bdda45e5ac33570b7669c81ceb4ea7e0283fbcab  src/client/mod.rs
d4996f1e00f456f02bb791101300d5f93c84e2db54101a27d69f1a0aa29518f6  src/client/session.rs
54352ce932c83283c56c89d0e6f7420deb46bb17ce7f774ead80640a66968d1c  src/client/test.rs
774b59e33ff0746e2898d896074bcf800ab167f7174750c20ddfca69ada03c2c  src/compression.rs
e92055a34871f714c284f85c9d38491c87e74ee18a047db263488032f1d19821  src/helpers.rs
d194184afb2d82f178918bd4a43a8eee10748c1de05673bc1568e53139519ebb  src/kex/curve25519.rs
718f7e62a6da1289e652d4444e7d803ce5d74bfdffa3ad9ecc64c188f64517b1  src/kex/dh/groups.rs
c03651d5654fbb910efeab4528e6949c17a772d7b4145eb4c59133aa1827843e  src/kex/dh/mod.rs
0bb7c452200130edf71be0966983ebdc311ca57154343d93eeab509ec2beaf17  src/kex/ecdh_nistp.rs
c12f092465624e9d5bef94cbce3f2510ae1784c9a112aac3ea92e0527ba9a342  src/kex/hybrid_mlkem.rs
3e1ee0458db94499e3808bab4b701d50d6501f07906ad371a9e7438605991933  src/kex/mod.rs
8c0d847c2caadd7a2db77d34877d3182e38cb2e90ed44e3823b53361a770960d  src/kex/none.rs
be3061d50eb957e7c102cb9ce57638391dddcc5679f0de666d98bda58ee8ecc7  src/keys/agent/client.rs
5652be6d2079a3fdc8361f7e31cfe3a602bdc85a773b491d0bef91c909181e54  src/keys/agent/mod.rs
3461e3f12792ddbddc2aaddaca25f4e074e91a3e58488fe6b82dc087590134bc  src/keys/agent/msg.rs
94876dc20ce2261881da41a36a9342b6970586937d2275b32364d98367f0f91f  src/keys/agent/server.rs
7ca4a796a523df1063f0ea0bac1b6d869288dd100d041a6455f8bd7d73b15678  src/keys/format/mod.rs
e345287a5c5af34b95740c90431e322b86ba4230be4e116b2194c7de7bd99816  src/keys/format/openssh.rs
fe8c841b87992b49e4fe3337cfe4c427d31a4cacb0e5891d7e926aa3c76d6271  src/keys/format/pkcs5.rs
766c7c66c903319ec61a15e9362d82031441a97a09d9be26ba8b405e20e7f065  src/keys/format/pkcs8.rs
c751866710db20fa55fda72a8245a6d32c9b7acd49675309f08de9ae6f58b9c8  src/keys/format/pkcs8_legacy.rs
47a0f71ac07b8bf2604bcdf47e18b2a29a91035432f24c41ec57b639b391e055  src/keys/format/tests.rs
17b75a90739c32fc4b2fbeb31103b9cc8dd5f70c0b7de885bf1238b41b0b50e7  src/keys/key.rs
13802e03e9d8c11a25673ac29bddccaebedbbd759465b2d2ec847ba86e8e1049  src/keys/known_hosts.rs
a68f9e90eed19a372f82245d722b46c5808a8e8a5f1fa15510e92d8f7d5e034f  src/keys/mod.rs
8398c8b25509efa7d56b8577ec9c46e27f8ab9b20ac0f2068e7390ddfad883a9  src/lib.rs
4ed5ae82d19d2a4eeae46266552f9f45ce6c34a57e1fb2787f93259b2ca97f6a  src/lib_inner.rs
a6f8f5a98a6d342f925bb2842650b0c92e4c57c91036caf93aa2fdaa28a0b218  src/mac/crypto.rs
36f76c3e30556c182bac25cc4b2918d6ce7f2be666531df849db4baf8fb02148  src/mac/crypto_etm.rs
a3418bb9fdc1cf9102ffd968811946e9c92cb4a3d50a0759f5cdb091cce6d18e  src/mac/mod.rs
c58fe761c9e07df4956a8e349f2053e2d3fd28988b99a4af7b152f73a7db1211  src/mac/none.rs
c377a22642b6fd62d5be3676874e79173cd26d72e6a2a6bdc8ff86b1671eb20f  src/msg.rs
b88f4e8e266e7a72cebb8b804973cbef9c4cb9a25acc0c2b54063f6f5016c11b  src/negotiation.rs
e3f006102267653d83224074a57f72486d7a3cd0742f64268d12e432ec257ab8  src/parsing.rs
db018a9139923b907999fb9870aad0e9bc93fe03d11b65752f3f08fe63c4eb3b  src/pty.rs
89ab2c2c33dceb2b064ab6a38eaf7aa9229394c9da883134421aacee41e0dc76  src/server/encrypted.rs
e1975ea44e2233de66b33830eafc6eeea62a743961b5440feba07e5295afdc45  src/server/kex.rs
f069f72ce0aafb30efd6d08f2595f4d8ba1ecf70bf0427a1db499c950f161961  src/server/mod.rs
7ef668aa90c80f20711b4331168cb0512871f59e576d565ad2dbcbdb8b2d5f42  src/server/session.rs
28e4594f7c6e587c54075b7e15321e43961f9e0ec3dcc734b47c5ccbd856dccb  src/session.rs
469bc85924824923372391eba9815e72434388e36f6f8c34d33aed2e95127a2f  src/ssh_read.rs
ecd35c32beae59d63b9683ca30d1043bbf628a020c75cdbfe8d06fbf98a3680d  src/sshbuffer.rs
3a2b7f10ef4003ba8ec955cfca41056dc903a1ac19b9fdb23471a64a8bb01772  src/tests.rs
fbda2fc16da8ec666136f3a1d940a70114c1993454396dd6b3e6dda8a6e88d06  tests/auth_state_reset.rs
afdda5a7c13eb75cbe2c83acc9c714909d6bfad2568f2571a808dc07cb800e5b  tests/test_backpressure.rs
734e3b9669d7a070af48f6995addc39184ce8e0d266900df23781f2cbb9f5b16  tests/test_data_stream.rs
43b64360b3fa06c88c8490a73ab667949091889fa10cf52019fa5570e3b2a86b  tests/test_kex_shared_secret.rs
9f0137d81d46fa74ec42acb5481a6cc1f98f7dcd4e628dab220404f12b4fcb0a  tests/test_max_channel_packet_size.rs
481a55d30c61b8741436bf2f3aace02c7f6ee725ef9dcb2bbc963ce3a16bd0ca  tests/test_mlkem_kex.rs
a0097c9ca46d518d454328e26e4339fb813767befd17ca7bf6b2418f9748bf6b  tests/test_rekey_strict_kex.rs
```
<!-- 原样清单 止 -->

## 改过的文件（`路径  原样 sha256  补后 sha256`）

<!-- 改过的文件 起 -->
```text
src/compression.rs  774b59e33ff0746e2898d896074bcf800ab167f7174750c20ddfca69ada03c2c  a6203a3b2ac629fc2f7418e844edb1fdcfbc0df17465cddea284efc027175284
```
<!-- 改过的文件 止 -->

## 副本特有的文件（`sha256  路径`；`VENDOR.md` 自己不记指纹）

<!-- 副本特有 起 -->
```text
62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a  LICENSE-APACHE
```
<!-- 副本特有 止 -->
