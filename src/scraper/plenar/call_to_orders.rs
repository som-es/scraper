use serde_json::Value;

use crate::{
    call_to_order::{
        extract_call_to_order_id_from_path, extract_call_to_order_paths_from_stages,
        CallToOrderInfo,
    },
    person_url_to_id,
};

pub(super) fn extract_call_to_order_info_from_stages(
    ityp: &str,
    inr: u64,
    stages: &Value,
) -> Vec<CallToOrderInfo> {
    let mut call_to_orders = vec![];
    for (person_path, person_name, call_to_order_path) in
        extract_call_to_order_paths_from_stages(stages).unwrap_or_default()
    {
        let receiver_id = person_url_to_id(&person_path).unwrap();
        let (call_to_order_inr, call_to_order_ityp) =
            extract_call_to_order_id_from_path(&call_to_order_path);

        call_to_orders.push(CallToOrderInfo {
            description: "".into(),
            ityp: call_to_order_ityp.unwrap_or("GO").into(),
            inr: call_to_order_inr.unwrap().parse().unwrap(),
            receiver: person_name,
            receiver_id,
            links_to: Some(format!("{ityp}/{inr}")),
        });
    }
    call_to_orders
}
