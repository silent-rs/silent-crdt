.PHONY: test coverage coverage-html clean check test-sync test-sync-py help

# 默认目标：显示帮助
help:
	@echo "Silent-CRDT Makefile 命令："
	@echo ""
	@echo "  make test             - 运行单元测试"
	@echo "  make test-sync        - 运行本地多节点同步测试 (Bash)"
	@echo "  make test-sync-py     - 运行本地多节点同步测试 (Python)"
	@echo "  make coverage         - 生成覆盖率报告"
	@echo "  make coverage-html    - 生成 HTML 覆盖率报告"
	@echo "  make check            - 运行所有检查（测试+格式+lint）"
	@echo "  make clean            - 清理构建文件"
	@echo "  make clean-test       - 清理测试生成的文件"

# 运行单元测试
test:
	cargo test

# 运行本地多节点同步测试 (Bash 版本)
test-sync:
	@echo "运行本地多节点同步测试（Bash 脚本）..."
	@./scripts/test-local-sync.sh

# 运行本地多节点同步测试 (Python 版本)
test-sync-py:
	@echo "运行本地多节点同步测试（Python 脚本）..."
	@python3 scripts/test-local-sync.py

# 生成覆盖率报告（仅统计本项目代码，排除 silent 依赖）
coverage:
	@echo "生成覆盖率报告（排除 silent 框架）..."
	@cargo llvm-cov --all-features --ignore-filename-regex '.*/silent/silent/.*' --summary-only

# 生成 HTML 覆盖率报告
coverage-html:
	@echo "生成 HTML 覆盖率报告（排除 silent 框架）..."
	@cargo llvm-cov --all-features --ignore-filename-regex '.*/silent/silent/.*' --html
	@echo "HTML 报告已生成到: target/llvm-cov/html/index.html"

# 清理构建文件
clean:
	cargo clean
	rm -rf target/llvm-cov target/llvm-cov-target

# 清理测试生成的文件
clean-test:
	@echo "清理测试生成的文件..."
	rm -rf test_data/
	rm -f node*.log
	pkill -f "silent-crdt --port" || true
	@echo "清理完成"

# 运行所有检查
check: test
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	pre-commit run --all-files
