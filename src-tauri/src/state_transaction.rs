use crate::{default_state_db_path, ensure_persistence_available, repository_for, AppState};
use dokkomplekt_core::{DocumentPack, SemanticCase};
use dokkomplekt_license_core::LicenseDocument;

#[derive(Clone)]
pub(crate) struct PersistedDesktopState {
    pub(crate) semantic_case: SemanticCase,
    pub(crate) pack: DocumentPack,
    pub(crate) license_document: Option<LicenseDocument>,
}

struct PreparedStateMutation<R> {
    result: R,
    next_state: Option<PersistedDesktopState>,
}

fn prepare_and_persist_state_mutation<R, F, P>(
    mut candidate: PersistedDesktopState,
    mutate: F,
    persist: P,
) -> Result<PreparedStateMutation<R>, String>
where
    F: FnOnce(&mut PersistedDesktopState) -> Result<(R, bool), String>,
    P: FnOnce(&PersistedDesktopState) -> Result<(), String>,
{
    let (result, changed) = mutate(&mut candidate)?;
    if !changed {
        return Ok(PreparedStateMutation {
            result,
            next_state: None,
        });
    }
    persist(&candidate)?;
    Ok(PreparedStateMutation {
        result,
        next_state: Some(candidate),
    })
}

pub(crate) fn transact_default_state<R, F>(
    app: &tauri::AppHandle,
    state: &AppState,
    mutate: F,
) -> Result<R, String>
where
    F: FnOnce(&mut PersistedDesktopState) -> Result<(R, bool), String>,
{
    ensure_persistence_available(state)?;
    // Serialize clone -> validate/mutate -> durable commit -> in-memory publish.
    // Application data mutexes are held only while cloning/swapping; SQLite I/O
    // never runs under semantic_case/pack/license_document mutexes.
    let _persistence_guard = state
        .persistence_gate
        .lock()
        .map_err(|_| "persistence gate lock failed")?;
    let current = PersistedDesktopState {
        semantic_case: state
            .semantic_case
            .lock()
            .map_err(|_| "state lock failed")?
            .clone(),
        pack: state.pack.lock().map_err(|_| "state lock failed")?.clone(),
        license_document: state
            .license_document
            .lock()
            .map_err(|_| "license state lock failed")?
            .clone(),
    };
    let path = default_state_db_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let prepared = prepare_and_persist_state_mutation(current, mutate, |candidate| {
        repository_for(&path)?
            .save_desktop_snapshot(
                "current",
                "default",
                &candidate.semantic_case,
                &candidate.pack,
                "license_document",
                &candidate.license_document,
            )
            .map_err(|error| error.to_string())
    })?;
    if let Some(next) = prepared.next_state {
        *state
            .semantic_case
            .lock()
            .map_err(|_| "state lock failed")? = next.semantic_case;
        *state.pack.lock().map_err(|_| "state lock failed")? = next.pack;
        *state
            .license_document
            .lock()
            .map_err(|_| "license state lock failed")? = next.license_document;
        *state.db_path.lock().map_err(|_| "state lock failed")? = Some(path);
    }
    Ok(prepared.result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state(name: &str) -> PersistedDesktopState {
        PersistedDesktopState {
            semantic_case: SemanticCase::default(),
            pack: DocumentPack {
                pack_id: "default".into(),
                name: name.into(),
                documents: Vec::new(),
            },
            license_document: None,
        }
    }

    #[test]
    fn persistence_failure_never_returns_a_candidate_for_publication() {
        let original = sample_state("before");
        let result = prepare_and_persist_state_mutation(
            original.clone(),
            |candidate| {
                candidate.pack.name = "after".into();
                Ok(((), true))
            },
            |_candidate| Err("injected persistence failure".into()),
        );
        assert!(result.is_err());
        assert_eq!(original.pack.name, "before");
    }

    #[test]
    fn only_durably_persisted_candidate_is_returned_for_publication() {
        let prepared = prepare_and_persist_state_mutation(
            sample_state("before"),
            |candidate| {
                candidate.pack.name = "after".into();
                Ok((candidate.pack.name.clone(), true))
            },
            |candidate| {
                assert_eq!(candidate.pack.name, "after");
                Ok(())
            },
        )
        .expect("candidate must persist");
        assert_eq!(prepared.result, "after");
        assert_eq!(
            prepared.next_state.expect("published candidate").pack.name,
            "after"
        );
    }

    #[test]
    fn unchanged_mutation_does_not_touch_persistence() {
        let prepared = prepare_and_persist_state_mutation(
            sample_state("before"),
            |candidate| Ok((candidate.pack.name.clone(), false)),
            |_candidate| panic!("unchanged mutation must not persist"),
        )
        .expect("unchanged candidate");
        assert_eq!(prepared.result, "before");
        assert!(prepared.next_state.is_none());
    }
}
