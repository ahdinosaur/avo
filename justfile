ludis-apply-example-simple:
  cargo run -p ludis-apply -- --plan ./examples/simple.ludis --params '{ "whatever": true }' --log trace

ludis-apply-example-multi:
  cargo run -p ludis-apply -- --plan ./examples/multi.ludis --log trace
