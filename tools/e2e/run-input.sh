#!/usr/bin/env bash
# 语音输入对真核心的端到端（施工 VO-9）：隔离的数据根，模型预先放进包的缓存目录，不开麦。
set -u
S=${MIYU_VOICE_E2E:?set MIYU_VOICE_E2E to a scratch folder, see tools/e2e/README.md}
CORE=${MIYU_CORE:?set MIYU_CORE to a built core checkout}
M=$CORE/target/release/miyu
V=$(cd "$(dirname "$0")/../.." && pwd)
W=$S/model-dl/x/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09/test_wavs
export MIYU_HOME=$S/e2e-home XDG_RUNTIME_DIR=$S/e2e-run MIYU_RESOURCES=$CORE/resources
rm -rf $MIYU_HOME $S/e2e-pkg; mkdir -p $MIYU_HOME $S/e2e-pkg/voice-input/bin $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
cp $V/package/voice-input/package.toml $S/e2e-pkg/voice-input/; cp $V/target/release/miyu-voice-input $S/e2e-pkg/voice-input/bin/
$M core > $S/core.log 2>&1 & CP=$!
trap 'kill $CP 2>/dev/null; wait 2>/dev/null' EXIT
for i in $(seq 50); do [ -f $MIYU_HOME/run/socket ] && break; sleep 0.2; done
mkdir -p $MIYU_HOME/var/cache/voice-input && cp -r $S/vi-cache/sense-voice-int8-2024-07-17 $MIYU_HOME/var/cache/voice-input/
pid_of() { for p in /proc/[0-9]*; do [ "$(readlink $p/exe 2>/dev/null)" = "$MIYU_HOME/opt/voice-input/bin/miyu-voice-input" ] && grep -qa -- "$1" $p/cmdline && echo ${p#/proc/}; done; }
$M pkg install --yes $S/e2e-pkg/voice-input 2>&1 | tail -1
for i in $(seq 30); do $M bus list stt 2>/dev/null | grep -q " 可用" && break; sleep 0.5; done
echo "1. bus: $($M bus list stt 2>&1 | tail -1)"
echo "   resident after start: $(grep VmRSS /proc/$(pid_of serve | head -1)/status 2>/dev/null)"
python3 -I $V/tools/e2e/head.py $MIYU_HOME \
  "blob.put {\"path\": \"$W/zh.wav\"}" \
  'bus.call {"name": "stt", "method": "transcribe", "params": {"audio": "$blob"}}' \
  "blob.put {\"path\": \"$W/en.wav\"}" \
  'bus.call {"name": "stt", "method": "transcribe", "params": {"audio": "$blob", "language": "auto"}}' \
  'bus.call {"name": "stt", "method": "transcribe", "params": {"audio": "$blob", "lang": "en"}}' \
  'bus.call {"name": "stt", "method": "transcribe", "params": {"audio": "$blob", "language": "xx"}}' \
  'bus.call {"name": "stt", "method": "transcribe", "params": {"audio": "sha256:0000000000000000000000000000000000000000000000000000000000000000"}}' \
  'bus.call {"name": "stt", "method": "stop"}' 2>&1 | sed 's/^/2. /'
python3 -I -c "import sys, wave; w = wave.open(sys.argv[1], 'wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(bytes(96000)); w.close()" $S/zeros.wav
echo "2c. three seconds of digital silence (VO-26):"; python3 -I $V/tools/e2e/head.py $MIYU_HOME "blob.put {\"path\": \"$S/zeros.wav\"}" 'bus.call {"name": "stt", "method": "transcribe", "params": {"audio": "$blob"}}' 2>&1 | tail -1 | cut -c1-220 | sed 's/^/    /'
echo "2a. settings page microphone choices (VO-21):"; python3 -I $V/tools/e2e/head.py $MIYU_HOME 'package.call {"package": "voice-input", "method": "microphones"}' 2>&1 | cut -c1-300 | sed "s/^/    /"
echo "    schema says: $(HEAD_FULL=1 python3 -I $V/tools/e2e/head.py $MIYU_HOME 'config.schema {}' 2>&1 | grep -o '"choices_from": {[^}]*}' | head -1)"
echo "2b. same connection, long transcribe then stop:"; python3 -I $V/tools/e2e/pipelined.py $MIYU_HOME $S/long.wav | sed "s/^/    /"
P=$(pid_of serve | head -1)
echo "3. main resident after recognizing: $(grep VmRSS /proc/$P/status)"
echo "   worker alive: $(pid_of recognize-worker | wc -l)"
python3 -I $V/tools/e2e/head.py $MIYU_HOME 'package.disable {"package": "voice-input"}' | sed "s/^/   /"
sleep 2; echo "4. after disabling: $($M bus list stt 2>&1 | tail -1)"
echo "   processes left: $(pid_of miyu-voice-input | wc -l)"
