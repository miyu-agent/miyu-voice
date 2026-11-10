#!/usr/bin/env bash
# 语音助手一整轮对真核心（施工 VO-11下）：假麦克风（KWS 的 5.wav，唤醒词「周望军」）→ 语音输入认字 → voice 场所的会话 →
# 假的对话服务流式回「好的，三点见。明天再聊！」→ 一句句交 tts.play（朗读接假的合成服务，ALSA 没设备、放不出来）。
# BY_COMMAND=1：不喊唤醒词，假麦克风换成 SenseVoice 带的 zh.wav（前面垫 6 秒安静），助手起来以后敲 `miyu listen` 开对话，
# 一轮完了 `miyu listen off` 关（施工 VO-14）。
set -u
S=${MIYU_VOICE_E2E:?set MIYU_VOICE_E2E to a scratch folder, see tools/e2e/README.md}
CORE=${MIYU_CORE:?set MIYU_CORE to a built core checkout}
M=$CORE/target/release/miyu
V=$(cd "$(dirname "$0")/../.." && pwd)
export MIYU_HOME=$S/e2e-home XDG_RUNTIME_DIR=$S/e2e-run MIYU_RESOURCES=$CORE/resources ALSA_CONFIG_PATH=${ALSA_CONFIG_PATH:-/dev/null}
export MIYU_VOICE_TEST_MIC=$S/kws-dl/x/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01/test_wavs/5.wav
[ -z "${BY_COMMAND:-}" ] || export MIYU_VOICE_TEST_MIC=$(ls $S/model-dl/x/*/test_wavs/zh.wav | head -1) MIYU_VOICE_TEST_MIC_LEAD=6
rm -rf $MIYU_HOME $S/e2e-pkg $S/fake.log $S/chat.log; mkdir -p $MIYU_HOME $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR
for id in miyu-voice-input miyu-say miyu-voice-assistant; do mkdir -p $S/e2e-pkg/$id/bin; cp $V/package/$id/package.toml $S/e2e-pkg/$id/; cp $V/target/release/$id $S/e2e-pkg/$id/bin/; done
python3 -I $V/tools/e2e/fake_tts.py 18931 $S/fake.log ${TTS_SECONDS:-0.1} & FT=$!
python3 -I $V/tools/e2e/fake_chat.py 18932 $S/chat.log & FC=$!
$M core > $S/core.log 2>&1 & CP=$!
trap 'kill $CP $FT $FC 2>/dev/null; wait 2>/dev/null' EXIT
for i in $(seq 50); do [ -f $MIYU_HOME/run/socket ] && break; sleep 0.2; done
mkdir -p $MIYU_HOME/var/cache/miyu-voice-input $MIYU_HOME/var/cache/miyu-voice-assistant
cp -r $S/vi-cache/sense-voice-int8-2024-07-17 $MIYU_HOME/var/cache/miyu-voice-input/
cp -r $S/va-cache/kws-zipformer-wenetspeech-3.3M-2024-01-01 $S/va-cache/silero-vad $MIYU_HOME/var/cache/miyu-voice-assistant/
c() { $M config set "$@" >/dev/null || echo "config set $1 failed"; }
[ -z "${PERSONA_VOICE:-}" ] || c persona.default software-engineer
c providers.fake.base_url http://127.0.0.1:18932/v1; c providers.fake.driver openai-chat; c models.chat fake/fake-model
# 核心 VA-1 合进来以前，照主人对应表手写一条，效果同 VA-1。
[ -n "${NO_BINDING:-}" ] || c 'external.bindings."voice:owner"' owner
for id in miyu-voice-input miyu-say miyu-voice-assistant; do $M pkg install --yes $S/e2e-pkg/$id 2>&1 | tail -1; done
c say.engine openai; c say.openai.base_url http://127.0.0.1:18931/v1; c say.openai.model fake-tts
c voice-assistant.wake_words '["周望军"]'
[ -z "${FULL_DUPLEX:-}" ] || c voice-assistant.full_duplex true
[ -z "${PERSONA_VOICE:-}" ] || c say.personas '[{"persona": "software-engineer", "voice": "'"$PERSONA_VOICE"'", "speed": 1.2}]'
$M package restart miyu-voice-assistant >/dev/null 2>&1 || python3 -I $V/tools/e2e/head.py $MIYU_HOME 'package.restart {"package": "miyu-voice-assistant"}' >/dev/null
sleep 3
echo "--- bus"; $M bus list 2>&1 | tail -3
[ -z "${PLAY_TIMING:-}" ] || { s=$(date +%s%N); $M say 你好 2>&1 | tail -1; e=$(date +%s%N); echo "--- miyu say took $(( (e-s)/1000000 )) ms"; }
[ -z "${BY_COMMAND:-}" ] || echo "--- miyu listen: $($M listen 2>&1)"
sleep ${WAIT:-20}
[ -z "${BY_COMMAND:-}" ] || echo "--- miyu listen off: $($M listen off 2>&1)"
echo "--- assistant stderr"; tail -8 $MIYU_HOME/var/log/miyu-voice-assistant/stderr 2>/dev/null
echo "--- what the model was sent (last user message)"; python3 -I -c "import json,sys; ls=[json.loads(l) for l in open(sys.argv[1])]; m=ls[-1]['messages'] if ls else []; print([x for x in m if x['role']=='user'][-1:] if m else 'nothing'); print('spoken fact present:', any('spoken' in json.dumps(x) for x in m))" $S/chat.log 2>&1
echo "--- what tts was asked to say"; python3 -I -c "import json,sys; [print(json.loads(l)['body']['input'], '| voice', json.loads(l)['body']['voice'], '| speed', json.loads(l)['body']['speed']) for l in open(sys.argv[1])]" $S/fake.log 2>&1
echo "--- session events"; python3 -I -c "import json,sys,glob; [print(e['kind'], json.dumps(e.get('body',{}),ensure_ascii=False)[:160]) for f in glob.glob(sys.argv[1]+'/root/sessions/*/0*.jsonl') for e in map(json.loads,open(f)) if e['kind'] in ('message.user','context.injected','turn.ended','message.assistant') and not (e['kind']=='context.injected' and e['by'].get('kind')=='kernel')]" $MIYU_HOME
echo "--- say tool files"; ls /root/workspace/*/audio/ 2>/dev/null; echo "--- tool results"; python3 -I -c "import json,sys,glob; [print(json.dumps(e['body'],ensure_ascii=False)[:200]) for f in glob.glob(sys.argv[1]+'/root/sessions/*/0*.jsonl') for e in map(json.loads,open(f)) if e['kind']=='tool.result']" 
