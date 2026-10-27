#!/bin/sh
# Build one Dioxus app for the browser. TANGLED_KNOT, when set, is the default knot.
set -eu
app="$1"
title="$2"
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi
rustup target add wasm32-unknown-unknown

if ! command -v wasm-bindgen >/dev/null 2>&1; then
    bindgen_version=0.2.127
    case "$(uname -s)-$(uname -m)" in
        Linux-x86_64)
            archive="wasm-bindgen-${bindgen_version}-x86_64-unknown-linux-musl"
            url="https://github.com/rustwasm/wasm-bindgen/releases/download/${bindgen_version}/${archive}.tar.gz"
            tmp=$(mktemp -d)
            curl -fsSL "$url" | tar -xz -C "$tmp"
            mkdir -p "$HOME/.local/bin"
            cp "$tmp/$archive/wasm-bindgen" "$HOME/.local/bin/wasm-bindgen"
            export PATH="$HOME/.local/bin:$PATH"
            ;;
        *)
            cargo install wasm-bindgen-cli --version "$bindgen_version" --locked
            ;;
    esac
fi

cargo build -p "$app" --target wasm32-unknown-unknown --release --features web
target_dir="${CARGO_TARGET_DIR:-$root/target}"
out="$root/dist/$app"
rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --out-dir "$out" \
    "$target_dir/wasm32-unknown-unknown/release/${app}.wasm"

mkdir -p "$out/fonts"
for font in FiraSans-Regular.woff2 MPLUS1Code-Regular.woff2; do
    src="$root/apps/$app/assets/fonts/$font"
    if [ -f "$src" ]; then
        cp "$src" "$out/fonts/$font"
    fi
done

cat > "$out/index.html" <<EOF
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${title}</title>
</head>
<body>
  <div id="main"></div>
  <script type="module">
    import init from "./${app}.js";
    init();
  </script>
</body>
</html>
EOF
