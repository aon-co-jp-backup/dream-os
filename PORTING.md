# PORTING.md — dream-os お引越しファイル / Porting notes

> 対象バージョン: 構想段階(2026-08-06、コード未着手)。/ Target version:
> concept stage (2026-08-06, no code yet).

## 0. このリポジトリのスコープ / Scope of this repository

日本語: `dream-os`はWindows・macOS・Android・各種Linux向け(将来的には
SONY PlayStation 5/6・Nintendo Switch 2)のアプリケーションサービス層/
GPU抽象化層の構想リポジトリです。現時点でコードは一切ありません。詳細は
[`CLAUDE.md`](CLAUDE.md)・[`README.md`](README.md)参照。

English: `dream-os` is a concept-stage repository for an application
service layer / GPU abstraction layer targeting Windows, macOS, Android,
and various Linux distributions (with Sony PlayStation 5/6 and Nintendo
Switch 2 as future goals). No code exists yet. See [`CLAUDE.md`](CLAUDE.md)
and [`README.md`](README.md) for details.

## 1. 持っていくもの(ファイル一覧) / Files to bring along

```
dream-os/
├── README.md          # 日本語(主言語) / Japanese (primary language)
├── README-English.md  # 英語 / English
├── CLAUDE.md           # 開発方針・エコシステム内の位置づけ・HANDOFF
├── PORTING.md          # 本ファイル / this file
├── Cargo.toml           # ワークスペース定義(2026-08-06新設)
└── crates/
    └── dream-os-kernel/  # PoC: open-cudaのopencuda-vulkanを再利用した
                           # Windows/Android共通実行基盤+電力出力調整可能な
                           # マイニングOS向けデューティサイクル制御
        ├── Cargo.toml     # ../../../open-cuda/crates/opencuda-* へのpath依存
        ├── src/{lib,power_profile,mining,sbm,directx_bridge,
        │        flash_attention_bridge}.rs
        │        # flash_attention_bridge.rs(2026-08-08新設): open-cudaの
        │        # 融合flash attention SPIR-Vカーネルをdream-osの共通Vulkan
        │        # 実行基盤経由で呼び出す、Windows GT730・Android Adreno 619
        │        # 両実機で実証済み(8×16 toyサイズPoC、GPT-2スケールの
        │        # ベンチマークは未実施)
        ├── shaders/{vector_add,sha256d_mine,sbm_ising}.{comp,spv}
        ├── examples/{dispatch_throttled,mine_benchmark,sbm_benchmark,
        │            directx_bridge_benchmark,dream_os_status}.rs
        │            # sbm_benchmark/directx_bridge_benchmark(2026-08-06新設):
        │            # cargo testが使えないAndroidクロスビルド環境向けに、
        │            # adb pushして直接実行できる単体バイナリとして実装
        │            # (Windows GT730・Android Adreno 619両方で実機検証済み)
        └── tests/{mining_real_vulkan,sbm_real_vulkan,directx_bridge_real_vulkan,
        │         flash_attention_bridge_real_vulkan}.rs
        # directx_bridge.rsは../../../open-directx/crates/
        # directx-shader-translate へのpath依存を追加で持つ
    └── dream-os-raid-bridge/  # open-raid-zのRAID6/Z2実装ブリッジ(2026-08-08新設)
        │                # open_raid_z_core::vdev::RaidZVdev(GF(2^8)
        │                # Reed-Solomon)+zfs_accel_hlsl(D3D12/DirectML・
        │                # Vulkan GPU/NPUパリティ高速化)への薄いpath依存
        ├── Cargo.toml   # ../../../open-raid-z/open_runo_zfs_source/
        │                # open_raid_z_core へのpath依存
        ├── src/lib.rs
        ├── examples/raid6_bridge_benchmark.rs
        │            # 1コマンドで動くrunnableデモ(2026-08-08新設)。
        │            # 実GT730でGPUアクセラレータ(CPUフォールバックでない)・
        │            # 8ストライプ往復・1台破損からの自己修復を確認済み。
        │            # 実機複数NVMe(4〜16枚)検証・Androidクロスビルドは
        │            # 未実施(正直な開示、次回課題)。
        └── tests/raid6_bridge_real.rs
    └── dream-os-wire/  # World Laboratory向け通信・永続化層
        │                # (open-web-server-wireのSecureChannel再利用=
        │                # AEAD暗号化+リプレイ対策、aruaru-db永続化)
        ├── Cargo.toml   # ../../../open-web-server/crates/open-web-server-*
        │                # (さらにopen-web-server-wireは
        │                # ../../../RS-SmartTCPへのpath依存を持つ)
        ├── src/{lib,aruaru_persistence}.rs
        └── tests/{secure_channel_integration,aruaru_db_integration}.rs
        # aruaru_db_integration.rsはDREAM_OS_ARUARU_PG_CONN環境変数で
        # 指定したaruaru-server(pgwire)インスタンスへの実接続が必要
        # (未設定/到達不能時は正直にスキップする設計)
```

