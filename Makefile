CARGO ?= cargo

.PHONY: fmt fmt-check typecheck unit integration e2e test lint audit coverage install ci

fmt:
	$(CARGO) fmt

fmt-check:
	$(CARGO) fmt --check

typecheck:
	$(CARGO) check --all-targets --all-features

unit:
	$(CARGO) test --lib --bins --all-features

integration:
	$(CARGO) test --tests --all-features

e2e:
	$(CARGO) test --test cli_test --test server_test --all-features

test:
	$(CARGO) test --all-targets --all-features

lint:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

audit:
	@PATH="$(HOME)/.cargo/bin:$(PATH)" $(CARGO) audit

coverage:
	$(CARGO) llvm-cov --all-targets --all-features

install:
	PATH="$(HOME)/.cargo/bin:$(PATH)" $(CARGO) install --path . --locked

ci: fmt-check typecheck test lint
