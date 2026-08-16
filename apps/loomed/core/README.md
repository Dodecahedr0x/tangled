# loomed-core

Loomed messenger domain (no Dioxus). Protocol via [`tangled`](../tangled/).

## Tests

Default suite uses an in-process `MockKnot`:

```bash
cargo test -p loomed-core
```

Live HTTP against a real `tangled-knot` subprocess (ignored; not run by default):

```bash
cargo build -p tangled-knot
cargo test -p loomed-core --test knot_live -- --ignored --nocapture
```

The live test binds `127.0.0.1:0`, uses a temp `--data` dir, and kills the knot on teardown. Set `TANGLED_KNOT` to override the binary path.
