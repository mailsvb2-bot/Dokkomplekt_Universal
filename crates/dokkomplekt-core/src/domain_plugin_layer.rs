use crate::data_schema_engine::{UnifiedFieldDefinition, UnifiedFieldKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DomainPluginId {
    Core,
    Medical,
    Legal,
    Hr,
    Education,
    Accounting,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredFieldRuleV2 {
    pub role: String,
    pub field_id: String,
    pub when_flag: Option<String>,
    pub unless_present: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainPluginV2 {
    pub id: DomainPluginId,
    pub title: String,
    pub field_definitions: Vec<UnifiedFieldDefinition>,
    pub role_signals: BTreeMap<String, Vec<String>>,
    pub required_rules: Vec<RequiredFieldRuleV2>,
}

pub fn builtin_domain_plugins_v2() -> Vec<DomainPluginV2> {
    vec![
        core_plugin(),
        medical_plugin(),
        legal_plugin(),
        hr_plugin(),
        education_plugin(),
        accounting_plugin(),
        custom_plugin(),
    ]
}

pub fn plugin_by_id(id: &DomainPluginId) -> DomainPluginV2 {
    builtin_domain_plugins_v2()
        .into_iter()
        .find(|p| &p.id == id)
        .unwrap_or_else(custom_plugin)
}

fn field(
    id: &str,
    title: &str,
    aliases: &[&str],
    kind: UnifiedFieldKind,
) -> UnifiedFieldDefinition {
    UnifiedFieldDefinition {
        id: id.into(),
        title: title.into(),
        aliases: aliases.iter().map(|x| x.to_string()).collect(),
        kind,
    }
}

fn rule(role: &str, field_id: &str) -> RequiredFieldRuleV2 {
    RequiredFieldRuleV2 {
        role: role.into(),
        field_id: field_id.into(),
        when_flag: None,
        unless_present: None,
    }
}

fn core_plugin() -> DomainPluginV2 {
    DomainPluginV2 {
        id: DomainPluginId::Core,
        title: "UniversalDocCore".into(),
        field_definitions: vec![
            field(
                "person.full_name",
                "ФИО / имя субъекта",
                &["фио", "пациент", "сотрудник", "клиент"],
                UnifiedFieldKind::Text,
            ),
            field(
                "document.number",
                "Номер документа",
                &["номер документа", "document.number"],
                UnifiedFieldKind::Text,
            ),
            field(
                "document.date",
                "Дата документа",
                &["дата документа", "document.date"],
                UnifiedFieldKind::Date,
            ),
            field(
                "org.name",
                "Компания",
                &["компания", "организация"],
                UnifiedFieldKind::Text,
            ),
        ],
        role_signals: BTreeMap::from([("document".into(), vec!["документ".into()])]),
        required_rules: vec![],
    }
}

fn medical_plugin() -> DomainPluginV2 {
    DomainPluginV2 {
        id: DomainPluginId::Medical,
        title: "MedicalProfile".into(),
        field_definitions: vec![
            field(
                "medical.case_number",
                "Номер истории болезни",
                &["история болезни №", "номер истории болезни"],
                UnifiedFieldKind::Text,
            ),
            field(
                "medical.diagnosis",
                "Диагноз",
                &["диагноз", "клинический диагноз"],
                UnifiedFieldKind::LongText,
            ),
            field(
                "medical.treatment",
                "Лечение",
                &["лечение", "назначенное лечение"],
                UnifiedFieldKind::LongText,
            ),
            field(
                "medical.discharge_date",
                "Дата выписки",
                &["дата выписки"],
                UnifiedFieldKind::Date,
            ),
            field(
                "medical.sick_leave_number",
                "Номер больничного листа",
                &["больничный лист №"],
                UnifiedFieldKind::Text,
            ),
        ],
        role_signals: BTreeMap::from([
            (
                "discharge".into(),
                vec!["выписной эпикриз".into(), "выписка".into()],
            ),
            ("diaries".into(), vec!["дневник".into(), "дневники".into()]),
        ]),
        required_rules: vec![
            rule("*", "medical.case_number"),
            rule("*", "medical.diagnosis"),
            RequiredFieldRuleV2 {
                role: "discharge".into(),
                field_id: "medical.treatment".into(),
                when_flag: None,
                unless_present: Some("medical.treatment".into()),
            },
            rule("discharge", "medical.discharge_date"),
            RequiredFieldRuleV2 {
                role: "discharge".into(),
                field_id: "medical.sick_leave_number".into(),
                when_flag: Some("sick_leave_enabled".into()),
                unless_present: None,
            },
            rule("diaries", "medical.discharge_date"),
        ],
    }
}

fn legal_plugin() -> DomainPluginV2 {
    DomainPluginV2 {
        id: DomainPluginId::Legal,
        title: "LegalProfile".into(),
        field_definitions: vec![
            field(
                "contract.number",
                "Номер договора",
                &["договор №", "номер договора", "контракт №"],
                UnifiedFieldKind::Text,
            ),
            field(
                "contract.date",
                "Дата договора",
                &["дата договора", "дата контракта"],
                UnifiedFieldKind::Date,
            ),
            field(
                "contract.party_a",
                "Сторона 1",
                &["заказчик", "продавец", "арендодатель"],
                UnifiedFieldKind::Text,
            ),
            field(
                "contract.party_b",
                "Сторона 2",
                &["исполнитель", "покупатель", "арендатор"],
                UnifiedFieldKind::Text,
            ),
            field(
                "contract.subject",
                "Предмет договора",
                &["предмет договора", "предмет контракта"],
                UnifiedFieldKind::LongText,
            ),
            field(
                "contract.amount",
                "Сумма договора",
                &["цена договора", "сумма договора", "стоимость"],
                UnifiedFieldKind::Money,
            ),
            field(
                "legal.claim_subject",
                "Предмет претензии",
                &["предмет претензии", "требование"],
                UnifiedFieldKind::LongText,
            ),
            field(
                "legal.claim_amount",
                "Сумма требования",
                &["сумма претензии", "сумма требования"],
                UnifiedFieldKind::Money,
            ),
            field(
                "counterparty.name",
                "Контрагент",
                &["контрагент", "получатель", "адресат"],
                UnifiedFieldKind::Text,
            ),
        ],
        role_signals: BTreeMap::from([
            ("contract".into(), vec!["договор".into(), "контракт".into()]),
            (
                "acceptance_act".into(),
                vec![
                    "акт приёмки".into(),
                    "акт приемки".into(),
                    "акт выполненных".into(),
                ],
            ),
            (
                "claim".into(),
                vec!["претензия".into(), "досудебное требование".into()],
            ),
            (
                "cover_letter".into(),
                vec!["сопроводительное письмо".into()],
            ),
        ]),
        required_rules: vec![
            rule("contract", "contract.number"),
            rule("contract", "contract.date"),
            rule("contract", "contract.party_a"),
            rule("contract", "contract.party_b"),
            rule("acceptance_act", "document.number"),
            rule("acceptance_act", "document.date"),
            rule("acceptance_act", "contract.number"),
            rule("acceptance_act", "contract.party_a"),
            rule("acceptance_act", "contract.party_b"),
            rule("claim", "document.number"),
            rule("claim", "document.date"),
            rule("claim", "contract.party_a"),
            rule("claim", "contract.party_b"),
            rule("claim", "legal.claim_subject"),
            rule("cover_letter", "document.number"),
            rule("cover_letter", "document.date"),
            rule("cover_letter", "org.name"),
            rule("cover_letter", "counterparty.name"),
        ],
    }
}

fn hr_plugin() -> DomainPluginV2 {
    DomainPluginV2 {
        id: DomainPluginId::Hr,
        title: "HRProfile".into(),
        field_definitions: vec![
            field(
                "hr.order_number",
                "Номер приказа",
                &["приказ №", "номер приказа"],
                UnifiedFieldKind::Text,
            ),
            field(
                "hr.order_date",
                "Дата приказа",
                &["дата приказа"],
                UnifiedFieldKind::Date,
            ),
            field(
                "employee.name",
                "Сотрудник",
                &["сотрудник", "работник", "фио сотрудника"],
                UnifiedFieldKind::Text,
            ),
            field(
                "employee.position",
                "Должность",
                &["должность"],
                UnifiedFieldKind::Text,
            ),
            field(
                "employee.department",
                "Подразделение",
                &["подразделение", "отдел"],
                UnifiedFieldKind::Text,
            ),
            field(
                "employee.hire_date",
                "Дата приёма",
                &["дата приёма", "дата приема", "приступить к работе"],
                UnifiedFieldKind::Date,
            ),
            field(
                "employee.salary",
                "Оклад",
                &["оклад", "заработная плата"],
                UnifiedFieldKind::Money,
            ),
            field(
                "employee.contract_number",
                "Номер трудового договора",
                &["трудовой договор №"],
                UnifiedFieldKind::Text,
            ),
            field(
                "employee.tab_number",
                "Табельный номер",
                &["табельный номер"],
                UnifiedFieldKind::Text,
            ),
        ],
        role_signals: BTreeMap::from([
            (
                "employment_contract".into(),
                vec!["трудовой договор".into()],
            ),
            (
                "employment_order".into(),
                vec!["приказ о приёме".into(), "приказ о приеме".into()],
            ),
            (
                "personal_data_consent".into(),
                vec!["согласие на обработку персональных данных".into()],
            ),
            (
                "familiarization_sheet".into(),
                vec!["лист ознакомления".into()],
            ),
        ]),
        required_rules: vec![
            rule("employment_contract", "document.date"),
            rule("employment_contract", "employee.name"),
            rule("employment_contract", "employee.position"),
            rule("employment_contract", "employee.hire_date"),
            rule("employment_contract", "employee.contract_number"),
            rule("employment_order", "hr.order_number"),
            rule("employment_order", "hr.order_date"),
            rule("employment_order", "employee.name"),
            rule("employment_order", "employee.position"),
            rule("employment_order", "employee.hire_date"),
            rule("personal_data_consent", "document.date"),
            rule("personal_data_consent", "employee.name"),
            rule("familiarization_sheet", "document.date"),
            rule("familiarization_sheet", "employee.name"),
            rule("familiarization_sheet", "employee.position"),
        ],
    }
}

fn education_plugin() -> DomainPluginV2 {
    DomainPluginV2 {
        id: DomainPluginId::Education,
        title: "EducationProfile".into(),
        field_definitions: vec![
            field(
                "education.student_name",
                "Студент / ученик",
                &["студент", "ученик", "обучающийся"],
                UnifiedFieldKind::Text,
            ),
            field(
                "education.group",
                "Группа / класс",
                &["группа", "класс"],
                UnifiedFieldKind::Text,
            ),
            field(
                "education.course",
                "Курс / предмет",
                &["курс", "предмет", "дисциплина"],
                UnifiedFieldKind::Text,
            ),
            field(
                "education.grade",
                "Оценка",
                &["оценка", "балл", "результат"],
                UnifiedFieldKind::Text,
            ),
            field(
                "education.institution",
                "Образовательная организация",
                &["учебное заведение", "образовательная организация"],
                UnifiedFieldKind::Text,
            ),
        ],
        role_signals: BTreeMap::from([
            (
                "certificate".into(),
                vec!["справка об обучении".into(), "справка".into()],
            ),
            (
                "grade_report".into(),
                vec!["ведомость".into(), "успеваемость".into()],
            ),
        ]),
        required_rules: vec![
            rule("certificate", "education.student_name"),
            rule("certificate", "document.date"),
            rule("grade_report", "education.student_name"),
            rule("grade_report", "education.course"),
            rule("grade_report", "education.grade"),
        ],
    }
}

fn accounting_plugin() -> DomainPluginV2 {
    DomainPluginV2 {
        id: DomainPluginId::Accounting,
        title: "AccountingProfile".into(),
        field_definitions: vec![
            field(
                "accounting.invoice_number",
                "Номер счёта",
                &["счёт №", "счет №", "номер счёта"],
                UnifiedFieldKind::Text,
            ),
            field(
                "accounting.invoice_date",
                "Дата счёта",
                &["дата счёта", "дата счета"],
                UnifiedFieldKind::Date,
            ),
            field(
                "counterparty.name",
                "Контрагент",
                &["покупатель", "заказчик", "контрагент"],
                UnifiedFieldKind::Text,
            ),
            field(
                "amount.total",
                "Итоговая сумма",
                &["итого", "к оплате", "сумма"],
                UnifiedFieldKind::Money,
            ),
            field(
                "amount.vat",
                "НДС",
                &["ндс", "в том числе ндс"],
                UnifiedFieldKind::Money,
            ),
            field(
                "amount.currency",
                "Валюта",
                &["валюта", "код валюты"],
                UnifiedFieldKind::Text,
            ),
        ],
        role_signals: BTreeMap::from([
            (
                "invoice".into(),
                vec!["счёт".into(), "счет".into(), "invoice".into()],
            ),
            (
                "service_act".into(),
                vec!["акт оказанных услуг".into(), "акт выполненных работ".into()],
            ),
            (
                "reconciliation".into(),
                vec!["акт сверки".into(), "сверка расчётов".into()],
            ),
        ]),
        required_rules: vec![
            rule("invoice", "accounting.invoice_number"),
            rule("invoice", "accounting.invoice_date"),
            rule("invoice", "org.name"),
            rule("invoice", "counterparty.name"),
            rule("invoice", "amount.total"),
            rule("service_act", "document.number"),
            rule("service_act", "document.date"),
            rule("service_act", "org.name"),
            rule("service_act", "counterparty.name"),
            rule("service_act", "amount.total"),
            rule("reconciliation", "document.date"),
            rule("reconciliation", "org.name"),
            rule("reconciliation", "counterparty.name"),
        ],
    }
}

fn custom_plugin() -> DomainPluginV2 {
    DomainPluginV2 {
        id: DomainPluginId::Custom,
        title: "CustomUserProfile".into(),
        field_definitions: vec![],
        role_signals: BTreeMap::new(),
        required_rules: vec![],
    }
}
