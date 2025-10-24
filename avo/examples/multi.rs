use avo::{
    create_store,
    operation::apply,
    plan::{plan, PlanId},
};
use std::env;

#[tokio::main]
async fn main() {
    let mut store = create_store();

    let path = env::current_dir().expect("Failed to get env::current_dir()");
    let plan_id = PlanId::Path(path.join("examples/multi.avo"));

    let operation = plan(plan_id, None, &mut store)
        .await
        .expect("Failed to plan");

    apply(operation).await.expect("Failed to apply");
}
