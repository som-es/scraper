#[test]
fn later_phase_without_priority_date_preserves_previous_activity_date() {
    let content = serde_json::json!({"phase": [
        {"name": "Plenarberatungen NR", "stages": [{"date": "07.12.2011", "priority": 1}]},
        {"name": "Plenarberatungen BR", "stages": [{"date": "09.12.2011", "priority": 1}]},
        {"name": "Plenarberatungen NR", "stages": [{"date": "10.12.2011"}]}
    ]});
    assert_eq!(
        crate::legis_init::extract_plenary_date(&content, "NR"),
        Some("07.12.2011")
    );
    assert_eq!(
        crate::legis_init::extract_plenary_date(&content, "BR"),
        Some("09.12.2011")
    );
}

#[test]
fn later_priority_date_replaces_previous_activity_date() {
    let content = serde_json::json!({"phase": [
        {"name": "Plenarberatungen NR", "stages": [{"date": "07.12.2011", "priority": 1}]},
        {"name": "Plenarberatungen NR", "stages": [{"date": "10.12.2011", "priority": 1}]}
    ]});
    assert_eq!(
        crate::legis_init::extract_plenary_date(&content, "NR"),
        Some("10.12.2011")
    );
}
