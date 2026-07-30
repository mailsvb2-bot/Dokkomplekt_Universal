use crate::state::{ActivationRecord, MemoryStore, OrderRecord, OrderStatus};
use crate::storage::{
    order_status_after_payment, validate_access_recovery_input, validate_order_status_transition,
    ActivationIssueOutcome, AuditEventRecord, LicenseRecord, LicenseStore, PaymentEventRecord,
    PaymentEventStatus, PaymentEventWriteOutcome, PaymentProvider, StoreError,
};
use std::mem::discriminant;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

impl LicenseStore for Arc<RwLock<MemoryStore>> {
    fn create_order(&self, record: OrderRecord) -> Result<(), StoreError> {
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        if store.orders.contains_key(&record.id) {
            return Err(StoreError::Conflict);
        }
        store.orders.insert(record.id, record);
        Ok(())
    }

    fn get_order(&self, order_id: Uuid) -> Result<Option<OrderRecord>, StoreError> {
        let store = self.read().map_err(|_| StoreError::Poisoned)?;
        Ok(store.orders.get(&order_id).cloned())
    }

    fn recover_legacy_order_access(
        &self,
        order_id: Uuid,
        machine_hash: &str,
        access_token_hash: &str,
        bind_missing_machine: bool,
    ) -> Result<OrderRecord, StoreError> {
        let machine_hash = validate_access_recovery_input(machine_hash, access_token_hash)?;
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        let (recovered, bound_missing_machine) = {
            let order = store
                .orders
                .get_mut(&order_id)
                .ok_or(StoreError::NotFound)?;
            if order.access_token_hash.is_some() || matches!(order.status, OrderStatus::Cancelled) {
                return Err(StoreError::Conflict);
            }
            let bound_missing_machine = match order.machine_hash.as_deref().map(str::trim) {
                Some(expected) if expected == machine_hash => false,
                Some(_) => return Err(StoreError::Conflict),
                None if bind_missing_machine => {
                    order.machine_hash = Some(machine_hash.clone());
                    true
                }
                None => {
                    return Err(StoreError::Invalid(
                        "legacy_order_has_no_machine_binding".to_string(),
                    ))
                }
            };
            order.access_token_hash = Some(access_token_hash.trim().to_string());
            (order.clone(), bound_missing_machine)
        };
        let audit = AuditEventRecord {
            id: Uuid::new_v4(),
            entity_id: order_id,
            event_type: "order_access_recovered".to_string(),
            happened_at: time::OffsetDateTime::now_utc(),
            details_json: serde_json::json!({
                "bound_missing_machine": bound_missing_machine,
            })
            .to_string(),
        };
        store.audit_events.insert(audit.id, audit);
        Ok(recovered)
    }

    fn update_order_status(&self, order_id: Uuid, status: OrderStatus) -> Result<(), StoreError> {
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        let order = store
            .orders
            .get_mut(&order_id)
            .ok_or(StoreError::NotFound)?;
        validate_order_status_transition(&order.status, &status)?;
        order.status = status;
        Ok(())
    }

    fn create_activation(&self, record: ActivationRecord) -> Result<(), StoreError> {
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        if store.activations.contains_key(&record.id) {
            return Err(StoreError::Conflict);
        }
        store.activations.insert(record.id, record);
        Ok(())
    }

    fn create_activation_for_order(
        &self,
        record: ActivationRecord,
        max_machines: u32,
    ) -> Result<ActivationIssueOutcome, StoreError> {
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        let order = store
            .orders
            .get(&record.order_id)
            .ok_or(StoreError::NotFound)?
            .clone();
        if !matches!(order.status, OrderStatus::Paid | OrderStatus::LicenseIssued) {
            return Err(StoreError::Conflict);
        }
        if let Some(existing) = store
            .activations
            .values()
            .find(|activation| {
                activation.order_id == record.order_id
                    && activation.machine_hash == record.machine_hash
            })
            .cloned()
        {
            return Ok(ActivationIssueOutcome {
                order,
                activation: existing,
                reused: true,
            });
        }
        if store.activations.contains_key(&record.id) {
            return Err(StoreError::Conflict);
        }
        let active_count = store
            .activations
            .values()
            .filter(|activation| activation.order_id == record.order_id)
            .count() as u32;
        if active_count == 0
            && order
                .machine_hash
                .as_deref()
                .is_some_and(|expected| expected != record.machine_hash)
        {
            return Err(StoreError::Conflict);
        }
        if active_count >= max_machines {
            return Err(StoreError::Conflict);
        }
        store.activations.insert(record.id, record.clone());
        Ok(ActivationIssueOutcome {
            order,
            activation: record,
            reused: false,
        })
    }

