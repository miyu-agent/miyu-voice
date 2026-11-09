#!/usr/bin/env bash
# 语音助手对真核心的端到端（施工 VO-11中）：隔离数据根；ALSA 没有设备（ALSA_CONFIG_PATH=/dev/null），开麦、出声都失败，
# 碰不到主人的麦克风、音箱。
set -u
S=${MIYU_VOICE_E2E:?set MIYU_VOICE_E2E to a scratch folder, see tools/e2e/README.md}
CORE=${MIYU_CORE:?set MIYU_CORE to a built core checkout}
M=$CORE/target/release/miyu
V=$(cd "$(dirname "$0")/../.." && pwd)
export MIYU_HOME=$S/e2e-home XDG_RUNTIME_DIR=$S/e2e-run MIYU_RESOURCES=$CORE/resources ALSA_CONFIG_PATH=/dev/null
rm -rf $MIYU_HOME $S/e2e-pkg; mkdir -p $MIYU_HOME $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
for id in miyu-voice-input miyu-say miyu-voice-assistant; do mkdir -p $S/e2e-pkg/$id/bin; cp $V/package/$id/package.toml $S/e2e-pkg/$id/; cp $V/target/release/$id $S/e2e-pkg/$id/bin/; done
$M core > $S/core.log 2>&1 & CP=$!
trap 'kill $CP 2>/dev/null; wait 2>/dev/null' EXIT
for i in $(seq 50); do [ -f $MIYU_HOME/run/socket ] && break; sleep 0.2; done
mkdir -p $MIYU_HOME/var/cache/miyu-voice-input $MIYU_HOME/var/cache/miyu-voice-assistant
cp -r $S/vi-cache/sense-voice-int8-2024-07-17 $MIYU_HOME/var/cache/miyu-voice-input/
cp -r $S/va-cache/kws-zipformer-wenetspeech-3.3M-2024-01-01 $S/va-cache/silero-vad $MIYU_HOME/var/cache/miyu-voice-assistant/
$M pkg install --yes $S/e2e-pkg/miyu-voice-input >/dev/null 2>&1; $M pkg install --yes $S/e2e-pkg/miyu-say >/dev/null 2>&1
$M pkg install --yes $S/e2e-pkg/miyu-voice-assistant 2>&1 | tail -1
pid_of() { for p in /proc/[0-9]*; do [ "$(readlink $p/exe 2>/dev/null)" = "$MIYU_HOME/opt/$1/bin/$1" ] && grep -qa serve $p/cmdline && echo ${p#/proc/}; done; }
sleep 3
P=$(pid_of miyu-voice-assistant | head -1)
echo "1. assistant running: pid ${P:-none}; state file: $(cat $MIYU_HOME/var/lib/miyu-voice-assistant/listening 2>/dev/null || echo none); resident while off: $(grep VmRSS /proc/$P/status | tr -s " \t" " ")"
echo "2. miyu listen:     $($M listen 2>&1)"
echo "   state file:      $(cat $MIYU_HOME/var/lib/miyu-voice-assistant/listening 2>/dev/null)"
sleep 3
echo "   sound devices open: $(ls -l /proc/$P/fd 2>/dev/null | grep -c /dev/snd)"
echo "   stderr: $(tail -3 $MIYU_HOME/var/log/miyu-voice-assistant/stderr 2>/dev/null | tr '\n' '|')"
echo "3. miyu listen off: $($M listen off 2>&1)"
echo "4. miyu listen on:  $($M listen on 2>&1)"
python3 -I $V/tools/e2e/head.py $MIYU_HOME 'package.restart {"package": "miyu-voice-assistant"}' >/dev/null
sleep 3; echo "5. after restart, listen off -> $($M listen off 2>&1) (was on, so it remembered)"
echo "6. bad switch: $($M listen sideways 2>&1 | head -1)"
echo "   resident: $(grep VmRSS /proc/$(pid_of miyu-voice-assistant | head -1)/status 2>/dev/null)"
