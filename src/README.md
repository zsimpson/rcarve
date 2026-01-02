
# Build release

You have to turn off all default features and then enable the cli_only
feature because Rust doesn't have a concept of "turning off a feature".

```sh
$ cargo build --release --no-default-features --features cli_only
```
