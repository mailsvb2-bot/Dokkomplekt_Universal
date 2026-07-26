use dokkomplekt_license_core::{evaluate_machine_activation, max_machines_for_plan, PlanId};

#[test]
fn plan_machine_limits_are_fixed_contract() {
    assert_eq!(max_machines_for_plan(&PlanId::DoctorStart), 1);
    assert_eq!(max_machines_for_plan(&PlanId::DoctorPro), 2);
    assert_eq!(max_machines_for_plan(&PlanId::Department), 5);
    assert_eq!(max_machines_for_plan(&PlanId::Clinic), 20);
}

#[test]
fn doctor_pro_allows_second_machine_but_rejects_third() {
    let second = evaluate_machine_activation(&PlanId::DoctorPro, 1, 1);
    assert!(second.allowed);
    assert_eq!(second.code, "slot_available");
    assert_eq!(second.remaining_slots, 0);

    let third = evaluate_machine_activation(&PlanId::DoctorPro, 2, 1);
    assert!(!third.allowed);
    assert_eq!(third.code, "machine_slot_limit");
    assert_eq!(third.max_machines, 2);
}

#[test]
fn department_allows_five_total_machines_only() {
    let fifth = evaluate_machine_activation(&PlanId::Department, 4, 1);
    assert!(fifth.allowed);

    let sixth = evaluate_machine_activation(&PlanId::Department, 5, 1);
    assert!(!sixth.allowed);
    assert_eq!(sixth.max_machines, 5);
}
