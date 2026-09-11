#!/usr/bin/env python3
"""
cg - 代码知识图谱便捷入口

用法:
    python scripts/cg.py [命令]

可在项目根目录下直接运行，会自动检测工作区。
"""

import os
import sys

# 将当前脚本所在目录加入 path
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPT_DIR)


def main():
    from code_graph.cli.main import main as cli_main
    cli_main()


if __name__ == "__main__":
    main()
