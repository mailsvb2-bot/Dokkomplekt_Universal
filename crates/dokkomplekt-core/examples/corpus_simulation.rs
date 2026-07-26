//! Воспроизводимый генератор синтетического корпуса для калибровки порогов.
//!
//! Назначение: доказать, что цепочка «корпус -> измерение -> калибровка ->
//! подписанный порог -> гейт генерации» работает end-to-end, и дать данные
//! для регрессионной проверки калибратора.
//!
//! Ключевое свойство: записи строятся ЧЕРЕЗ настоящий `build_corpus_entry`,
//! поэтому SHA-256 отпечатки полей идентичны продовым. Никакого
//! параллельного порта отпечатков — это исключает дрейф между симуляцией
//! и рантаймом.
//!
//! Это МОДЕЛЬ, а не полевые данные. Вероятности верного извлечения заданы
//! правдоподобно (реквизиты с контрольной суммой — надёжнее, свободный
//! текст — хуже), а уверенность коррелирует с корректностью — без этой
//! связи калибровка была бы бессмысленной. Реальная точность на настоящих
//! сканах может отличаться, и тогда часть доменов уйдёт в fail-closed.
//!
//! Запуск:
//!   cargo run -p dokkomplekt-core --example corpus_simulation -- 1000 > corpus.json
//! затем:
//!   python3 scripts/calibrate_thresholds.py corpus.json --domain hr \
//!       --output hr.json --target-auto-error-rate 0.005

use dokkomplekt_core::{
    build_corpus_entry, CorpusEntryRequest, DomainKind, SemanticCase, SemanticValue, ValueSource,
};

/// Детерминированный SplitMix64 — воспроизводимость без внешних зависимостей.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Усечённый гаусс через Бокс–Мюллер.
    fn noise(&mut self, scale: f64) -> f64 {
        let u1 = self.unit().max(1e-12);
        let u2 = self.unit();
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        (z * scale).clamp(-1.0, 1.0)
    }
}

struct FieldModel {
    field_id: &'static str,
    /// Как часто поле вообще извлекается.
    extract_rate: f64,
    /// Вероятность верного значения, когда поле извлечено.
    p_true: f64,
}

struct Profession {
    name: &'static str,
    domain: DomainKind,
    pack_id: &'static str,
    kit: &'static [&'static str],
    fields: &'static [FieldModel],
}

macro_rules! field {
    ($id:expr, $e:expr, $p:expr) => {
        FieldModel {
            field_id: $id,
            extract_rate: $e,
            p_true: $p,
        }
    };
}

static ACC: &[FieldModel] = &[
    field!("accounting.invoice_number", 0.99, 0.96),
    field!("accounting.invoice_date", 0.99, 0.95),
    field!("amount.total", 0.98, 0.93),
    field!("amount.vat", 0.95, 0.92),
    field!("org.inn", 0.97, 0.97),
    field!("org.kpp", 0.90, 0.90),
    field!("org.name", 0.95, 0.84),
    field!("counterparty.name", 0.90, 0.80),
];
static HR: &[FieldModel] = &[
    field!("employee.name", 0.97, 0.86),
    field!("employee.position", 0.92, 0.83),
    field!("employee.hire_date", 0.95, 0.94),
    field!("hr.order_number", 0.96, 0.95),
    field!("hr.order_date", 0.96, 0.95),
    field!("org.name", 0.95, 0.84),
];
static LEGAL: &[FieldModel] = &[
    field!("contract.number", 0.98, 0.95),
    field!("contract.date", 0.98, 0.95),
    field!("contract.subject", 0.85, 0.78),
    field!("counterparty.name", 0.90, 0.80),
    field!("org.name", 0.95, 0.84),
    field!("amount.total", 0.90, 0.92),
];
static MED: &[FieldModel] = &[
    field!("patient.name", 0.96, 0.85),
    field!("medical.admission_date", 0.95, 0.94),
    field!("medical.discharge_date", 0.95, 0.94),
    field!("medical.diagnosis_code", 0.80, 0.88),
];
static EDU: &[FieldModel] = &[
    field!("subject.name", 0.96, 0.85),
    field!("document.date", 0.97, 0.95),
    field!("document.number", 0.96, 0.95),
    field!("org.name", 0.95, 0.84),
];