`crates/dream-os-kernel`は`open-cuda`(`../../open-cuda`、同じ`F:\runo`
直下にcloneされている前提)・`open-directx`(`../../open-directx`、
同様)の2つへのpath依存を持つ。`crates/dream-os-wire`は`open-web-server`
(`../../../open-web-server`)へのpath依存を持ち、さらに
`open-web-server-wire`自体が`RS-SmartTCP`(`../../../RS-SmartTCP`、
`open-web-server`から見た相対位置)へのpath依存を持つ。`dream-os-wire`の
`aruaru_persistence`モジュールは実行時に`aruaru-db`(`aruaru-server`
バイナリ、pgwire)への接続を必要とする(path依存ではなくネットワーク
接続、`DREAM_OS_ARUARU_PG_CONN`環境変数で接続先を指定)。移設する場合は
`open-cuda`・`open-directx`・`open-web-server`・`RS-SmartTCP`をすべて
同じ相対位置関係でcloneしておくこと。 / `crates/dream-os-kernel` has path
dependencies on `open-cuda` (`../../open-cuda`, assumed cloned alongside
this repo under `F:\runo`) and `open-directx` (`../../open-directx`,
likewise). `crates/dream-os-wire` has a path dependency on
`open-web-server` (`../../../open-web-server`), which in turn has a path
dependency on `RS-SmartTCP` (`../../../RS-SmartTCP`, relative to
`open-web-server`). `dream-os-wire`'s `aruaru_persistence` module needs a
runtime network connection to `aruaru-db` (the `aruaru-server` binary,
via pgwire) rather than a path dependency — configured via the
`DREAM_OS_ARUARU_PG_CONN` env var. When relocating, keep `open-cuda`,
`open-directx`, `open-web-server`, and `RS-SmartTCP` all cloned at the
same relative positions.

## 2. VPS/ローカルとの対応 / Local & VPS locations

- ローカル / Local: `F:\runo\dream-os`
- VPS(conoha): `/root/dream-os`(2026-08-06新設、プレースホルダの
  `README.md`のみ、git管理下ではない — created 2026-08-06, placeholder
  `README.md` only, not yet under git)

## 3. 移設・再開時の注意 / Notes for relocation / resuming

コードがまだ無いため、移設時は上記4ファイルをコピーするだけで完結します。
実装が始まった段階で、このセクションを`open-directx`/`RPoem`等と同様の
「ビルド手順」「持っていくもの」節へ拡充してください。

Since there is no code yet, relocating this repository is as simple as
copying the four files above. Once implementation begins, expand this
section to match the "build steps" / "files to bring" sections used by
`open-directx` / `RPoem`, etc.

---

Other languages: [English](PORTING-English.md) · [Deutsch](PORTING-German.md) ·
[Italiano](PORTING-Italian.md) · [Français](PORTING-French.md) ·
[Русский](PORTING-Russian.md) · [Українська](PORTING-Ukrainian.md) ·
[עברית](PORTING-Hebrew.md) · [فارسی](PORTING-Persian.md) ·
[العربية](PORTING-Arabic.md)

## Windows への導入と、Windows / Ubuntu での互換試験 (2026-10-14) / Windows install and compatibility tests

**導入**: `installer\windows\install.ps1`(ユーザー単位・管理者権限不要。`-Uninstall` で削除)。`%LOCALAPPDATA%\dream-os\bin` に、状態確認・RAID6・DXBC→SPIR-V のデモと `dream-os` コマンドを置きます。
`dream-os status|raid|dx` は Windows 上のネイティブ実行、`dream-os linux` は WSL2 の Ubuntu を開き、`dream-os both` は両方を**同時**に動かします(Windows はそのまま使えます)。
**正直な開示**: dream-os は、まだ単独で起動できる OS ではありません(Linux x TRON のハイブリッドカーネルは構想段階)。「Windows と切り替えて使う」「dream-os だけで起動する」ことはできず、ここでの「同時使用」は、Windows と WSL2(Linux)の並行実行です。
ビルドには、シェーダーコンパイラ `glslc`(Vulkan SDK / Ubuntu は `apt install glslc`)が要ります。`open-cuda` の `flash_attention.spv` は git 管理外なので、`glslc --target-env=vulkan1.1 flash_attention.comp -o flash_attention.spv` で作ります。