    fn activations_for_order(&self, order_id: Uuid) -> Result<Vec<ActivationRecord>, StoreError> {
        let store = self.read().map_err(|_| StoreError::Poisoned)?;
        if !store.orders.contains_key(&order_id) {
            return Err(StoreError::NotFound);
        }
        let mut activations = store
            .activations
            .values()
            .filter(|activation| activation.order_id == order_id)
            .cloned()
            .collect::<Vec<_>>();
        activations.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.machine_hash.cmp(&right.machine_hash))
        });
        Ok(activations)
    }

    fn record_payment_event(&self, record: PaymentEventRecord) -> Result<(), StoreError> {
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        if store.payment_events.values().any(|existing| {
            same_provider(&existing.provider, &record.provider)
                && existing.provider_event_id == record.provider_event_id
        }) {
            return Err(StoreError::Conflict);
        }
        store.payment_events.insert(record.id, record);
        Ok(())
    }

    fn record_payment_event_for_order(
        &self,
        record: PaymentEventRecord,
    ) -> Result<PaymentEventWriteOutcome, StoreError> {
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        if store.payment_events.values().any(|existing| {
            same_provider(&existing.provider, &record.provider)
                && existing.provider_event_id == record.provider_event_id
        }) {
            return Ok(PaymentEventWriteOutcome::Duplicate);
        }
        let order = store
            .orders
            .get_mut(&record.order_id)
            .ok_or(StoreError::NotFound)?;
        if order.amount_rub != record.amount_rub {
            return Err(StoreError::Invalid("amount_mismatch".to_string()));
        }
        order.status = order_status_after_payment(&order.status, &record.status)?;
        store.payment_events.insert(record.id, record);
        Ok(PaymentEventWriteOutcome::Recorded)
    }

    fn store_license(&self, record: LicenseRecord) -> Result<(), StoreError> {
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        if store.licenses.contains_key(&record.id)
            || store
                .licenses
                .values()
                .any(|existing| existing.license_id == record.license_id)
        {
            return Err(StoreError::Conflict);
        }
        store.licenses.insert(record.id, record);
        Ok(())
    }

    fn audit(&self, record: AuditEventRecord) -> Result<(), StoreError> {
        let mut store = self.write().map_err(|_| StoreError::Poisoned)?;
        if store.audit_events.contains_key(&record.id) {
            return Err(StoreError::Conflict);
        }
        store.audit_events.insert(record.id, record);
        Ok(())
    }
}

