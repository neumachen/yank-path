.PHONY: build test lint fmt fmt-check docker-build docker-test clean

build:
	cargo build --release

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

docker-build:
	docker build -t yank-path:latest .

docker-test:
	docker-compose run --rm dev cargo test

clean:
	cargo clean
