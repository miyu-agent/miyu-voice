# 实时的假音箱：ALSA file 插件把放出来的 wav 写进管道，这里照 wav 头里的每秒字节数读，放 2 秒的就真要 2 秒。
# 每次打开（一段 wav 头）记一行 open，读完记一行 close 和放了几秒，印到标准输出（施工 VO-15）。
import os, struct, sys, time
fifo = sys.argv[1]
while True:
    with open(fifo, "rb") as f:
        head = f.read(44)
        if len(head) < 44:
            continue
        rate = struct.unpack("<I", head[28:32])[0] or 1
        print(f"{time.time():.3f} open byte_rate={rate}", flush=True)
        start = time.time(); total = 0
        while True:
            chunk = f.read(4096)
            if not chunk:
                break
            total += len(chunk)
            ahead = total / rate - (time.time() - start)
            if ahead > 0:
                time.sleep(ahead)
        print(f"{time.time():.3f} close {total / rate:.2f}s", flush=True)
