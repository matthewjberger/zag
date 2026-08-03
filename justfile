set windows-shell := ["powershell.exe"]
export RUST_BACKTRACE := "1"

# Displays the list of available commands
@just:
    just --list

# Builds the workspace in release mode
build:
    cargo build -r

# Runs every gate: format check, lint, tests
check: format-check lint test

# Runs all tests
test:
    cargo test --workspace

# Formats the code using cargo fmt
format:
    cargo fmt --all

# Checks formatting without writing
format-check:
    cargo fmt --all -- --check

# Runs linter and displays warnings
lint:
    cargo clippy --all-targets -- -D warnings

# Fixes linting issues automatically
fix:
    cargo clippy --all-targets --fix

# Generates and opens documentation
docs:
    cargo doc --open -p zag

# Writes the worked example fact file, then ports it
#
# Nothing reads fixtures/example.zig. The Zig frontend that would turn it into
# fact tables does not exist yet, so zag-facts hand-builds the tables that
# source would yield and this runs the rest of the pipeline over them.
fixture:
    cargo run -q -p zag -- fixture --output target/example.facts
    cargo run -q -p zag -- emit --facts target/example.facts --source target/example.rs --report target/example.report.txt
    @echo "wrote target/example.rs and target/example.report.txt"

# Regenerates the checked in fixture output
#
# `cargo test` compares what the pipeline produces against fixtures/expected,
# so a deliberate change to the emitter is landed by running this and reading
# the diff. An accidental one fails the suite instead.
regenerate: fixture
    cargo run -q -p zag -- emit --facts target/example.facts --source fixtures/expected/example.rs --report fixtures/expected/example.report.txt
    cargo test -p zag

# Shows the ownership report for the worked example
report: fixture
    @cat target/example.report.txt

# Checks for unused dependencies
udeps:
    cargo machete

# Prints a table of all dependencies and their licenses
licenses:
    cargo license

# Checks for problematic licenses in dependencies
licenses-check:
    cargo deny check licenses

# Install development tools
install-tools:
    cargo install cargo-license
    cargo install cargo-deny
    cargo install cargo-machete

# Displays version information for Rust tools
@versions:
    rustc --version
    cargo fmt -- --version
    cargo clippy -- --version

# Watches for changes and runs tests
watch:
    cargo watch -x 'test --workspace'
