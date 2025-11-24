lusid-local-apply:
  cargo run -p lusid -- local apply --config ./examples/lusid.toml --params '{ "whatever": true }' --log trace

lusid-apply-example-simple:
  cargo run -p lusid-apply -- --plan ./examples/simple.lusid --params '{ "whatever": true }' --log trace

lusid-apply-example-multi:
  cargo run -p lusid-apply -- --plan ./examples/multi.lusid --log trace
