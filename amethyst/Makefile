lint:
	cargo clippy --fix --allow-dirty

test: lint
	cargo test --no-default-features

testall: test
	cargo test