fn same_provider(left: &PaymentProvider, right: &PaymentProvider) -> bool {
    discriminant(left) == discriminant(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn memory_store() -> Arc<RwLock<MemoryStore>> {
        Arc::new(RwLock::new(MemoryStore::default()))
    }

    fn order_record(id: Uuid, status: OrderStatus) -> OrderRecord {
        OrderRecord {
            id,
            plan: "doctor_pro".to_string(),
            amount_rub: 3900,
            status,
            machine_hash: None,
            access_token_hash: Some("a".repeat(64)),
            created_at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn legacy_access_recovery_is_atomic_machine_bound_and_one_time() {
        let store = memory_store();
        let order_id = Uuid::new_v4();
        let mut order = order_record(order_id, OrderStatus::Paid);
        order.machine_hash = Some("machine-owner".to_string());
        order.access_token_hash = None;
        store.create_order(order).unwrap();

        assert_eq!(
            store
                .recover_legacy_order_access(order_id, "machine-attacker", &"b".repeat(64), false,)
                .unwrap_err(),
            StoreError::Conflict
        );
        let recovered_hash = "c".repeat(64);
        let recovered = store
            .recover_legacy_order_access(order_id, "machine-owner", &recovered_hash, false)
            .unwrap();
        assert_eq!(
            recovered.access_token_hash.as_deref(),
            Some(recovered_hash.as_str())
        );
        assert_eq!(
            store
                .recover_legacy_order_access(order_id, "machine-owner", &"d".repeat(64), false)
                .unwrap_err(),
            StoreError::Conflict
        );
    }

    #[test]
    fn legacy_access_recovery_can_deliberately_bind_an_unbound_order() {
        let store = memory_store();
        let order_id = Uuid::new_v4();
        let mut order = order_record(order_id, OrderStatus::Paid);
        order.machine_hash = None;
        order.access_token_hash = None;
        store.create_order(order).unwrap();
        assert!(matches!(
            store.recover_legacy_order_access(order_id, "machine-owner", &"e".repeat(64), false),
            Err(StoreError::Invalid(_))
        ));
        let recovered = store
            .recover_legacy_order_access(order_id, "machine-owner", &"e".repeat(64), true)
            .unwrap();
        assert_eq!(recovered.machine_hash.as_deref(), Some("machine-owner"));
    }

    #[test]
    fn concurrent_legacy_recovery_produces_exactly_one_winner() {
        let store = memory_store();
        let order_id = Uuid::new_v4();
        let mut order = order_record(order_id, OrderStatus::Paid);
        order.machine_hash = Some("machine-owner".to_string());
        order.access_token_hash = None;
        store.create_order(order).unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(16));
        let winners = (0..16)
            .map(|index| {
                let store = store.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store
                        .recover_legacy_order_access(
                            order_id,
                            "machine-owner",
                            &format!("{index:064x}"),
                            false,
                        )
                        .is_ok()
                })
            })
            .map(|thread| thread.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
    }

    #[test]
    fn provider_callback_store_method_marks_order_paid_and_deduplicates() {
        let store = memory_store();
        let order_id = Uuid::new_v4();
        store
            .create_order(order_record(order_id, OrderStatus::WaitingPayment))
            .unwrap();

        let record = PaymentEventRecord {
            id: Uuid::new_v4(),
            order_id,
            provider: PaymentProvider::Manual,
            provider_event_id: "evt-1".to_string(),
            provider_payment_id: Some("pay-1".to_string()),
            status: PaymentEventStatus::Succeeded,
            amount_rub: 3900,
            received_at: OffsetDateTime::now_utc(),
        };

        assert_eq!(
            store
                .record_payment_event_for_order(record.clone())
                .unwrap(),
            PaymentEventWriteOutcome::Recorded,
        );
        assert!(matches!(
            store.get_order(order_id).unwrap().unwrap().status,
            OrderStatus::Paid
        ));

        let duplicate = PaymentEventRecord {
            id: Uuid::new_v4(),
            ..record
        };
        assert_eq!(
            store.record_payment_event_for_order(duplicate).unwrap(),
            PaymentEventWriteOutcome::Duplicate,
        );
    }

    #[test]
    fn activation_store_method_checks_paid_order_and_slot_capacity_under_one_write_lock() {
        let store = memory_store();
        let order_id = Uuid::new_v4();
        store
            .create_order(order_record(order_id, OrderStatus::Paid))
            .unwrap();

        let first = ActivationRecord {
            id: Uuid::new_v4(),
            order_id,
            machine_hash: "machine-a".to_string(),
            created_at: OffsetDateTime::now_utc(),
        };
        let first_outcome = store.create_activation_for_order(first.clone(), 1).unwrap();
        assert!(!first_outcome.reused);
        assert_eq!(first_outcome.activation.id, first.id);
        let repeated = store
            .create_activation_for_order(
                ActivationRecord {
                    id: Uuid::new_v4(),
                    ..first.clone()
                },
                1,
            )
            .unwrap();
        assert!(repeated.reused);
        assert_eq!(repeated.activation.id, first.id);
        assert_eq!(store.activations_for_order(order_id).unwrap().len(), 1);

        let second = ActivationRecord {
            id: Uuid::new_v4(),
            order_id,
            machine_hash: "machine-b".to_string(),
            created_at: OffsetDateTime::now_utc(),
        };
        assert_eq!(
            store.create_activation_for_order(second, 1).unwrap_err(),
            StoreError::Conflict
        );
    }

    #[test]
    fn first_activation_is_bound_to_machine_recorded_at_checkout() {
        let store = memory_store();
        let order_id = Uuid::new_v4();
        let mut order = order_record(order_id, OrderStatus::Paid);
        order.machine_hash = Some("machine-owner".to_string());
        store.create_order(order).unwrap();
        let attacker = ActivationRecord {
            id: Uuid::new_v4(),
            order_id,
            machine_hash: "machine-attacker".to_string(),
            created_at: OffsetDateTime::now_utc(),
        };
        assert_eq!(
            store.create_activation_for_order(attacker, 3,).unwrap_err(),
            StoreError::Conflict
        );
        assert!(store.activations_for_order(order_id).unwrap().is_empty());
    }

    #[test]
    fn activation_capacity_is_serialized_under_concurrent_contention() {
        use std::sync::Barrier;
        use std::thread;

        let store = memory_store();
        let order_id = Uuid::new_v4();
        store
            .create_order(order_record(order_id, OrderStatus::Paid))
            .unwrap();
        let workers = 16;
        let barrier = Arc::new(Barrier::new(workers));
        let mut handles = Vec::new();
        for index in 0..workers {
            let store = store.clone();
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                barrier.wait();
                store.create_activation_for_order(
                    ActivationRecord {
                        id: Uuid::new_v4(),
                        order_id,
                        machine_hash: format!("machine-{index}"),
                        created_at: OffsetDateTime::now_utc(),
                    },
                    1,
                )
            }));
        }
        let successes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(Result::is_ok)
            .count();
        assert_eq!(successes, 1);
        assert_eq!(store.activations_for_order(order_id).unwrap().len(), 1);
    }

    #[test]
    fn late_success_event_never_downgrades_license_issued_order() {
        let store = memory_store();
        let order_id = Uuid::new_v4();
        store
            .create_order(order_record(order_id, OrderStatus::LicenseIssued))
            .unwrap();
        let record = PaymentEventRecord {
            id: Uuid::new_v4(),
            order_id,
            provider: PaymentProvider::Manual,
            provider_event_id: "late-success".to_string(),
            provider_payment_id: None,
            status: PaymentEventStatus::Succeeded,
            amount_rub: 3900,
            received_at: OffsetDateTime::now_utc(),
        };
        store.record_payment_event_for_order(record).unwrap();
        assert_eq!(
            store.get_order(order_id).unwrap().unwrap().status,
            OrderStatus::LicenseIssued
        );
    }
}
