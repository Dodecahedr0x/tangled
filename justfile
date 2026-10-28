# Tangled development commands. Install just: brew install just

set positional-arguments := true

listen := "127.0.0.1:3000"
data_dir := ".data"

# List recipes
default:
    @just --list

# Start and seed all local services/apps; Enter or Ctrl-C stops the owned processes.
[group('Demo')]
demo *flags:
    @exec node scripts/local-demo.mjs "$@"

# Stop running demos in the selected data root and wait for process cleanup.
[group('Demo')]
demo-stop *flags:
    @exec node scripts/local-demo.mjs --stop "$@"

# Supervisor CLI, seeding and real process-group lifecycle tests.
[group('Checks')]
test-demo:
    node --test tests/local-demo*.test.mjs

# Workspace tests (does not run --ignored live knot or SNS RPC)
[group('Checks')]
test:
    cargo test --workspace

# Three-node HTTP app/protocol acceptance plus provider migration and harness checks
[group('Checks')]
test-e2e *flags:
    cargo build -p tangled-e2e -p tangled-knot
    cargo test -p tangled-e2e
    target/debug/tangled-e2e --output target/e2e/acceptance.json "$@"

# Release workflows and replication; fresh clusters, one warmup and three measured runs
[group('Checks')]
bench-e2e *flags:
    cargo build --release -p tangled-e2e -p tangled-knot
    target/release/tangled-e2e --benchmark --output target/e2e/benchmark.json "$@"

# Build tangled-knot, then run ignored loomed-core knot_live
[group('Checks')]
test-live:
    cargo build -p tangled-knot
    cargo test -p loomed-core --test knot_live -- --ignored --nocapture

# Local knot on http://127.0.0.1:3000 (data in .data/, gitignored)
[group('Run')]
knot:
    cargo run -p tangled-knot -- --listen "{{ listen }}" --data "{{ data_dir }}"

# Loomed Dioxus desktop app
[group('Run')]
loomed:
    cargo run -p loomed --features desktop

# Seed a running knot with inbound DMs (needs `just knot`; `just loomed-seed http://127.0.0.1:3000 --create-local` writes keys.json if missing)
[group('Seed')]
loomed-seed knot="http://127.0.0.1:3000" *flags:
    cargo run -p loomed-core --bin loomed-seed -- --knot "$@"

# Weft Dioxus desktop app (network explorer). Default knot http://127.0.0.1:3000.
[group('Run')]
weft:
    cargo run -p weft --features desktop

# Three local knots (:3000 home, :3001/:3002 peers) with public posts; `just weft` in another terminal; Ctrl-C stops.
[group('Seed')]
weft-seed:
    #!/bin/sh
    set -eu
    cargo build -p tangled-knot --bin tangled-knot
    cargo build -p weft-core --bin weft-seed
    knot="{{ justfile_directory() }}/target/debug/tangled-knot"
    seed="{{ justfile_directory() }}/target/debug/weft-seed"
    root="{{ justfile_directory() }}/{{ data_dir }}/weft-seed"
    mkdir -p "$root/a" "$root/b" "$root/c"
    pid_a= pid_b= pid_c=
    trap 'for pid in $pid_a $pid_b $pid_c; do kill "$pid" 2>/dev/null || true; done' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    "$knot" --listen 127.0.0.1:3000 --data "$root/a" --public-url http://127.0.0.1:3000 \
        --peer http://127.0.0.1:3001 --peer http://127.0.0.1:3002 &
    pid_a=$!
    "$knot" --listen 127.0.0.1:3001 --data "$root/b" --public-url http://127.0.0.1:3001 \
        --peer http://127.0.0.1:3000 --peer http://127.0.0.1:3002 &
    pid_b=$!
    "$knot" --listen 127.0.0.1:3002 --data "$root/c" --public-url http://127.0.0.1:3002 \
        --peer http://127.0.0.1:3000 --peer http://127.0.0.1:3001 &
    pid_c=$!
    for spec in "http://127.0.0.1:3000 $pid_a" "http://127.0.0.1:3001 $pid_b" "http://127.0.0.1:3002 $pid_c"; do
        url=${spec% *}
        pid=${spec##* }
        i=0
        while ! curl -sf "$url/xrpc/tangled.node.describe" >/dev/null; do
            if ! kill -0 "$pid" 2>/dev/null; then
                echo "knot failed to start at $url (is the port in use?)" >&2
                exit 1
            fi
            i=$((i + 1))
            if [ "$i" -ge 180 ]; then
                echo "knot did not become ready at $url (is the port in use?)" >&2
                exit 1
            fi
            sleep 1
        done
    done
    "$seed" --knot http://127.0.0.1:3000
    "$seed" --knot http://127.0.0.1:3001
    "$seed" --knot http://127.0.0.1:3002
    echo
    echo "Weft → http://127.0.0.1:3000  (just weft in another terminal)"
    echo "peers http://127.0.0.1:3001 http://127.0.0.1:3002"
    echo "Ctrl-C stops the cluster"
    wait

# Reed Dioxus desktop app
[group('Run')]
reed:
    cargo run -p reed --features desktop

# Seed a running knot with sample posts/follows (needs `just knot`; `just reed-seed http://127.0.0.1:3000 --create-local` writes keys.json if missing)
[group('Seed')]
reed-seed knot="http://127.0.0.1:3000" *flags:
    cargo run -p reed-core --bin reed-seed -- --knot "$@"

# Follow a real Mastodon acct and pull its public outbox (REED_AP_ACCT / --acct).
[group('Seed')]
reed-ap-seed knot="http://127.0.0.1:3000" *flags:
    cargo run -p reed-core --bin reed-ap-seed -- --knot "$@"

# Hosted knot: email account, custodial key, public posts (http://127.0.0.1:8787)
[group('Run')]
provider:
    cargo run -p tangled-provider

# Site preview at http://localhost:4321 (home → docs → spec)
[group('Run')]
docs:
    npm --prefix site run dev

# rustfmt
[group('Checks')]
fmt:
    cargo fmt --all

# clippy (deny warnings, same as CI)
[group('Checks')]
clippy:
    cargo clippy --all-targets -- -D warnings
