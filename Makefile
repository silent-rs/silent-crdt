.PHONY: test coverage coverage-html clean

# 运行测试
test:
	cargo test

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

# 运行所有检查
check: test
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	pre-commit run --all-files
