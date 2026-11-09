#!/usr/bin/env bash
# 下 sherpa-onnx 的预编译静态库、对照 tools/sherpa-onnx.lock 里钉死的 SHA-256 校验，解进 target/sherpa-onnx/lib。
# .cargo/config.toml 把 SHERPA_ONNX_LIB_DIR 一直指到那里：没跑这个就编不过（sherpa-onnx-sys 报那个目录不在），
# 不会退回成它自己下、不验校验和的那条路。CI、发包、本机开发都跑它；Windows 上不用 bash 的跑 tools/sherpa-lib.ps1。
#
#   用法：在仓库根目录 tools/sherpa-lib.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
lock="$root/tools/sherpa-onnx.lock"
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux-x86_64 ;;
  Darwin-arm64) platform=macos-aarch64 ;;
  MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) platform=windows-x86_64 ;;
  *) echo "sherpa-lib.sh: no pinned sherpa-onnx library for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
version=$(awk '$1 == "version" {print $2}' "$lock")
read -r name sha < <(awk -v p="$platform" '$1 == p {print $2, $3}' "$lock")
stem="sherpa-onnx-v$version-$name"

dir="$root/target/sherpa-onnx"
if [ "$(cat "$dir/lib/.stem" 2>/dev/null)" != "$stem" ]; then
  rm -rf "$dir" && mkdir -p "$dir"
  archive="$dir/$stem.tar.bz2"
  curl -fsSL --retry 3 -o "$archive" \
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$version/$stem.tar.bz2"
  if command -v sha256sum >/dev/null; then
    got=$(sha256sum "$archive" | cut -d' ' -f1)
  else
    got=$(shasum -a 256 "$archive" | cut -d' ' -f1)
  fi
  if [ "$got" != "$sha" ]; then
    echo "sherpa-lib.sh: $stem.tar.bz2 has SHA-256 $got, expected $sha" >&2
    rm -f "$archive"
    exit 1
  fi
  tar -xjf "$archive" -C "$dir"
  rm -f "$archive"
  mv "$dir/$stem/lib" "$dir/lib"
  rm -rf "$dir/$stem"
  echo "$stem" > "$dir/lib/.stem"
fi
echo "$dir/lib"
