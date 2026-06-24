#!/usr/bin/env python3
"""
生成模拟的 make -j4 并行构建交错日志
"""
import random
from pathlib import Path

OUT = Path("benchmarks/messy_jbuild.log")
random.seed(42)

targets = ["core/utils.o", "core/string.o", "core/buffer.o",
           "net/socket.o", "net/http.o", "net/tls.o",
           "db/connection.o", "db/query.o", "db/cache.o",
           "render/canvas.o", "render/texture.o", "render/shader.o"]

gcc_flags = ["-O2", "-Wall", "-Wextra", "-g", "-std=c++17",
             "-I./include", "-I./third_party/boost",
             "-I/home/jenkins/workspace/project-A/src/core",
             "-L./lib", "-L/usr/lib/x86_64-linux-gnu",
             "-fPIC", "-DVERSION=4.2.0", "-DBUILD_ID=0xa3f7c19d"]

lines = []
counter = {t: 0 for t in targets}
for i in range(400):
    target = random.choice(targets)
    counter[target] += 1
    n = counter[target]
    if n % 17 == 0 and random.random() < 0.7:
        # 错误
        lines.append(f"[  {i:3d}s] g++ {' '.join(random.sample(gcc_flags, 6))} -c {target}.cpp -o {target}.o")
        lines.append(f"{target}.cpp:{random.randint(10, 500)}:5: error: 'undeclared identifier' in expression")
        lines.append(f"  expected primary-expression before ')' token")
        lines.append(f"  {random.randint(10000, 99999)} |     return ptr->value();")
        lines.append(f"      |                  ^")
        lines.append(f"make[3]: *** [CMakeFiles/project.dir/build.make:847: {target}.o] Error 1")
    elif n % 11 == 0:
        # 警告
        lines.append(f"[  {i:3d}s] g++ {' '.join(random.sample(gcc_flags, 6))} -c {target}.cpp -o {target}.o")
        lines.append(f"{target}.cpp:{random.randint(10, 500)}:12: warning: unused variable 'temp_buf' [-Wunused-variable]")
    else:
        # 正常
        lines.append(f"[  {i:3d}s] g++ {' '.join(random.sample(gcc_flags, 5))} -c {target}.cpp -o {target}.o")

lines.append("[  403s] make[1]: *** Waiting for unfinished jobs....")
lines.append("[  405s] make: *** Error 2")
lines.append("[  405s] Build failed. See console for details.")

OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"wrote {OUT} ({OUT.stat().st_size} bytes, {len(lines)} lines)")
