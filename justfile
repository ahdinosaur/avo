lusid-apply-example-simple:
  cargo run -p lusid-apply -- --plan ./examples/simple.lusid --params '{ "whatever": true }' --log trace

lusid-apply-example-multi:
  cargo run -p lusid-apply -- --plan ./examples/multi.lusid --log trace
