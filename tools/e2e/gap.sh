#!/usr/bin/env bash
# 句与句之间断不断（施工 VO-15）：跑 run-turn.sh，朗读接假的合成服务（每句 2 秒音频、合成慢 1 秒），声音经 ALSA 的 file
# 插件写进一个管道，pace.py 照 wav 的每秒字节数读（放 2 秒的真要 2 秒，同真音箱）；印声卡每次开了多久、两次之间静了多久。
# 一轮念两句：连着念的是一次打开里放完（叫醒那一声另算一次）。
set -u
S=${MIYU_VOICE_E2E:?set MIYU_VOICE_E2E to a scratch folder, see tools/e2e/README.md}
V=$(cd "$(dirname "$0")/../.." && pwd)
D=$S/gap; rm -rf $D; mkdir -p $D; mkfifo $D/out.fifo
printf 'pcm.!default {\n    type file\n    slave.pcm { type null }\n    file "%s"\n    format wav\n}\n' $D/out.fifo > $D/alsa.conf
python3 -I $V/tools/e2e/pace.py $D/out.fifo > $D/pace.log & PP=$!
trap 'kill $PP 2>/dev/null' EXIT
TTS_SECONDS=2 TTS_DELAY=1 ALSA_CONFIG_PATH=$D/alsa.conf WAIT=${WAIT:-20} bash $V/tools/e2e/run-turn.sh > $D/run.log 2>&1
awk '$2=="open"{if (c) printf "  silent %.2f s\n", $1-c; printf "speaker open\n"} $2=="close"{printf "  played %s, closed\n", $3; c=$1}' $D/pace.log
