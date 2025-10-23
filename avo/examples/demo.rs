use avo::{apply, parser::PlanId, plan};
use avo_params::ParamValues;
use indexmap::IndexMap;
use rimu::{SourceId, Span, Spanned, Value};
use rimu_interop::FromRimu;
use std::env;

#[tokio::main]
async fn main() {
    let path = env::current_dir().expect("Failed to get env::current_dir()");
    let plan_id = PlanId::Path(path.join("examples/demo.avo"));
    let params = ParamValues::from_rimu_spanned(Spanned::new(
        Value::Object(IndexMap::new()),
        Span::new(SourceId::empty(), 0, 0),
    ))
    .expect("Failed to create params");

    let operation = plan(plan_id, params).await.expect("Failed to plan");

    apply(operation).await.expect("Failed to apply");
}
