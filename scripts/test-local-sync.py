#!/usr/bin/env python3
"""
Silent-CRDT 本地多节点同步测试脚本 (Python 版本)
用于验证 README 中描述的本地测试步骤
"""

import os
import sys
import time
import json
import signal
import subprocess
import requests
from typing import Optional, Dict, List
from pathlib import Path

# 配置
PORT1 = 8080
PORT2 = 8081
NODE1_ID = "node1"
NODE2_ID = "node2"
DATA_DIR1 = "./test_data/node1"
DATA_DIR2 = "./test_data/node2"
TIMEOUT = 30
WAIT_TIME = 2

# 全局进程列表
processes = []


class Colors:
    """终端颜色代码"""

    RED = "\033[0;31m"
    GREEN = "\033[0;32m"
    YELLOW = "\033[1;33m"
    BLUE = "\033[0;34m"
    NC = "\033[0m"  # No Color


def print_colored(message: str, color: str = Colors.NC):
    """打印彩色消息"""
    print(f"{color}{message}{Colors.NC}")


def cleanup():
    """清理测试环境"""
    print_colored("\n清理测试环境...", Colors.YELLOW)

    # 停止所有子进程
    for proc in processes:
        try:
            proc.terminate()
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        except Exception as e:
            print(f"清理进程时出错: {e}")

    # 清理测试数据
    import shutil

    if os.path.exists("test_data"):
        shutil.rmtree("test_data")

    # 清理日志文件
    for log_file in ["node1.log", "node2.log"]:
        if os.path.exists(log_file):
            os.remove(log_file)

    print_colored("✓ 清理完成", Colors.GREEN)


def signal_handler(signum, frame):
    """信号处理器"""
    print_colored("\n收到中断信号，清理并退出...", Colors.YELLOW)
    cleanup()
    sys.exit(0)


def check_port(port: int) -> bool:
    """检查端口是否可用"""
    import socket

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    result = sock.connect_ex(("127.0.0.1", port))
    sock.close()
    return result != 0


def wait_for_service(port: int, max_wait: int = TIMEOUT) -> bool:
    """等待服务启动"""
    print_colored(f"等待端口 {port} 上的服务启动...", Colors.YELLOW)

    for i in range(max_wait):
        try:
            response = requests.get(f"http://127.0.0.1:{port}/health", timeout=1)
            if response.status_code == 200:
                print_colored(f"✓ 端口 {port} 上的服务已启动", Colors.GREEN)
                return True
        except requests.exceptions.RequestException:
            pass

        print(".", end="", flush=True)
        time.sleep(1)

    print()
    print_colored("✗ 服务启动超时", Colors.RED)
    return False


def start_node(
    port: int, node_id: str, data_path: str, log_file: str
) -> Optional[subprocess.Popen]:
    """启动节点"""
    Path(data_path).mkdir(parents=True, exist_ok=True)

    cmd = [
        "cargo",
        "run",
        "--release",
        "--",
        "--port",
        str(port),
        "--node-id",
        node_id,
        "--data-path",
        data_path,
    ]

    with open(log_file, "w") as log:
        proc = subprocess.Popen(
            cmd,
            stdout=log,
            stderr=subprocess.STDOUT,
            preexec_fn=os.setsid if sys.platform != "win32" else None,
        )

    processes.append(proc)
    return proc


def post_sync(port: int, changes: List[Dict]) -> Dict:
    """提交变更到节点"""
    url = f"http://127.0.0.1:{port}/sync"
    response = requests.post(url, json={"changes": changes})
    response.raise_for_status()
    return response.json()


def get_state_hash(port: int) -> str:
    """获取节点状态哈希"""
    url = f"http://127.0.0.1:{port}/state-hash"
    response = requests.get(url)
    response.raise_for_status()
    return response.json()["hash"]


def get_state(port: int) -> Dict:
    """获取节点完整状态"""
    url = f"http://127.0.0.1:{port}/state"
    response = requests.get(url)
    response.raise_for_status()
    return response.json()


def sync_peer(from_port: int, to_port: int) -> Dict:
    """触发节点间同步"""
    url = f"http://127.0.0.1:{from_port}/sync-peer"
    response = requests.post(url, json={"peer": f"127.0.0.1:{to_port}"})
    response.raise_for_status()
    return response.json()


