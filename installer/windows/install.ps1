# dream-os を Windows に入れる(ユーザー単位・管理者権限は不要)。
# Windows と dream-os は同時に動かせます: dream-os は、Windows の上のネイティブ実行(dream-os status など)と、
# WSL2 の Linux 側(Ubuntu)の両方で動き、どちらも同時に使えます。切り替えは `dream-os` コマンドから。
# 注意(正直な開示): dream-os は、まだ単独で起動できる OS ではありません(Linux x TRON のハイブリッドカーネルは構想段階)。
#   ここで入れるのは、実機で動作確認済みの実行基盤(GPU / RAID6 / 通信)と、状態確認・切り替えの道具です。
# 使い方: powershell -ExecutionPolicy Bypass -File installer\windows\install.ps1 [-Uninstall]
param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$dest = Join-Path $env:LOCALAPPDATA 'dream-os'
if ($Uninstall) {
    $p = [Environment]::GetEnvironmentVariable('Path', 'User')
    $np = ($p -split ';' | Where-Object { $_ -and ($_ -ne "$dest\bin") }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $np, 'User')
    if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
    Write-Host "dream-os を削除しました / removed."
    return
}
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
Push-Location $root
cargo build --release --workspace --examples
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "build failed (open-cuda の shaders\flash_attention.spv が無いときは glslc で作ってください)" }
Pop-Location
$bin = Join-Path $dest 'bin'
New-Item -ItemType Directory -Force $bin | Out-Null
foreach ($e in 'dream_os_status', 'directx_bridge_benchmark', 'dispatch_throttled') {
    Copy-Item (Join-Path $root "target\release\examples\$e.exe") $bin -Force
}
Copy-Item (Join-Path $root 'target\release\examples\raid6_bridge_benchmark.exe') $bin -Force
Copy-Item (Join-Path $root 'target\release\examples\font_check.exe') $bin -Force
$fonts = Join-Path $dest 'fonts'
& (Join-Path $bin 'font_check.exe') $fonts | Out-Null
Copy-Item (Join-Path $root 'crates\dream-os-fonts\fonts\IPA-LICENSE.txt') $fonts -Force
Copy-Item (Join-Path $PSScriptRoot 'dream-os.cmd') $bin -Force
$p = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($p -split ';') -notcontains $bin) { [Environment]::SetEnvironmentVariable('Path', "$p;$bin", 'User') }
Write-Host "dream-os を入れました: $dest  (新しいターミナルで `dream-os status`)"
