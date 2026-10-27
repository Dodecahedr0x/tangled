# Tangled workspace recipes.
#
# Install just: `brew install just` or `cargo install just`
#
# `just test-live` needs a `tangled-knot` binary. The recipe runs
# `cargo build -p tangled-knot` first (override with TANGLED_KNOT).
#
# Recipes use `cargo -p <name>`. Loomed is apps/loomed (UI) and
# apps/loomed/core (loomed-core). Reed is apps/reed (UI) and
# apps/reed/core (reed-core). Weft is apps/weft (UI) and
# apps/weft/core (weft-core). Protocol crates stay under crates/.

listen := "127.0.0.1:3000"
data_dir := ".data"

# List recipes
default:
    @just --list

# Workspace tests (does not run --ignored live knot or SNS RPC)
test:
    cargo test --workspace

# Three-node HTTP app/protocol acceptance plus provider migration and harness checks
test-e2e *flags:
    cargo build -p tangled-e2e -p tangled-knot
    cargo test -p tangled-e2e
    target/debug/tangled-e2e --output target/e2e/acceptance.json {{flags}}

# Release workflows and replication; fresh clusters, one warmup and three measured runs
bench-e2e *flags:
    cargo build --release -p tangled-e2e -p tangled-knot
    target/release/tangled-e2e --benchmark --output target/e2e/benchmark.json {{flags}}

# Build tangled-knot, then run ignored loomed-core knot_live
test-live:
    cargo build -p tangled-knot
    cargo test -p loomed-core --test knot_live -- --ignored --nocapture

# Local knot on http://127.0.0.1:3000 (data in .data/, gitignored)
knot:
    cargo run -p tangled-knot -- --listen {{listen}} --data {{data_dir}}

# Loomed Dioxus desktop app
loomed:
    cargo run -p loomed --features desktop

# Seed a running knot with inbound DMs (needs `just knot`; `just loomed-seed http://127.0.0.1:3000 --create-local` writes keys.json if missing)
loomed-seed knot="http://127.0.0.1:3000" *flags:
    cargo run -p loomed-core --bin loomed-seed -- --knot {{knot}} {{flags}}

# One-shot local demo (`just loomed-demo`): background knot, loomed-seed --create-local (no key clobber), Loomed desktop; knot dies on exit.
loomed-demo knot="http://127.0.0.1:3000":
    #!/bin/sh
    set -eu
    "{{just_executable()}}" knot &
    pid=$!
    trap 'kill "$pid" 2>/dev/null || true' EXIT
    i=0
    while ! curl -sf "{{knot}}/xrpc/tangled.node.describe" >/dev/null; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "knot failed to start at {{knot}} (is the port in use?)" >&2
            exit 1
        fi
        i=$((i + 1))
        if [ "$i" -ge 180 ]; then
            echo "knot did not become ready at {{knot}} (is the port in use?)" >&2
            exit 1
        fi
        sleep 1
    done
    "{{just_executable()}}" loomed-seed "{{knot}}" --create-local
    "{{just_executable()}}" loomed

# Weft Dioxus desktop app (network explorer). Default knot http://127.0.0.1:3000.
weft:
    cargo run -p weft --features desktop

# Writes .data/weft-seed/pids so `just weft-demo` can kill the cluster.
# Three local knots (:3000 home, :3001/:3002 peers) with public posts; `just weft` in another terminal; Ctrl-C stops.
weft-seed:
    #!/bin/sh
    set -eu
    cargo build -p tangled-knot -p weft-core --bin weft-seed
    knot="{{justfile_directory()}}/target/debug/tangled-knot"
    seed="{{justfile_directory()}}/target/debug/weft-seed"
    root="{{justfile_directory()}}/{{data_dir}}/weft-seed"
    mkdir -p "$root/a" "$root/b" "$root/c"
    "$knot" --listen 127.0.0.1:3000 --data "$root/a" --public-url http://127.0.0.1:3000 \
        --peer http://127.0.0.1:3001 --peer http://127.0.0.1:3002 &
    pid_a=$!
    "$knot" --listen 127.0.0.1:3001 --data "$root/b" --public-url http://127.0.0.1:3001 \
        --peer http://127.0.0.1:3000 --peer http://127.0.0.1:3002 &
    pid_b=$!
    "$knot" --listen 127.0.0.1:3002 --data "$root/c" --public-url http://127.0.0.1:3002 \
        --peer http://127.0.0.1:3000 --peer http://127.0.0.1:3001 &
    pid_c=$!
    echo "$pid_a $pid_b $pid_c" > "$root/pids"
    trap 'kill "$pid_a" "$pid_b" "$pid_c" 2>/dev/null || true' EXIT
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