def main():
    """主测试流程"""
    # 注册信号处理器
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        print_colored("=" * 50, Colors.GREEN)
        print_colored("Silent-CRDT 本地多节点同步测试", Colors.GREEN)
        print_colored("=" * 50, Colors.GREEN)
        print()

        # 步骤 0: 检查并编译项目
        print_colored("[步骤 0] 编译项目...", Colors.YELLOW)
        result = subprocess.run(["cargo", "build", "--release"], capture_output=True)
        if result.returncode != 0:
            print_colored("✗ 编译失败", Colors.RED)
            print(result.stderr.decode())
            return 1
        print_colored("✓ 编译完成", Colors.GREEN)
        print()

        # 步骤 1: 检查端口可用性
        print_colored("[步骤 1] 检查端口可用性...", Colors.YELLOW)
        if not check_port(PORT1):
            print_colored(f"✗ 端口 {PORT1} 已被占用", Colors.RED)
            return 1
        if not check_port(PORT2):
            print_colored(f"✗ 端口 {PORT2} 已被占用", Colors.RED)
            return 1
        print_colored("✓ 端口检查通过", Colors.GREEN)
        print()

        # 步骤 2: 启动节点 1
        print_colored(f"[步骤 2] 启动节点 1 (端口 {PORT1})...", Colors.YELLOW)
        node1_proc = start_node(PORT1, NODE1_ID, DATA_DIR1, "node1.log")
        print(f"节点 1 PID: {node1_proc.pid}")
        if not wait_for_service(PORT1):
            return 1
        print_colored("✓ 节点 1 启动成功", Colors.GREEN)
        print()

        # 步骤 3: 启动节点 2
        print_colored(f"[步骤 3] 启动节点 2 (端口 {PORT2})...", Colors.YELLOW)
        node2_proc = start_node(PORT2, NODE2_ID, DATA_DIR2, "node2.log")
        print(f"节点 2 PID: {node2_proc.pid}")
        if not wait_for_service(PORT2):
            return 1
        print_colored("✓ 节点 2 启动成功", Colors.GREEN)
        print()

        # 步骤 4: 向节点 1 提交变更
        print_colored("[步骤 4] 向节点 1 提交变更...", Colors.YELLOW)
        changes = [
            {"op": "add", "key": "user", "value": "Alice"},
            {"op": "increment", "key": "counter", "delta": 5},
            {"op": "set", "key": "status", "value": "active"},
        ]
        response = post_sync(PORT1, changes)
        print(f"响应: {json.dumps(response, indent=2)}")

        hash1_before = get_state_hash(PORT1)
        print(f"节点 1 状态哈希: {hash1_before}")
        print_colored("✓ 变更提交成功", Colors.GREEN)
        print()

        time.sleep(WAIT_TIME)

        # 步骤 5: 获取节点 2 同步前的状态哈希
        print_colored("[步骤 5] 检查节点 2 同步前的状态...", Colors.YELLOW)
        hash2_before = get_state_hash(PORT2)
        print(f"节点 2 状态哈希（同步前）: {hash2_before}")

        if hash1_before == hash2_before:
            print_colored("✗ 警告: 同步前两个节点的状态哈希已经相同", Colors.RED)
        else:
            print_colored("✓ 同步前两个节点的状态哈希不同（符合预期）", Colors.GREEN)
        print()

        # 步骤 6: 触发节点间同步
        print_colored("[步骤 6] 触发节点 1 -> 节点 2 的同步...", Colors.YELLOW)
        sync_response = sync_peer(PORT1, PORT2)
        print(f"同步响应: {json.dumps(sync_response, indent=2)}")
        print_colored("✓ 同步请求已发送", Colors.GREEN)
        print()

        time.sleep(WAIT_TIME)

        # 步骤 7: 验证节点 2 的状态
        print_colored("[步骤 7] 验证节点 2 的状态...", Colors.YELLOW)
        hash2_after = get_state_hash(PORT2)
        print(f"节点 2 状态哈希（同步后）: {hash2_after}")
        print()

        # 步骤 8: 验证状态收敛
        print_colored("[步骤 8] 验证状态收敛性...", Colors.YELLOW)
        if hash1_before == hash2_after:
            print_colored("✓✓✓ 状态收敛验证成功！", Colors.GREEN)
            print_colored("节点 1 和节点 2 的状态哈希一致", Colors.GREEN)
        else:
            print_colored("✗✗✗ 状态收敛验证失败！", Colors.RED)
            print_colored(f"节点 1 哈希: {hash1_before}", Colors.RED)
            print_colored(f"节点 2 哈希: {hash2_after}", Colors.RED)
            return 1
        print()

        # 步骤 9: 显示详细状态对比
        print_colored("[步骤 9] 显示详细状态对比...", Colors.YELLOW)
        print("\n=== 节点 1 状态 ===")
        state1 = get_state(PORT1)
        print(json.dumps(state1, indent=2))

        print("\n=== 节点 2 状态 ===")
        state2 = get_state(PORT2)
        print(json.dumps(state2, indent=2))
        print()

        # 步骤 10: 测试反向同步
        print_colored("[步骤 10] 测试反向同步...", Colors.YELLOW)
        print("向节点 2 添加新数据...")
        post_sync(PORT2, [{"op": "add", "key": "user", "value": "Bob"}])

        time.sleep(WAIT_TIME)

        print("触发节点 2 -> 节点 1 的同步...")
        sync_peer(PORT2, PORT1)

        time.sleep(WAIT_TIME)

        hash1_final = get_state_hash(PORT1)
        hash2_final = get_state_hash(PORT2)

        print(f"最终节点 1 哈希: {hash1_final}")
        print(f"最终节点 2 哈希: {hash2_final}")

        if hash1_final == hash2_final:
            print_colored("✓ 双向同步验证成功！", Colors.GREEN)
        else:
            print_colored("✗ 双向同步验证失败", Colors.RED)
            return 1
        print()

        # 测试完成
        print_colored("=" * 50, Colors.GREEN)
        print_colored("✓✓✓ 所有测试通过！", Colors.GREEN)
        print_colored("=" * 50, Colors.GREEN)
        print()
        print("日志文件：")
        print("  - 节点 1: node1.log")
        print("  - 节点 2: node2.log")

        return 0

    except Exception as e:
        print_colored(f"\n✗ 测试过程中出错: {e}", Colors.RED)
        import traceback

        traceback.print_exc()
        return 1
    finally:
        cleanup()


if __name__ == "__main__":
    sys.exit(main())
