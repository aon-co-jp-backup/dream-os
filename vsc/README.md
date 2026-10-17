# DreamOS Code (`vsc/`)

**日本語**: dream-os 用の Visual Studio Code として、VS Code 本体のオープンソース版 **Code-OSS**(MIT、`microsoft/vscode`)を Linux 向けにビルド・パッケージし、[`open-vsc-claude`](https://github.com/aon-co-jp/open-vsc-claude) を**組み込み拡張**として同梱する手順とスクリプトです。Windows の VS Code 用 `open-vsc-claude` と**同じ拡張**(同じ `.vsix`)を使うため、互換性は[互換性の契約](https://github.com/aon-co-jp/open-vsc-claude/blob/main/docs/COMPATIBILITY.md)で保たれます。

**English**: Build and package **Code-OSS** (the MIT-licensed open-source core of VS Code) for Linux, with the [`open-vsc-claude`](https://github.com/aon-co-jp/open-vsc-claude) extension bundled as a built-in extension. It is the very same `.vsix` as the Windows VS Code one, so compatibility is governed by the [host contract](https://github.com/aon-co-jp/open-vsc-claude/blob/main/docs/COMPATIBILITY.md).

> dream-os 自体はまだ起動できる OS ではありません(構想段階+PoC)。ここでの検証は **WSL2 の Ubuntu(Linux)** で行います。dream-os のカーネル上で動かすのは、dream-os が Linux ユーザーランドを提供できるようになってからの課題です。
> *dream-os itself is not a bootable OS yet (concept + PoCs). Verification here is done on **Ubuntu in WSL2**; running it on the dream-os kernel depends on dream-os providing a Linux userland.*

## 使い方 / Usage

```bash
# Linux / WSL2 (native disk, not /mnt/*) — needs ~15 GB free and 8+ GB RAM
sudo apt-get install -y libx11-dev libxkbfile-dev libsecret-1-dev libkrb5-dev pkg-config make g++ python3 git curl xz-utils xvfb
./build.sh     # Node (user-level) -> clone microsoft/vscode @ VSCODE_TAG -> npm ci -> gulp package -> bundle extension + overlay
./verify.sh    # version, built-in extension, extension host runs open-vsc-claude (headless via Xvfb)
~/dream-vsc/VSCode-linux-x64/bin/code-oss
```

- `VSCODE_TAG`(既定 `1.142.0`)、`NODE_VERSION`(そのタグの `.nvmrc`)、`WORK`(既定 `~/dream-vsc`)、`EXT_VSIX`(同梱する `.vsix`。未指定ならローカルの最新、無ければ GitHub のリリース)で変更できます。
- `product-overlay.json`: 名称(DreamOS Code)、データフォルダ(`.dream-os-code`)、拡張ギャラリー(**Open VSX**。Code-OSS 系は Microsoft Marketplace を使えません)。
- 各段階は完了印(`$WORK/.done-*`)で再開できます。

## 注意 / Notes

- Code-OSS のビルド成果物は Microsoft の「Visual Studio Code」製品とは別物です(商標・ライセンス・テレメトリが異なる)。再配布する場合は MIT ライセンス表記を同梱してください。*The result is not Microsoft's branded VS Code product; keep the MIT notices if you redistribute.*
- Microsoft Marketplace では配布されない拡張は、Open VSX 側にも公開する必要があります(`open-vsc-claude` の `PUBLISHING.md` 参照)。
- 実際に走らせた結果と所要時間は [`BUILD-REPORT.md`](BUILD-REPORT.md) に記録します。