fn professions() -> Vec<Profession> {
    vec![
        Profession {
            name: "accounting",
            domain: DomainKind::Accounting,
            pack_id: "tier1-accounting-ru",
            kit: &["accounting.invoice", "accounting.service_act"],
            fields: ACC,
        },
        Profession {
            name: "hr",
            domain: DomainKind::Hr,
            pack_id: "tier1-hr-ru",
            kit: &["hr.employment_contract", "hr.employment_order"],
            fields: HR,
        },
        Profession {
            name: "legal",
            domain: DomainKind::Legal,
            pack_id: "tier1-legal-ru",
            kit: &["legal.contract", "legal.acceptance_act"],
            fields: LEGAL,
        },
        Profession {
            name: "medical",
            domain: DomainKind::Medical,
            pack_id: "tier1-medical-ru",
            kit: &["medical.discharge"],
            fields: MED,
        },
        Profession {
            name: "education",
            domain: DomainKind::Education,
            pack_id: "tier1-education-ru",
            kit: &["education.certificate"],
            fields: EDU,
        },
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runs: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let key = [0x5Au8; 32];
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);

    print!("{{\"schema\":\"dokkomplekt.ground-truth-corpus.v1\",\"entries\":[");
    let mut first = true;

    for prof in professions() {
        for run in 0..runs {
            let mut extracted_case = SemanticCase::default();
            let mut final_case = SemanticCase::default();

            for fm in prof.fields {
                let truth = format!("{}#{}#truth", fm.field_id, run);
                // Эталон содержит поле всегда.
                final_case.values.insert(
                    fm.field_id.into(),
                    SemanticValue::new(fm.field_id, truth.clone(), ValueSource::UserConfirmed, 1.0),
                );
                if rng.unit() > fm.extract_rate {
                    continue; // поле не извлеклось
                }
                let correct = rng.unit() < fm.p_true;
                let value = if correct {
                    truth
                } else {
                    format!("{}#{}#WRONG{}", fm.field_id, run, rng.next_u64() % 97)
                };
                let center = if correct { 0.93 } else { 0.80 };
                let confidence = (center + rng.noise(0.06)).clamp(0.05, 0.999) as f32;
                extracted_case.values.insert(
                    fm.field_id.into(),
                    SemanticValue::new(fm.field_id, value, ValueSource::Model, confidence),
                );
            }

            let source_sha = format!(
                "{:064x}",
                (run as u128) * 0x0100_0003 + prof.name.len() as u128
            );
            let request = CorpusEntryRequest {
                entry_id: format!("{}-{:05}", prof.name, run),
                case_id: format!("case-{}-{:05}", prof.name, run),
                source_sha256: &source_sha,
                fingerprint_key: &key,
                input_text: &format!("{} document {}", prof.name, run),
                domain: prof.domain.clone(),
                pack_id: Some(prof.pack_id.into()),
                cluster_id: None,
                model_case: &extracted_case,
                deterministic_case: &extracted_case,
                final_case: &final_case,
                proposed_kit_documents: prof.kit.iter().map(|s| s.to_string()).collect(),
                kit_proposal_source: Some("routing".into()),
                kit_documents: prof.kit.iter().map(|s| s.to_string()).collect(),
                created_at: "2026-07-24T00:00:00Z".into(),
            };
            // Ошибки пробрасываются типизированно, а не через panic: этот пример
            // поставляется и запускается, поэтому держится того же стандарта,
            // что и продакшен (см. audit_rust_production_panics).
            let entry = build_corpus_entry(request)?;
            let json = serde_json::to_string(&entry)?;
            if !first {
                print!(",");
            }
            first = false;
            print!("{json}");
        }
    }
    println!("]}}");
    Ok(())
}
