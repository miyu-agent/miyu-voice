# 同 tools/sherpa-lib.sh，给没有 bash 的 Windows：下 sherpa-onnx 的预编译静态库，对照 tools/sherpa-onnx.lock 校验，
# 解进 target\sherpa-onnx\lib（.cargo/config.toml 的 SHERPA_ONNX_LIB_DIR 指着那里）。
#   用法：在仓库根目录 powershell -File tools\sherpa-lib.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$lock = Get-Content (Join-Path $root 'tools/sherpa-onnx.lock') | Where-Object { $_ -notmatch '^#' }
$version = (($lock | Where-Object { $_ -match '^version ' }) -split ' ')[1]
$fields = ($lock | Where-Object { $_ -match '^windows-x86_64 ' }) -split ' '
$name = $fields[1]; $sha = $fields[2]
$stem = "sherpa-onnx-v$version-$name"
$dir = Join-Path $root 'target/sherpa-onnx'
$stamp = Join-Path $dir 'lib/.stem'
if (-not (Test-Path $stamp) -or (Get-Content $stamp) -ne $stem) {
  if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
  New-Item -ItemType Directory -Force $dir | Out-Null
  $archive = Join-Path $dir "$stem.tar.bz2"
  Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$version/$stem.tar.bz2" -OutFile $archive
  $got = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLower()
  if ($got -ne $sha) { Remove-Item $archive; throw "$stem.tar.bz2 has SHA-256 $got, expected $sha" }
  tar -xjf $archive -C $dir
  Remove-Item $archive
  Move-Item (Join-Path $dir "$stem/lib") (Join-Path $dir 'lib')
  Remove-Item -Recurse -Force (Join-Path $dir $stem)
  Set-Content -NoNewline $stamp $stem
}
Join-Path $dir 'lib'
