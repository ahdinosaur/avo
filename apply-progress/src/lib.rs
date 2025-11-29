pub struct ResourceParamsTree;
pub struct ResourcesTree;
pub struct ResourceStatesTree;
pub struct ResourceChangesTree;
pub struct OperationsTree;
pub struct OperationsEpochs;
pub struct OperationApplyProgress;

// NOTE: instead of actual objects, could be "Render" type objects that are returned by the actual
// objects. so they basically have a Render trait, like Display, which returns an object we can
// render.
