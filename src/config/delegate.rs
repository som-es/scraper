use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DelegateEndpoint {
    pub delegate_url: String,
    pub delegate_marker: String,
    pub deleagate_url_idx: usize,
}
