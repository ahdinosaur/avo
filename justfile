build-lusid-apply:
  cargo build -p lusid-apply --target x86_64-unknown-linux-gnu --release
  # cargo build -p lusid-apply --target aarch64-unknown-linux-gnu --release

lusid-local-apply:
  cargo run -p lusid -- local apply --config ./examples/lusid.toml --params '{ "whatever": true }' --log trace

lusid-dev-apply: build-lusid-apply
  cargo run -p lusid --release -- dev apply --config ./examples/lusid.toml --machine a

lusid-dev-ssh:
  cargo run -p lusid --release -- dev ssh --config ./examples/lusid.toml --machine a

lusid-apply-example-simple:
  cargo run -p lusid-apply -- --plan ./examples/simple.lusid --params '{ "whatever": true }' --log trace

lusid-apply-example-multi:
  cargo run -p lusid-apply -- --plan ./examples/multi.lusid --log trace
