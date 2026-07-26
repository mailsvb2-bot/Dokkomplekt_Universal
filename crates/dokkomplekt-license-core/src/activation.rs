use crate::models::PlanId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationDecision {
    pub allowed: bool,
    pub code: &'static str,
    pub max_machines: u32,
    pub remaining_slots: u32,
}

pub fn max_machines_for_plan(plan: &PlanId) -> u32 {
    match plan {
        PlanId::Trial | PlanId::DoctorStart => 1,
        PlanId::DoctorPro => 2,
        PlanId::Department => 5,
        PlanId::Clinic => 20,
        PlanId::Enterprise => 9999,
        PlanId::Vip => 1,
    }
}

pub fn evaluate_machine_activation(
    plan: &PlanId,
    active_machines: u32,
    requested_machines: u32,
) -> ActivationDecision {
    let max_machines = max_machines_for_plan(plan);
    let projected = active_machines.saturating_add(requested_machines);
    if projected <= max_machines {
        ActivationDecision {
            allowed: true,
            code: "slot_available",
            max_machines,
            remaining_slots: max_machines.saturating_sub(projected),
        }
    } else {
        ActivationDecision {
            allowed: false,
            code: "machine_slot_limit",
            max_machines,
            remaining_slots: 0,
        }
    }
}
