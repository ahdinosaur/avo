use serde::{Deserialize, Serialize};

use crate::View;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewNode {
    NotStarted,
    Started,
    Complete(View),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewTree {
    Branch { view: View, children: Vec<ViewTree> },
    Leaf { view: View },
}
