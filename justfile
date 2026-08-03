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
    cargo run -q -p zag -- facts --example fixture --output target/example.facts
    cargo run -q -p zag -- emit --facts target/example.facts --source target/example.rs --report target/example.report.txt
    @echo "wrote target/example.rs and target/example.report.txt"

# Ports every example program and rewrites the expected output beside each one
#
# `cargo test` compares against those files, so a deliberate change to the
# emitter is landed by running this and reading the diff.
[windows]
examples:
    @foreach ($name in (cargo run -q -p zag -- examples)) { if (Test-Path "examples/$name") { cargo run -q -p zag -- facts --example $name --output "target/$name.facts"; cargo run -q -p zag -- emit --facts "target/$name.facts" --source "examples/$name/expected/port.rs" --report "examples/$name/expected/port.report.txt"; Write-Host "ported $name" } }

# Ports every example program and rewrites the expected output beside each one
[unix]
examples:
    #!/usr/bin/env bash
    set -euo pipefail
    for name in $(cargo run -q -p zag -- examples); do
        [ -d "examples/$name" ] || continue
        cargo run -q -p zag -- facts --example "$name" --output "target/$name.facts"
        cargo run -q -p zag -- emit --facts "target/$name.facts" \
            --source "examples/$name/expected/port.rs" \
            --report "examples/$name/expected/port.report.txt"
        echo "ported $name"
    done

# Prints what the Zig compiler resolved about one example, which is what the
# hand-built fact tables are checked against. Needs zig on PATH
reflect name:
    zig run --dep target -Mroot=tools/reflect/main.zig -Mtarget=examples/{{name}}/src/main.zig

# Builds and runs every example Zig program. Needs zig on PATH
[windows]
examples-zig:
    @Get-ChildItem examples -Directory | ForEach-Object { Write-Host "== $($_.Name)"; Push-Location $_.FullName; zig build run; if ($LASTEXITCODE -ne 0) { Pop-Location; throw "$($_.Name) failed" }; Pop-Location }

# Builds and runs every example Zig program. Needs zig on PATH
[unix]
examples-zig:
    #!/usr/bin/env bash
    set -euo pipefail
    for directory in examples/*/; do
        echo "== $directory"
        (cd "$directory" && zig build run)
    done

# Regenerates the checked in fixture output
#
# `cargo test` compares what the pipeline produces against fixtures/expected,
# so a deliberate change to the emitter is landed by running this and reading
# the diff. An accidental one fails the suite instead.
regenerate: fixture examples
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
