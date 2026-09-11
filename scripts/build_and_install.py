import os
import sys
import shutil
import subprocess
import platform

def main():
    print("Building TokenSlim (debug mode)...")
    
    # 1. 编译
    result = subprocess.run(["cargo", "build", "--bin", "tokenslim"])
    if result.returncode != 0:
        print("\n[FAIL] Build failed! Please fix compilation errors.")
        sys.exit(result.returncode)

    # 2. 识别路径
    is_win = platform.system() == "Windows" or 'microsoft' in platform.release().lower()
    exe_name = "tokenslim.exe" if is_win else "tokenslim"
    src_exe = os.path.join("target", "debug", exe_name)
    
    if not os.path.exists(src_exe):
        print(f"\n[FAIL] Error: Built binary not found at {src_exe}")
        sys.exit(1)

    # 目标 1: 项目目录下的 bin/
    bin_dir = "bin"
    os.makedirs(bin_dir, exist_ok=True)
    dest_local = os.path.join(bin_dir, exe_name)

    # 目标 2: Rust 用户的默认环境变量目录 ~/.cargo/bin
    cargo_bin = os.path.expanduser("~/.cargo/bin")
    dest_global = os.path.join(cargo_bin, exe_name)

    # 3. 执行拷贝
    try:
        shutil.copy2(src_exe, dest_local)
        print(f"[OK] Copied to local bin: {dest_local}")
        
        # 顺便拷贝到全局，实现真正“零环境变量配置”
        if os.path.exists(cargo_bin):
            shutil.copy2(src_exe, dest_global)
            print(f"[OK] Copied to global path: {dest_global} (You can use `tokenslim` directly anywhere!)")
            
            # 同时必须拷贝 config 目录到 ~/.cargo/bin 目录下，否则跨目录运行时找不到插件配置
            src_config = "config"
            dest_config = os.path.join(cargo_bin, "config")
            if os.path.exists(src_config):
                # 先删除旧的 config 目录（如果存在）以保证最新
                if os.path.exists(dest_config):
                    shutil.rmtree(dest_config)
                shutil.copytree(src_config, dest_config)
                print(f"[OK] Copied config directory to: {dest_config}")
    except PermissionError:
        print("\n[WARN] Permission denied while copying. Ensure the file isn't currently in use/running.")
        sys.exit(1)
    except Exception as e:
        print(f"\n[FAIL] Copy failed: {e}")
        sys.exit(1)

    print("\n[SUCCESS] TokenSlim is ready! Try running: tokenslim workspace --format llm")

if __name__ == "__main__":
    main()
