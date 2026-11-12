#!/usr/bin/env bash
# 语音助手对真核心的端到端（施工 VO-11中、VO-14）：隔离数据根；ALSA 没有设备（ALSA_CONFIG_PATH=/dev/null），开麦、出声都失败，
# 碰不到主人的麦克风、音箱。
set -u
S=${MIYU_VOICE_E2E:?set MIYU_VOICE_E2E to a scratch folder, see tools/e2e/README.md}
CORE=${MIYU_CORE:?set MIYU_CORE to a built core checkout}
M=$CORE/target/release/miyu
V=$(cd "$(dirname "$0")/../.." && pwd)
export MIYU_HOME=$S/e2e-home XDG_RUNTIME_DIR=$S/e2e-run MIYU_RESOURCES=$CORE/resources ALSA_CONFIG_PATH=/dev/null
# 通知不往桌面上弹，一条一行记进文件（施工 VO-16）。
export MIYU_VOICE_TEST_NOTICES=$S/notices.log; rm -f $S/notices.log
rm -rf $MIYU_HOME $S/e2e-pkg; mkdir -p $MIYU_HOME $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
for id in voice-input say voice-assistant; do mkdir -p $S/e2e-pkg/$id/bin; cp $V/package/$id/package.toml $S/e2e-pkg/$id/; cp $V/target/release/miyu-$id $S/e2e-pkg/$id/bin/; done
$M core > $S/core.log 2>&1 & CP=$!
trap 'kill $CP 2>/dev/null; wait 2>/dev/null' EXIT
for i in $(seq 50); do [ -f $MIYU_HOME/run/socket ] && break; sleep 0.2; done
mkdir -p $MIYU_HOME/var/cache/voice-input $MIYU_HOME/var/cache/voice-assistant
cp -r $S/vi-cache/sense-voice-int8-2024-07-17 $MIYU_HOME/var/cache/voice-input/
cp -r $S/va-cache/kws-zipformer-wenetspeech-3.3M-2024-01-01 $S/va-cache/silero-vad $MIYU_HOME/var/cache/voice-assistant/
$M pkg install --yes $S/e2e-pkg/voice-input >/dev/null 2>&1; $M pkg install --yes $S/e2e-pkg/say >/dev/null 2>&1
$M pkg install --yes $S/e2e-pkg/voice-assistant 2>&1 | tail -1
pid_of() { for p in /proc/[0-9]*; do [ "$(readlink $p/exe 2>/dev/null)" = "$MIYU_HOME/opt/$1/bin/miyu-$1" ] && grep -qa serve $p/cmdline && echo ${p#/proc/}; done; }
sleep 3
P=$(pid_of voice-assistant | head -1)
echo "1. assistant running: pid ${P:-none}; resident: $(grep VmRSS /proc/$P/status | tr -s " \t" " ")"
echo "   sound devices open: $(ls -l /proc/$P/fd 2>/dev/null | grep -c /dev/snd) (none here: ALSA has no devices)"
echo "   stderr (tried the microphone right away, said it once): $(tail -3 $MIYU_HOME/var/log/voice-assistant/stderr 2>/dev/null | tr '\n' '|')"
echo "   notices: $(cat $S/notices.log 2>/dev/null | tr '\n' '|')"
echo "2. miyu listen:     $($M listen 2>&1), again: $($M listen 2>&1)"
echo "   on: $($M listen on 2>&1), on: $($M listen on 2>&1), off: $($M listen off 2>&1), off: $($M listen off 2>&1)"
echo "3. bad argument:    $($M listen sideways 2>&1 | head -1)"
$M listen on >/dev/null
python3 -I $V/tools/e2e/head.py $MIYU_HOME 'package.restart {"package": "voice-assistant"}' >/dev/null
sleep 3; P=$(pid_of voice-assistant | head -1)
echo "4. after restart:   pid ${P:-none}, miyu listen -> $($M listen 2>&1) (a conversation does not outlive a restart; the wake word still works)"
echo "   stderr: $(tail -2 $MIYU_HOME/var/log/voice-assistant/stderr 2>/dev/null | tr '\n' '|')"
echo "5. notices:         $(cat $S/notices.log 2>/dev/null | tr '\n' '|')"
