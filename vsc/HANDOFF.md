<!-- Shared hand-off block. dream-os / open-vsc-claude docs link or copy this text. -->
## 🔁 お引越し・再開用メッセージ / Resume message (2026-10-18, Windows 再起動前 / before the Windows restart)

**日本語(次のセッションの Claude へ)**
1. まず `F:\dream-os\vsc\BUILD-REPORT.md` を読む。DreamOS Code(Code-OSS 1.142.0)のビルドは **node / clone / `npm ci` までが完了、パッケージ化(`gulp`)以降が未完了・未検証**。
2. 再開は PowerShell で `wsl -d Ubuntu -- bash /mnt/f/dream-os/vsc/build.sh`(完了印があれば続きから)。終わったら `verify.sh`(Xvfb が要る場合は `wsl -u root apt-get install -y xvfb`)。結果は `BUILD-REPORT.md` に正直に書き、`vsc/` をコミット。`F:\dream-os` の `installer/windows/make-usb.ps1` の未コミット変更は**この作業のものではない**ので巻き込まない。
3. `open-vsc-claude` は v0.1.2 を公開済み(`F:\open-vsc-claude`、デモ `https://easy-web.tokyo/open-vps-claude/`、テスト 4 種が通る)。**未了**: Marketplace/Open VSX 公開(所有者のアカウントとトークンが必要。手順は `PUBLISHING.md`)、Cosmo Router の実起動、実キーでの Claude 生成、実機 iOS/Android、翻訳のネイティブ確認。
4. 秘密(トークン・API キー)はリポジトリに入れない。`F:\` 直下の txt に置き、値を画面・コミットに出さない。
5. 落とし穴: Git Bash は `/mnt/...` を書き換える(PowerShell を使う)、シェルの heredoc で python に `\n` を渡すと本物の改行になる(補助スクリプトは先にファイルへ)、CRLF のファイルは置換が静かに失敗する(Edit ツールを使う)、書き換え済みリポジトリで `git pull` しない。

**English (for the next Claude session)**
1. Read `F:\dream-os\vsc\BUILD-REPORT.md` first. The DreamOS Code (Code-OSS 1.142.0) build is **done through node / clone / `npm ci`; packaging (`gulp`) and later stages are not done and unverified**.
2. Resume from PowerShell: `wsl -d Ubuntu -- bash /mnt/f/dream-os/vsc/build.sh` (stamped stages are skipped), then `verify.sh` (if Xvfb is missing: `wsl -u root apt-get install -y xvfb`). Record the real result honestly in `BUILD-REPORT.md` and commit `vsc/`. The uncommitted change to `installer/windows/make-usb.ps1` in `F:\dream-os` is **not part of this work** — leave it alone.
3. `open-vsc-claude` v0.1.2 is released (`F:\open-vsc-claude`, demo `https://easy-web.tokyo/open-vps-claude/`, all four test suites pass). **Open**: Marketplace / Open VSX publication (needs the owner's account and token; see `PUBLISHING.md`), running the Cosmo Router, Claude generation with a real key, real iOS/Android devices, native review of translations.
4. Never put secrets (tokens, API keys) in repositories; keep them in txt files at the root of `F:\` and never print or commit their values.
5. Pitfalls: Git Bash rewrites `/mnt/...` (use PowerShell); escapes in shell heredocs turn `\n` into real newlines (write helper scripts to files first); CRLF files make scripted replacement fail silently (use the Edit tool); never `git pull` in rewritten repositories.
