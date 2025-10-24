use avo::{
    create_store,
    operation::apply,
    plan::{plan, PlanId},
};
use avo_params::ParamValues;
use indexmap::indexmap;
use rimu::{SourceId, Span, Spanned, Value};
use rimu_interop::FromRimu;
use std::env;

#[tokio::main]
async fn main() {
    let mut store = create_store();

    let path = env::current_dir().expect("Failed to get env::current_dir()");
    let plan_id = PlanId::Path(path.join("examples/demo.avo"));

    let value_span = Span::new(SourceId::empty(), 0, 0);
    let params = ParamValues::from_rimu_spanned(Spanned::new(
        Value::Object(indexmap! {
            "whatever".into() => Spanned::new(Value::Boolean(true), value_span.clone()),
        }),
        value_span,
    ))
    .expect("Failed to create params");

    let operation = plan(plan_id, params, &mut store)
        .await
        .expect("Failed to plan");

    apply(operation).await.expect("Failed to apply");
}
