fmt:
    cargo fmt
lint:
    cargo clippy --workspace --all-targets -- -D warnings
test:
    cargo test --workspace --locked
check:
    cargo fmt --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo test --workspace --locked
    cargo clippy -p atlas-lib --no-default-features --all-targets --locked -- -D warnings
    cargo test -p atlas-lib --no-default-features --locked
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
    npm test
build:
    cargo build --release --locked
verify-vendor:
    npm ci --ignore-scripts --no-audit --no-fund
    npm run verify-vendor
check-install:
    renderer/check-install.sh
# Publish the described jj change: run `just check` on exactly it, then push it
land *args:
    scripts/jj-land.sh {{args}}
