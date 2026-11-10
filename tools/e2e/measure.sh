#!/usr/bin/env bash
# 量尺（施工 VO-13下）：三个包在真核心下的内存、CPU、识别时延。隔离数据根，ALSA 没设备，助手的「麦克风」是 60 秒静音。
set -u
S=${MIYU_VOICE_E2E:?set MIYU_VOICE_E2E to a scratch folder, see tools/e2e/README.md}
CORE=${MIYU_CORE:?set MIYU_CORE to a built core checkout}
M=$CORE/target/release/miyu
V=$(cd "$(dirname "$0")/../.." && pwd)
W=$S/model-dl/x/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09/test_wavs
export MIYU_HOME=$S/e2e-home XDG_RUNTIME_DIR=$S/e2e-run MIYU_RESOURCES=$CORE/resources ALSA_CONFIG_PATH=/dev/null MIYU_VOICE_TEST_MIC=$S/silence60.wav
rm -rf $MIYU_HOME $S/e2e-pkg; mkdir -p $MIYU_HOME $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
for id in miyu-voice-input miyu-say miyu-voice-assistant; do mkdir -p $S/e2e-pkg/$id/bin; cp $V/package/$id/package.toml $S/e2e-pkg/$id/; cp $V/target/release/$id $S/e2e-pkg/$id/bin/; done
$M core > $S/core.log 2>&1 & CP=$!
trap 'kill $CP 2>/dev/null; wait 2>/dev/null' EXIT
for i in $(seq 50); do [ -f $MIYU_HOME/run/socket ] && break; sleep 0.2; done
mkdir -p $MIYU_HOME/var/cache/miyu-voice-input $MIYU_HOME/var/cache/miyu-voice-assistant
cp -r $S/vi-cache/sense-voice-int8-2024-07-17 $MIYU_HOME/var/cache/miyu-voice-input/
cp -r $S/va-cache/kws-zipformer-wenetspeech-3.3M-2024-01-01 $S/va-cache/silero-vad $MIYU_HOME/var/cache/miyu-voice-assistant/
for id in miyu-voice-input miyu-say miyu-voice-assistant; do $M pkg install --yes $S/e2e-pkg/$id >/dev/null 2>&1; done
pid_of() { for p in /proc/[0-9]*; do [ "$(readlink $p/exe 2>/dev/null)" = "$MIYU_HOME/opt/$1/bin/$1" ] && grep -qa -- "$2" $p/cmdline && echo ${p#/proc/}; done; }
mem() { awk '/^Rss:/{r=$2} /^Pss:/{p=$2} /^Anonymous:/{a=$2} END{printf "RSS %d MB, PSS %d MB, anon %.1f MB", r/1024, p/1024, a/1024}' /proc/$1/smaps_rollup; }
cpu_ms() { awk '{print ($14+$15)*10}' /proc/$1/stat; }
sleep 8
VI=$(pid_of miyu-voice-input serve | head -1); SAY=$(pid_of miyu-say serve | head -1); VA=$(pid_of miyu-voice-assistant serve | head -1)
echo "idle, just started:"
echo "  voice-input  $(mem $VI)"
echo "  say          $(mem $SAY)"
echo "  assistant    $(mem $VA)   (off)"
t() { local s=$(date +%s%N); python3 -I $V/tools/e2e/head.py $MIYU_HOME "blob.put {\"path\": \"$1\"}" 'bus.call {"name": "stt", "method": "transcribe", "params": {"audio": "$blob"}}' >/dev/null; echo $(( ($(date +%s%N)-s)/1000000 )); }
echo "recognize zh.wav (5.6 s of speech), round trip through the bus incl. blob upload:"
echo "  cold (worker starts, loads the model): $(t $W/zh.wav) ms"
echo "  warm: $(t $W/zh.wav) ms, $(t $W/zh.wav) ms, $(t $W/zh.wav) ms"
WK=$(pid_of miyu-voice-input recognize-worker | head -1)
echo "  worker: $(mem $WK), peak RSS $(awk '/VmHWM/{printf "%d MB", $2/1024}' /proc/$WK/status)"
echo "  voice-input after recognizing: $(mem $VI)"
sleep 5
c0=$(cpu_ms $VA); sleep 30; c1=$(cpu_ms $VA)
echo "assistant listening to silence (models loaded): $(mem $VA), CPU $(( (c1-c0)*100/30000 )).$(( (c1-c0)*1000/30000 % 10 ))% over 30 s ($((c1-c0)) ms)"
