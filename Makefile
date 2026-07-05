# Thin wrapper around the canonical mise task runner (.mise.toml).
# `mise run <task>` is the single source of truth; these targets just forward
# so `make <target>` keeps working for muscle memory and tooling that expects
# a Makefile. Run `mise tasks` to see everything available.
.PHONY: build test lint fmt fmt-check check audit deny coverage bench doc clean \
        docker-build docker-test

build:
	mise run build

test:
	mise run test

lint:
	mise run lint

fmt:
	mise run fmt

fmt-check:
	mise run fmt-check

check:
	mise run check

audit:
	mise run audit

deny:
	mise run deny

coverage:
	mise run coverage

bench:
	mise run bench

doc:
	mise run doc

clean:
	mise run clean

# Docker helpers (not mise tasks — kept here for convenience).
docker-build:
	docker build -t yank-path:latest .

docker-test:
	docker compose run --rm dev cargo test
