#!/usr/bin/env bash
# Verify the built DreamOS Code: version, built-in extension present, extension host runs open-vsc-claude (headless via Xvfb).
set -euo pipefail
WORK="${WORK:-$HOME/dream-vsc}"
OUT="$WORK/VSCode-linux-x64"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$WORK/node/bin:$PATH"
BIN="$OUT/bin/code-oss"
[ -x "$BIN" ] || BIN="$(ls "$OUT"/bin/* | head -1)"
tmp="$(mktemp -d)"
common=(--no-sandbox --disable-gpu --disable-dev-shm-usage --user-data-dir="$tmp/data" --extensions-dir="$tmp/ext")

echo "== version";            "$BIN" "${common[@]}" --version
echo "== built-in extension"; grep -o '"name": *"open-vsc-claude"' "$OUT/resources/app/extensions/open-vsc-claude/package.json"
echo "== product";            node -e 'const p=require(process.argv[1]);console.log(p.nameLong,"|",p.dataFolderName,"|",p.extensionsGallery&&p.extensionsGallery.serviceUrl)' "$OUT/resources/app/product.json" 2>/dev/null \
                               || python3 -c 'import json,sys;p=json.load(open(sys.argv[1]));print(p["nameLong"],"|",p["dataFolderName"],"|",p["extensionsGallery"]["serviceUrl"])' "$OUT/resources/app/product.json"
echo "== extension host (xvfb)"
log="$tmp/run.log"
set +e
xvfb-run -a timeout 180 "$BIN" "${common[@]}" --extensionTestsPath="$HERE/verify-suite.js" >"$log" 2>&1
code=$?
set -e
grep -E "DREAMOS-VERIFY-OK" "$log" || { echo "FAILED (exit $code); log tail:" >&2; tail -30 "$log" >&2; exit 1; }
echo "all checks passed"