# Temp WEFT_HOME so this run hits :3000 without rewriting ~/.weft/knot.txt.
# One-shot local demo (`just weft-demo`): weft-seed cluster, Weft desktop on :3000; cluster dies on exit.
weft-demo:
    #!/bin/sh
    set -eu
    if curl -s --connect-timeout 1 -o /dev/null http://127.0.0.1:3000/; then
        echo "http://127.0.0.1:3000 already in use (stop leftover just knot / weft-seed)" >&2
        exit 1
    fi
    pids_file="{{justfile_directory()}}/{{data_dir}}/weft-seed/pids"
    "{{just_executable()}}" weft-seed &
    pid=$!
    trap 'kill "$pid" 2>/dev/null || true
          if [ -f "$pids_file" ]; then kill $(cat "$pids_file") 2>/dev/null || true; fi' EXIT
    i=0
    while ! curl -sf http://127.0.0.1:3000/xrpc/tangled.node.describe >/dev/null; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "weft-seed failed to start (is port 3000 in use?)" >&2
            exit 1
        fi
        i=$((i + 1))
        if [ "$i" -ge 180 ]; then
            echo "weft-seed did not become ready at http://127.0.0.1:3000 (is the port in use?)" >&2
            exit 1
        fi
        sleep 1
    done
    home="${TMPDIR:-/tmp}/weft-demo"
    mkdir -p "$home"
    printf '%s\n' "http://127.0.0.1:3000" > "$home/knot.txt"
    WEFT_HOME="$home" "{{just_executable()}}" weft

# Reed Dioxus desktop app
reed:
    cargo run -p reed --features desktop

# Seed a running knot with sample posts/follows (needs `just knot`; `just reed-seed http://127.0.0.1:3000 --create-local` writes keys.json if missing)
reed-seed knot="http://127.0.0.1:3000" *flags:
    cargo run -p reed-core --bin reed-seed -- --knot {{knot}} {{flags}}

# Follow a real Mastodon acct and pull its public outbox (REED_AP_ACCT / --acct).
reed-ap-seed knot="http://127.0.0.1:3000" *flags:
    cargo run -p reed-core --bin reed-ap-seed -- --knot {{knot}} {{flags}}

# Tangled-only seed stays on `just reed-demo`.
# Default acct:Gargron@mastodon.social (override REED_AP_ACCT). No mock peer.
reed-ap-demo knot="http://127.0.0.1:3000":
    #!/bin/sh
    set -eu
    "{{just_executable()}}" knot &
    pid=$!
    trap 'kill "$pid" 2>/dev/null || true' EXIT
    i=0
    while ! curl -sf "{{knot}}/xrpc/tangled.node.describe" >/dev/null; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "knot failed to start at {{knot}} (is the port in use?)" >&2
            exit 1
        fi
        i=$((i + 1))
        if [ "$i" -ge 180 ]; then
            echo "knot did not become ready at {{knot}} (is the port in use?)" >&2
            exit 1
        fi
        sleep 1
    done
    "{{just_executable()}}" reed-seed "{{knot}}" --create-local
    "{{just_executable()}}" reed-ap-seed "{{knot}}" --create-local
    "{{just_executable()}}" reed

# Old loopback mock Mastodon peer (no public internet).
reed-ap-demo-mock knot="http://127.0.0.1:3000":
    #!/bin/sh
    set -eu
    "{{just_executable()}}" knot &
    pid=$!
    trap 'kill "$pid" 2>/dev/null || true' EXIT
    i=0
    while ! curl -sf "{{knot}}/xrpc/tangled.node.describe" >/dev/null; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "knot failed to start at {{knot}} (is the port in use?)" >&2
            exit 1
        fi
        i=$((i + 1))
        if [ "$i" -ge 180 ]; then
            echo "knot did not become ready at {{knot}} (is the port in use?)" >&2
            exit 1
        fi
        sleep 1
    done
    "{{just_executable()}}" reed-ap-seed "{{knot}}" --create-local --mock
    "{{just_executable()}}" reed

# One-shot local demo (`just reed-demo`): background knot, reed-seed --create-local (no key clobber), Reed desktop; knot dies on exit.
reed-demo knot="http://127.0.0.1:3000":
    #!/bin/sh
    set -eu
    "{{just_executable()}}" knot &
    pid=$!
    trap 'kill "$pid" 2>/dev/null || true' EXIT
    i=0
    while ! curl -sf "{{knot}}/xrpc/tangled.node.describe" >/dev/null; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "knot failed to start at {{knot}} (is the port in use?)" >&2
            exit 1
        fi
        i=$((i + 1))
        if [ "$i" -ge 180 ]; then
            echo "knot did not become ready at {{knot}} (is the port in use?)" >&2
            exit 1
        fi
        sleep 1
    done
    "{{just_executable()}}" reed-seed "{{knot}}" --create-local
    "{{just_executable()}}" reed

# Hosted knot: email account, custodial key, public posts (http://127.0.0.1:8787)
provider:
    cargo run -p tangled-provider

# Site preview at http://localhost:4321 (home → docs → spec)
docs:
    npm --prefix site run dev

# rustfmt
fmt:
    cargo fmt --all

# clippy (deny warnings, same as CI)
clippy:
    cargo clippy --all-targets -- -D warnings
