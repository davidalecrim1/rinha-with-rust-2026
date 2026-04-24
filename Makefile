.PHONY: lint run test

lint:
	cargo fmt --check
	cargo clippy -- -D warnings

test:
	cargo test

run:
	docker compose -f docker-compose.local.yml up --build -d
