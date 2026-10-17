# BUILD-REPORT (DreamOS Code / Code-OSS)

## 2026-10-18 時点 / Status as of 2026-10-18

**日本語**
- 環境: WSL2 Ubuntu 26.04(32コア、RAM 15GB、ext4に936GB空き)。Windows 側から `wsl -u root apt-get install -y libx11-dev libxkbfile-dev libsecret-1-dev libkrb5-dev pkg-config make g++ python3 git curl xz-utils` で依存を導入済み(パスワード不要)。
- `build.sh` の進捗(`~/dream-vsc/.done-*` の完了印): **node ✅ / clone ✅(`microsoft/vscode` 1.142.0)/ install ✅(`npm ci`)/ package ❌(未完了)/ bundle ❌**。
  Windows の再起動で、`gulp` によるパッケージ化の途中で中断した。**パッケージ化以降は未検証**(`verify.sh` も未実行)。「ビルドできた」とはまだ言えない。
- 再開: そのまま `build.sh` を再実行すれば、完了印のある段階を飛ばして `package` から続きます。

**English**
- Environment: WSL2 Ubuntu 26.04 (32 cores, 15 GB RAM, 936 GB free on ext4). Dependencies were installed from Windows with `wsl -u root apt-get install -y libx11-dev libxkbfile-dev libsecret-1-dev libkrb5-dev pkg-config make g++ python3 git curl xz-utils` (no password needed).
- `build.sh` progress (stamps in `~/dream-vsc/.done-*`): **node ✅ / clone ✅ (`microsoft/vscode` 1.142.0) / install ✅ (`npm ci`) / package ❌ / bundle ❌**.
  The Windows restart interrupted the `gulp` packaging. **Everything from packaging onward is unverified** (`verify.sh` has not been run) — do not claim "it builds" yet.
- Resume: just run `build.sh` again; it skips stamped stages and continues at `package`.

## 再開コマンド / Resume commands (PowerShell)

```powershell
wsl -d Ubuntu -- bash /mnt/f/dream-os/vsc/build.sh
wsl -d Ubuntu -- bash /mnt/f/dream-os/vsc/verify.sh      # needs xvfb: wsl -u root apt-get install -y xvfb
```

## 落とし穴 / Pitfalls

- **Git Bash はパスを書き換える**: `wsl ... /mnt/f/...` が `C:/Program Files/Git/mnt/f/...` になる。PowerShell から実行するか `MSYS_NO_PATHCONV=1` を付ける。 / *Git Bash rewrites `/mnt/...` paths; use PowerShell or `MSYS_NO_PATHCONV=1`.*
- PowerShell の引用符内の `|` や `$変数` は WSL に渡る前に解釈される。複雑な処理は `.sh` ファイルに書いて実行する。 / *Pipes and `$vars` inside quoted PowerShell strings are interpreted before reaching WSL; put logic in a `.sh` file.*
- パッケージ化のタスク名は VS Code の版で変わりうる。`build.sh` は `gulp --tasks-simple` から探すが、**この 1.142.0 で実際に通るかは未確認**。失敗したらログ末尾のタスク名を確認する。 / *The gulp task name may differ per VS Code version; `build.sh` discovers it, but it is unconfirmed on 1.142.0.*
- メモリ: `NODE_OPTIONS=--max-old-space-size=8192`。RAM 15GB で足りるかは未確認。 / *Memory sufficiency on 15 GB is unconfirmed.*
