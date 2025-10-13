#!/bin/bash

# 本地多节点同步测试脚本
# 用于验证 README 中描述的本地测试步骤

set -e  # 遇到错误立即退出

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 配置
PORT1=8080
PORT2=8081
NODE1_ID="node1"
NODE2_ID="node2"
DATA_DIR1="./test_data/node1"
DATA_DIR2="./test_data/node2"
TIMEOUT=30
WAIT_TIME=2

# 清理函数
cleanup() {
    echo -e "${YELLOW}清理测试环境...${NC}"
    # 杀死可能存在的进程
    pkill -f "silent-crdt --port $PORT1" 2>/dev/null || true
    pkill -f "silent-crdt --port $PORT2" 2>/dev/null || true
    sleep 1
    # 清理测试数据
    rm -rf test_data
    echo -e "${GREEN}清理完成${NC}"
}

# 设置退出时清理
trap cleanup EXIT

# 检查端口是否可用
check_port() {
    local port=$1
    if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1 ; then
        echo -e "${RED}端口 $port 已被占用${NC}"
        lsof -Pi :$port -sTCP:LISTEN
        return 1
    fi
    return 0
}

# 等待服务启动
wait_for_service() {
    local port=$1
    local max_wait=$2
    local waited=0

    echo -e "${YELLOW}等待端口 $port 上的服务启动...${NC}"
    while [ $waited -lt $max_wait ]; do
        if curl -s http://127.0.0.1:$port/health > /dev/null 2>&1; then
            echo -e "${GREEN}端口 $port 上的服务已启动${NC}"
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
        echo -n "."
    done
    echo ""
    echo -e "${RED}服务启动超时${NC}"
    return 1
}

