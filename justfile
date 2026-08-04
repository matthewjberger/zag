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
    -zig build-obj -fno-emit-bin --dep target -Mroot=crates/zag/tools/reflect/main.zig -Mtarget=examples/{{name}}/src/main.zig

# Prints the dataflow the parser found in one example, which is the other half
# of what the hand-built fact tables are checked against. Needs zig on PATH
extract name:
    zig run crates/zag/tools/extract/main.zig -- examples/{{name}}/src/main.zig

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

# Ports one example and prints what came out. `just port tokenizer`
#
# Writes into target/, so the output checked in under examples/ is left alone.
# `just examples` is the one that rewrites those. `just names` lists what can
# go here, and the Zig each name refers to is examples/<name>/src/main.zig.
port name:
    @cargo run -q -p zag -- facts --example {{name}} --output target/{{name}}.facts
    @cargo run -q -p zag -- emit --facts target/{{name}}.facts --source target/{{name}}.rs --report target/{{name}}.report.txt
    @echo "=== target/{{name}}.rs ==="
    @cat target/{{name}}.rs
    @echo "=== target/{{name}}.report.txt ==="
    @cat target/{{name}}.report.txt

# Ports a Zig file the repository knows nothing about. `just read path/to.zig`
#
# Reads it with the compiler, builds fact tables from what it finds, and prints
# the Rust and the report. Needs zig on PATH.
read file:
    @cargo run -q -p zag -- read --zig {{file}} --output target/read.facts
    @cargo run -q -p zag -- emit --facts target/read.facts --source target/read.rs --report target/read.report.txt
    @echo "=== target/read.rs ==="
    @cat target/read.rs
    @echo "=== target/read.report.txt ==="
    @cat target/read.report.txt

# Ports one example into a crate you can build. `just build ledger`
#
# A Zig file becomes a Rust module in its own file rather than a `pub mod`
# block, which is what a port a person keeps looks like. A program that names a
# package the crawl could not read gets a workspace, with a crate standing in
# for each one.
build-crate name:
    @cargo run -q -p zag -- facts --example {{name}} --output target/{{name}}.facts
    cargo run -q -p zag -- build --facts target/{{name}}.facts --into target/{{name}}-crate --name {{name}}
    @cargo build --manifest-path target/{{name}}-crate/Cargo.toml

# Prints one example's fact tables as text. `just dump tokenizer`
#
# The wire format is columns of numbers and says nothing to a reader. This is
# the same data one row per line, which is what to grep when a port looks wrong
# and the question is what the tables actually say.
dump name:
    @cargo run -q -p zag -- facts --example {{name}} --output target/{{name}}.facts
    @cargo run -q -p zag -- dump --facts target/{{name}}.facts

# Lists the examples that can be ported
@names:
    cargo run -q -p zag -- examples

# Shows the ownership report for the worked example
report: fixture
    @cat target/example.report.txt

# Times the passes over a synthetic program of the given size
#
# The default in `cargo test` is small enough to stay a normal test. This turns
# it up, which is the only way to tell a pass that scales from one that has
# never been asked to.
bench scale="80000":
    ZAG_SCALE={{scale}} cargo test -q -p zag --release --test scaling -- --nocapture

# Publishes every crate to crates.io, in dependency order
#
# A crate cannot be published before what it depends on, so the order here is
# the dependency order and changing it breaks the run partway through.
# `zag-verify` is not published: it exists only to compile the checked in ports
# during the build.
publish:
    cargo publish -p zag-facts
    cargo publish -p zag-render
    cargo publish -p zag-analysis
    cargo publish -p zag-frontend
    cargo publish -p zag-emit
    cargo publish -p zag-repair
    cargo publish -p zag

# Dry run of publishing every crate
publish-dry:
    cargo publish -p zag-facts --dry-run
    cargo publish -p zag-render --dry-run
    cargo publish -p zag-analysis --dry-run
    cargo publish -p zag-frontend --dry-run
    cargo publish -p zag-emit --dry-run
    cargo publish -p zag-repair --dry-run
    cargo publish -p zag --dry-run

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
