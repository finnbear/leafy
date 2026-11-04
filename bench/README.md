# Benchmarks

The same apps built against yew 0.23 and against leafy. The results are the
charts in the [top-level README](../README.md).

## Dependencies

[trunk](https://trunk-rs.github.io/trunk/), `wasm-tools`, `brotli`, `bc`, `taskset`,
Node 24 and Chromium (`$CHROME`, by default `chromium`). Rustup installs the
nightly toolchain in `rust-toolchain.toml`. `make tools` checks for them.

## Running

```sh
make all          # everything (under an hour)
make apps         # size and runtime speed of the apps: frontier.svg
make buildtime    # build time against size, 1 to 100 components: buildtime.svg
make ssr          # server-rendering time: ssr.svg
make charts       # redraw the charts from the last results (out/)
```