# 主测试流程
main() {
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}Silent-CRDT 本地多节点同步测试${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""

    # 步骤 0: 检查并编译项目
    echo -e "${YELLOW}[步骤 0] 编译项目...${NC}"
    cargo build --release
    echo -e "${GREEN}✓ 编译完成${NC}"
    echo ""

    # 步骤 1: 检查端口可用性
    echo -e "${YELLOW}[步骤 1] 检查端口可用性...${NC}"
    check_port $PORT1 || exit 1
    check_port $PORT2 || exit 1
    echo -e "${GREEN}✓ 端口检查通过${NC}"
    echo ""

    # 步骤 2: 启动节点 1
    echo -e "${YELLOW}[步骤 2] 启动节点 1 (端口 $PORT1)...${NC}"
    mkdir -p $DATA_DIR1
    cargo run --release -- --port $PORT1 --node-id $NODE1_ID --data-path $DATA_DIR1 > node1.log 2>&1 &
    NODE1_PID=$!
    echo "节点 1 PID: $NODE1_PID"
    wait_for_service $PORT1 $TIMEOUT || exit 1
    echo -e "${GREEN}✓ 节点 1 启动成功${NC}"
    echo ""

    # 步骤 3: 启动节点 2
    echo -e "${YELLOW}[步骤 3] 启动节点 2 (端口 $PORT2)...${NC}"
    mkdir -p $DATA_DIR2
    cargo run --release -- --port $PORT2 --node-id $NODE2_ID --data-path $DATA_DIR2 > node2.log 2>&1 &
    NODE2_PID=$!
    echo "节点 2 PID: $NODE2_PID"
    wait_for_service $PORT2 $TIMEOUT || exit 1
    echo -e "${GREEN}✓ 节点 2 启动成功${NC}"
    echo ""

    # 步骤 4: 向节点 1 提交变更
    echo -e "${YELLOW}[步骤 4] 向节点 1 提交变更...${NC}"
    RESPONSE=$(curl -s -X POST http://127.0.0.1:$PORT1/sync \
        -H "Content-Type: application/json" \
        -d '{"changes":[
            {"op":"add","key":"user","value":"Alice"},
            {"op":"increment","key":"counter","delta":5},
            {"op":"set","key":"status","value":"active"}
        ]}')
    echo "响应: $RESPONSE"

    # 获取节点 1 的状态哈希
    HASH1_BEFORE=$(curl -s http://127.0.0.1:$PORT1/state-hash | jq -r '.hash')
    echo "节点 1 状态哈希: $HASH1_BEFORE"
    echo -e "${GREEN}✓ 变更提交成功${NC}"
    echo ""

    sleep $WAIT_TIME

    # 步骤 5: 获取节点 2 同步前的状态哈希
    echo -e "${YELLOW}[步骤 5] 检查节点 2 同步前的状态...${NC}"
    HASH2_BEFORE=$(curl -s http://127.0.0.1:$PORT2/state-hash | jq -r '.hash')
    echo "节点 2 状态哈希（同步前）: $HASH2_BEFORE"

    if [ "$HASH1_BEFORE" == "$HASH2_BEFORE" ]; then
        echo -e "${RED}✗ 警告: 同步前两个节点的状态哈希已经相同${NC}"
    else
        echo -e "${GREEN}✓ 同步前两个节点的状态哈希不同（符合预期）${NC}"
    fi
    echo ""

    # 步骤 6: 触发节点间同步
    echo -e "${YELLOW}[步骤 6] 触发节点 1 -> 节点 2 的同步...${NC}"
    SYNC_RESPONSE=$(curl -s -X POST http://127.0.0.1:$PORT1/sync-peer \
        -H "Content-Type: application/json" \
        -d '{"peer":"127.0.0.1:'$PORT2'"}')
    echo "同步响应: $SYNC_RESPONSE"
    echo -e "${GREEN}✓ 同步请求已发送${NC}"
    echo ""

    sleep $WAIT_TIME

    # 步骤 7: 验证节点 2 的状态
    echo -e "${YELLOW}[步骤 7] 验证节点 2 的状态...${NC}"
    HASH2_AFTER=$(curl -s http://127.0.0.1:$PORT2/state-hash | jq -r '.hash')
    echo "节点 2 状态哈希（同步后）: $HASH2_AFTER"
    echo ""

    # 步骤 8: 验证状态收敛
    echo -e "${YELLOW}[步骤 8] 验证状态收敛性...${NC}"
    if [ "$HASH1_BEFORE" == "$HASH2_AFTER" ]; then
        echo -e "${GREEN}✓✓✓ 状态收敛验证成功！${NC}"
        echo -e "${GREEN}节点 1 和节点 2 的状态哈希一致${NC}"
    else
        echo -e "${RED}✗✗✗ 状态收敛验证失败！${NC}"
        echo -e "${RED}节点 1 哈希: $HASH1_BEFORE${NC}"
        echo -e "${RED}节点 2 哈希: $HASH2_AFTER${NC}"
        exit 1
    fi
    echo ""

    # 步骤 9: 显示详细状态对比
    echo -e "${YELLOW}[步骤 9] 显示详细状态对比...${NC}"
    echo ""
    echo "=== 节点 1 状态 ==="
    curl -s http://127.0.0.1:$PORT1/state | jq '.'
    echo ""
    echo "=== 节点 2 状态 ==="
    curl -s http://127.0.0.1:$PORT2/state | jq '.'
    echo ""

    # 步骤 10: 测试反向同步（可选）
    echo -e "${YELLOW}[步骤 10] 测试反向同步...${NC}"
    echo "向节点 2 添加新数据..."
    curl -s -X POST http://127.0.0.1:$PORT2/sync \
        -H "Content-Type: application/json" \
        -d '{"changes":[{"op":"add","key":"user","value":"Bob"}]}' > /dev/null

    sleep $WAIT_TIME

    echo "触发节点 2 -> 节点 1 的同步..."
    curl -s -X POST http://127.0.0.1:$PORT2/sync-peer \
        -H "Content-Type: application/json" \
        -d '{"peer":"127.0.0.1:'$PORT1'"}' > /dev/null

    sleep $WAIT_TIME

    HASH1_FINAL=$(curl -s http://127.0.0.1:$PORT1/state-hash | jq -r '.hash')
    HASH2_FINAL=$(curl -s http://127.0.0.1:$PORT2/state-hash | jq -r '.hash')

    echo "最终节点 1 哈希: $HASH1_FINAL"
    echo "最终节点 2 哈希: $HASH2_FINAL"

    if [ "$HASH1_FINAL" == "$HASH2_FINAL" ]; then
        echo -e "${GREEN}✓ 双向同步验证成功！${NC}"
    else
        echo -e "${RED}✗ 双向同步验证失败${NC}"
        exit 1
    fi
    echo ""

    # 测试完成
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}✓✓✓ 所有测试通过！${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo "日志文件："
    echo "  - 节点 1: node1.log"
    echo "  - 节点 2: node2.log"
}

# 运行主测试
main "$@"
