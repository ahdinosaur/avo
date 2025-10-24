use avo::{
    create_store,
    operation::apply,
    plan::{PlanId, plan},
};
use avo_params::ParamValues;
use rimu::SourceId;
use serde::Serialize;
use std::env;

#[derive(Serialize)]
struct ExampleParams {
    pub whatever: bool,
}

#[tokio::main]
async fn main() {
    let mut store = create_store();

    let path = env::current_dir().expect("Failed to get env::current_dir()");
    let plan_id = PlanId::Path(path.join("examples/demo.avo"));

    let params = ParamValues::from_type(ExampleParams { whatever: true }, SourceId::empty())
        .expect("Failed to create params");

    let operation = plan(plan_id, params, &mut store)
        .await
        .expect("Failed to plan");

    apply(operation).await.expect("Failed to apply");
}