**試験結果**(Windows 11 + NVIDIA GT 730 / WSL2 Ubuntu + llvmpipe):
| 対象 | Windows | Ubuntu(WSL2) |
|---|---|---|
| dream-os(kernel・wire) | 全件成功 | 18 件成功 |
| dream-os-raid-bridge | 成功(GPU でパリティ計算、自己修復) | 成功(2026-10-16。CPU フォールバックで書き込み・読み出し・自己修復。全体で 25 件成功) |
| `dream_os_status`(GPU・DXBC→SPIR-V・Flash Attention) | OK(GT 730) | OK(llvmpipe) |
| open-cuda | 176 件成功 | 全件成功 |
| open-directx | 136 件成功 | 136 件成功 |
| open-cpu | 40 件成功 | 40 件成功 |
| aruaru-llm | 168 件成功 | 168 件成功 |
| aruaru-IME | 177 件成功 | CI で ibus / fcitx5 / インストールの通し試験に成功 |

見つけて直した不具合: `opencuda-blas` の `sgemm_auto_dispatch...nvidia` 試験が、NVIDIA 以外の Vulkan デバイス(WSL の llvmpipe)で失敗していた(NVIDIA でなければスキップするよう修正、open-cuda 側)。

English summary: install with `installer\windows\install.ps1` (per-user). `dream-os status|raid|dx` run natively on Windows; `dream-os linux` opens WSL2 Ubuntu; `dream-os both` runs both at once. dream-os is not yet a bootable standalone OS, so "switching" and "standalone boot" are not possible; concurrency means Windows and WSL2 side by side. Tests pass on Windows (GT 730) and Ubuntu/WSL2 (llvmpipe) except `dream-os-raid-bridge` on Ubuntu (no packaged `dxc`), untested. Fixed a GPU-vendor assumption in an open-cuda test (skips on non-NVIDIA devices).

## 日本語フォントの同梱 (2026-10-14) / Bundled Japanese font
`crates/dream-os-fonts` に、オープンソースの日本語フォント **IPAゴシック**(IPA Font License v1.0、OSI 承認、再配布可)を同梱しました(`fonts/ipag.ttf`、ライセンス全文 `fonts/IPA-LICENSE.txt`)。Ubuntu の公式パッケージ `fonts-ipafont-gothic` から取得。
Windows のインストーラーは `%LOCALAPPDATA%\dream-os\fonts\` に書き出します。`dream-os fonts` で、字形の網羅(ひらがな・カタカナ・漢字・記号・全角半角の見本 296 文字)を検査できます。
**試験**: 単体試験 4 件(見本の字形が全部ある・TTF として正しい・無い文字を報告する・ライセンス同梱)は Windows と Ubuntu で成功。`dream-os fonts` も両方で見本 296 文字すべてに字形あり。**描画も確認済み**(`render_text` と `examples/render_png`、`crates/dream-os-fonts/sample.png`): 「今日は、様です。ひらがな カタカナ 漢字変換 般若心経 ABC 123」を実際に絵にして目視で確認し、Windows と Ubuntu で出力 PNG がバイト単位で同一でした(単体試験は 5 件)。
なお、前に直した文字化けの原因は、フォントの不足ではなく、`.cmd` と PowerShell スクリプトの文字コード(UTF-8 の日本語を、Windows が別の文字コードで読んでいた)でした。フォント同梱は、dream-os 自身が日本語を表示するための別の備えです。
English: bundled the open-source IPAGothic font (IPA Font License v1.0) in `dream-os-fonts`, written to `%LOCALAPPDATA%\dream-os\fonts` by the installer; `dream-os fonts` checks glyph coverage (296 sample chars, none missing). 4 unit tests pass on Windows and Ubuntu; rasterization verified (identical PNG on Windows and Ubuntu). The earlier mojibake was a script-encoding problem, not a missing font.

### Ubuntu での dream-os-raid-bridge (2026-10-16) / raid-bridge on Ubuntu
`dxc`(Microsoft の DirectXShaderCompiler v1.9.2609 の公式配布 `linux_dxc_*.tar.gz`、約 13MB。利用者の許可を得て取得、`~/dxc` に展開、PATH と `LD_LIBRARY_PATH` に追加)でビルドは通るが、`zfs_accel_hlsl` の既定の `gpu` 機能は Windows 専用(D3D12/DirectML)で Linux ではコンパイルできなかった。`dream-os-raid-bridge/Cargo.toml` を、Windows は従来どおり `gpu`、Linux・macOS は `gpu` を外して `vulkan` 機能(Vulkan Compute)を使う指定に分けた。結果: Ubuntu で dream-os 全体の試験 25 件成功、デモは CPU フォールバックで書き込み・読み出し・1 台壊しての自己修復が一致(WSL2 の llvmpipe では GPU アクセラレーションは選ばれない)。
English: with the official `dxc` for Linux (downloaded with permission), the build passes, but `zfs_accel_hlsl`'s default `gpu` feature is Windows-only. `dream-os-raid-bridge` now uses `gpu` on Windows and `vulkan` (no `gpu`) elsewhere. On Ubuntu all 25 dream-os tests pass and the demo round-trips and self-heals on the CPU fallback.
