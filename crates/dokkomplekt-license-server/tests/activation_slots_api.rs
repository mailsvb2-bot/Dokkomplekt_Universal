use dokkomplekt_license_core::{evaluate_machine_activation, PlanId};

#[test]
fn server_uses_core_activation_policy_for_doctor_pro_slots() {
    let first = evaluate_machine_activation(&PlanId::DoctorPro, 0, 1);
    assert!(first.allowed);

    let second = evaluate_machine_activation(&PlanId::DoctorPro, 1, 1);
    assert!(second.allowed);

    let third = evaluate_machine_activation(&PlanId::DoctorPro, 2, 1);
    assert!(!third.allowed);
    assert_eq!(third.code, "machine_slot_limit");
}
