use crate::{DomainKind, FieldDefinition};
use std::collections::{BTreeMap, BTreeSet};

pub fn generic_fields() -> Vec<FieldDefinition> {
    vec![
        field(
            "subject.name",
            "Имя / субъект документа",
            DomainKind::Generic,
            true,
            &[
                "ФИО",
                "Ф.И.О.",
                "ФИО пациента",
                "ФИО сотрудника",
                "fullName",
                "patientName",
                "patientFio",
                "subject",
                "client.name",
                "patient.fio",
                "patient.full_name",
            ],
        ),
        field(
            "subject.birth_date",
            "Дата рождения",
            DomainKind::Generic,
            false,
            &["birthDate", "Дата рождения", "patient.birth_date"],
        ),
        field(
            "subject.age",
            "Возраст",
            DomainKind::Generic,
            false,
            &["age", "Возраст", "patient.age", "person.age"],
        ),
        field(
            "subject.address",
            "Адрес",
            DomainKind::Generic,
            false,
            &["address", "Адрес", "Место жительства", "patient.address"],
        ),
        field(
            "document.number",
            "Номер документа",
            DomainKind::Generic,
            false,
            &["documentNo", "doc.number", "Номер документа"],
        ),
        field(
            "document.date",
            "Дата документа",
            DomainKind::Generic,
            false,
            &["date", "doc.date", "Дата"],
        ),
        field(
            "document.title",
            "Название документа",
            DomainKind::Generic,
            false,
            &["title", "doc.title", "Название документа"],
        ),
        field(
            "subject.organization",
            "Организация / место работы",
            DomainKind::Generic,
            false,
            &[
                "workPlace",
                "workplace",
                "Место работы",
                "Организация работы",
            ],
        ),
        field(
            "subject.position",
            "Должность / роль",
            DomainKind::Generic,
            false,
            &["jobTitle", "position", "Должность", "Роль"],
        ),
        field(
            "organization.name",
            "Организация",
            DomainKind::Generic,
            false,
            &["company", "organization", "Организация"],
        ),
        field(
            "output.folder_name",
            "Название папки результата",
            DomainKind::Generic,
            false,
            &["folder", "patient.folder"],
        ),
    ]
}

pub fn medical_fields() -> Vec<FieldDefinition> {
    vec![
        field(
            "medical.case_number",
            "Номер истории болезни",
            DomainKind::Medical,
            true,
            &[
                "case.number",
                "medicalRecordNo",
                "caseNo",
                "case #",
                "ИБ №",
                "История болезни №",
                "Номер истории болезни",
            ],
        ),
        field(
            "medical.diagnosis",
            "Диагноз",
            DomainKind::Medical,
            false,
            &[
                "diagnosis",
                "mainDiagnosis",
                "diagnosisMain",
                "Диагноз",
                "Клинический диагноз",
            ],
        ),
        field(
            "medical.icd10",
            "Код МКБ-10",
            DomainKind::Medical,
            false,
            &[
                "icd_code",
                "diagnosisCode",
                "ICD-10",
                "MKB-10",
                "МКБ-10",
                "Код МКБ",
                "Код МКБ-10",
                "medical.diagnosis_code",
            ],
        ),
        field(
            "medical.treatment",
            "Лечение",
            DomainKind::Medical,
            false,
            &[
                "treatment.plan",
                "treatmentPlan",
                "assignedTreatment",
                "Назначенное лечение",
                "Назначения",
                "Лечение",
            ],
        ),
        field(
            "medical.admission_date",
            "Дата поступления",
            DomainKind::Medical,
            false,
            &[
                "admission.date",
                "admissionDate",
                "hospitalizationDate",
                "Дата поступления",
                "Дата госпитализации",
            ],
        ),
        field(
            "medical.discharge_date",
            "Дата выписки",
            DomainKind::Medical,
            false,
            &["discharge.date", "dischargeDate", "Дата выписки"],
        ),
        field(
            "medical.complaints",
            "Жалобы",
            DomainKind::Medical,
            false,
            &["complaints", "Жалобы", "Жалобы при поступлении"],
        ),
        field(
            "medical.anamnesis_disease",
            "Анамнез заболевания",
            DomainKind::Medical,
            false,
            &[
                "anamnesis.disease",
                "disease_anamnesis",
                "Анамнез заболевания",
            ],
        ),
        field(
            "medical.anamnesis_life",
            "Анамнез жизни",
            DomainKind::Medical,
            false,
            &["anamnesis.life", "life_anamnesis", "Анамнез жизни"],
        ),
        field(
            "medical.profile_status",
            "Профильный статус",
            DomainKind::Medical,
            false,
            &[
                "status.profile",
                "status.mental",
                "mental_status",
                "Профильный статус",
                "Психический статус",
            ],
        ),
        field(
            "medical.somatic_status",
            "Соматический / объективный статус",
            DomainKind::Medical,
            false,
            &[
                "status.objective",
                "status.somatic",
                "somatic_status",
                "Соматический статус",
                "Объективный статус",
            ],
        ),
        field(
            "medical.examination_plan",
            "План обследования",
            DomainKind::Medical,
            false,
            &["examination.plan", "examination_plan", "План обследования"],
        ),
        field(
            "medical.treatment_result",
            "Результат лечения",
            DomainKind::Medical,
            false,
            &["treatment.result", "Результат лечения", "Исход лечения"],
        ),
        field(
            "medical.labs_source",
            "Источник результатов исследований",
            DomainKind::Medical,
            false,
            &["labs.source", "Источник анализов"],
        ),
        field(
            "medical.labs_date_policy",
            "Политика даты исследований",
            DomainKind::Medical,
            false,
            &["labs.date_policy", "Политика даты анализов"],
        ),
        field(
            "medical.labs_without",
            "Без лабораторных исследований",
            DomainKind::Medical,
            false,
            &["labs.without", "Без анализов"],
        ),
        field(
            "medical.diary_schedule_style",
            "График дневников",
            DomainKind::Medical,
            false,
            &["График дневников", "Режим дневников"],
        ),
        field(
            "medical.diary_intraday_rhythm",
            "Ритм записей в течение дня",
            DomainKind::Medical,
            false,
            &["Ритм дневников", "Интервал дневников"],
        ),
        field(
            "medical.diary_day_start_time",
            "Начало времени дневников",
            DomainKind::Medical,
            false,
            &["Начало записей", "Время начала дневников"],
        ),
        field(
            "medical.diary_day_end_time",
            "Окончание времени дневников",
            DomainKind::Medical,
            false,
            &["Окончание записей", "Время окончания дневников"],
        ),
        field(
            "medical.commission_date",
            "Дата комиссии / осмотра",
            DomainKind::Medical,
            false,
            &[
                "commission.date",
                "Дата комиссии",
                "Дата проведения комиссии",
                "Дата ВК",
            ],
        ),
        field(
            "medical.protocol_date",
            "Дата протокола",
            DomainKind::Medical,
            false,
            &["protocol.date", "Дата протокола", "Протокол от"],
        ),
        field(
            "medical.sick_leave_commission_date",
            "Дата комиссии по больничному листу",
            DomainKind::Medical,
            false,
            &[
                "sick_leave.commission_date",
                "Дата комиссии по больничному",
                "Дата проведения комиссии",
            ],
        ),
        field(
            "medical.sick_leave_number",
            "Номер больничного листа",
            DomainKind::Medical,
            false,
            &[
                "sickLeaveNo",
                "sickLeaveNumber",
                "Номер больничного",
                "Больничный лист №",
            ],
        ),
        field(
            "medical.sick_leave_from",
            "Больничный лист с",
            DomainKind::Medical,
            false,
            &[
                "expert.sick_leave_from",
                "sickLeaveFrom",
                "Больничный с",
                "Лечится с",
            ],
        ),
        field(
            "medical.protocol_number",
            "Номер протокола",
            DomainKind::Medical,
            false,
            &[
                "protocol.number",
                "protocolNo",
                "Номер протокола",
                "Протокол №",
            ],
        ),
        field(
            "medical.commission_number",
            "Номер комиссии",
            DomainKind::Medical,
            false,
            &[
                "commission.number",
                "commissionNo",
                "Номер комиссии",
                "Комиссия №",
            ],
        ),
        field(
            "medical.rvk_act_number",
            "Номер акта / заключения РВК",
            DomainKind::Medical,
            false,
            &[
                "rvk.act_number",
                "rvkActNo",
                "Номер акта РВК",
                "Акт РВК №",
                "Заключение №",
            ],
        ),
        field(
            "medical.discharge_condition",
            "Состояние при выписке",
            DomainKind::Medical,
            false,
            &[
                "discharge.condition",
                "condition.discharge",
                "dischargeCondition",
                "Состояние при выписке",
            ],
        ),
        field(
            "medical.recommendations",
            "Рекомендации",
            DomainKind::Medical,
            false,
            &[
                "recommendations",
                "discharge.recommendations",
                "Рекомендации",
            ],
        ),
        field(
            "medical.labs",
            "Лабораторные и иные исследования",
            DomainKind::Medical,
            false,
            &[
                "labs.results",
                "labs.block",
                "analysis.results",
                "analyses.results",
                "labResults",
                "Лабораторные исследования",
                "Анализы",
            ],
        ),
        field(
            "medical.labs_date",
            "Дата исследований",
            DomainKind::Medical,
            false,
            &[
                "labs.date",
                "labsDate",
                "Дата анализов",
                "Дата лабораторных исследований",
            ],
        ),
        field(
            "medical.rvk_commissariat",
            "Военный комиссариат",
            DomainKind::Medical,
            false,
            &["РВК", "Военный комиссариат"],
        ),
        field(
            "medical.workplace",
            "Место работы",
            DomainKind::Medical,
            false,
            &["Место работы", "workplace", "Организация работы"],
        ),
        field(
            "medical.position",
            "Должность",
            DomainKind::Medical,
            false,
            &["Должность", "position"],
        ),
        field(
            "medical.attending_doctor",
            "Лечащий врач",
            DomainKind::Medical,
            false,
            &["Лечащий врач", "doctor.attending"],
        ),
        field(
            "medical.department_head",
            "Зав. отделением",
            DomainKind::Medical,
            false,
            &["Зав. отделением", "department.head"],
        ),
    ]
}

pub fn legal_fields() -> Vec<FieldDefinition> {
    vec![
        field(
            "legal.contract_number",
            "Номер договора",
            DomainKind::Legal,
            true,
            &[
                "contract.number",
                "Договор №",
                "Номер договора",
                "Контракт №",
            ],
        ),
        field(
            "legal.contract_date",
            "Дата договора",
            DomainKind::Legal,
            true,
            &["contract.date", "Дата договора", "Дата контракта"],
        ),
        field(
            "legal.party_a",
            "Сторона А",
            DomainKind::Legal,
            true,
            &[
                "party.a",
                "Заказчик",
                "Сторона 1",
                "Арендодатель",
                "Продавец",
            ],
        ),
        field(
            "legal.party_b",
            "Сторона Б",
            DomainKind::Legal,
            true,
            &[
                "party.b",
                "Исполнитель",
                "Сторона 2",
                "Арендатор",
                "Покупатель",
            ],
        ),
        field(
            "legal.subject",
            "Предмет договора",
            DomainKind::Legal,
            false,
            &["contract.subject", "Предмет договора", "Предмет"],
        ),
        field(
            "legal.amount",
            "Сумма договора",
            DomainKind::Legal,
            false,
            &[
                "contract.amount",
                "Цена договора",
                "Сумма договора",
                "Стоимость",
            ],
        ),
        field(
            "legal.deadline",
            "Срок исполнения",
            DomainKind::Legal,
            false,
            &["deadline", "Срок исполнения", "Действует до"],
        ),
        field(
            "legal.claim_subject",
            "Предмет претензии",
            DomainKind::Legal,
            true,
            &["Предмет претензии", "Требование", "Основание требования"],
        ),
        field(
            "legal.claim_amount",
            "Сумма требования",
            DomainKind::Legal,
            false,
            &["Сумма претензии", "Сумма требования"],
        ),
    ]
}

pub fn hr_fields() -> Vec<FieldDefinition> {
    vec![
        field(
            "hr.order_number",
            "Номер приказа",
            DomainKind::Hr,
            true,
            &["order.number", "Приказ №", "Номер приказа"],
        ),
        field(
            "hr.order_date",
            "Дата приказа",
            DomainKind::Hr,
            true,
            &["order.date", "Дата приказа"],
        ),
        field(
            "hr.employee_name",
            "Сотрудник",
            DomainKind::Hr,
            true,
            &["employee.name", "Сотрудник", "Работник", "ФИО сотрудника"],
        ),
        field(
            "hr.position",
            "Должность",
            DomainKind::Hr,
            true,
            &["employee.position", "Должность"],
        ),
        field(
            "hr.department",
            "Отдел",
            DomainKind::Hr,
            false,
            &["department", "Отдел", "Подразделение"],
        ),
    ]
}

pub fn education_fields() -> Vec<FieldDefinition> {
    vec![
        field(
            "education.student_name",
            "Студент / ученик",
            DomainKind::Education,
            true,
            &["student.name", "Студент", "Ученик", "Обучающийся"],
        ),
        field(
            "education.group",
            "Группа / класс",
            DomainKind::Education,
            false,
            &["group", "Группа", "Класс"],
        ),
        field(
            "education.course",
            "Курс / предмет",
            DomainKind::Education,
            false,
            &["course", "Предмет", "Дисциплина"],
        ),
        field(
            "education.grade",
            "Оценка",
            DomainKind::Education,
            false,
            &["grade", "Оценка", "Балл"],
        ),
        field(
            "education.institution",
            "Образовательная организация",
            DomainKind::Education,
            false,
            &[
                "Учебное заведение",
                "Образовательная организация",
                "Школа",
                "ВУЗ",
            ],
        ),
    ]
}

pub fn accounting_fields() -> Vec<FieldDefinition> {
    vec![
        field(
            "accounting.invoice_number",
            "Номер счёта",
            DomainKind::Accounting,
            true,
            &["invoice.number", "Счет №", "Счёт №"],
        ),
        field(
            "accounting.invoice_date",
            "Дата счёта",
            DomainKind::Accounting,
            true,
            &["invoice.date", "Дата счёта", "Дата счета"],
        ),
        field(
            "accounting.client",
            "Клиент",
            DomainKind::Accounting,
            true,
            &["client", "Клиент", "Покупатель", "Плательщик"],
        ),
        field(
            "accounting.inn",
            "ИНН",
            DomainKind::Accounting,
            false,
            &["inn", "ИНН"],
        ),
        field(
            "accounting.kpp",
            "КПП",
            DomainKind::Accounting,
            false,
            &["kpp", "КПП"],
        ),
        field(
            "accounting.amount_total",
            "Сумма",
            DomainKind::Accounting,
            true,
            &["amount.total", "К оплате", "Итого", "Сумма"],
        ),
        field(
            "accounting.currency",
            "Валюта",
            DomainKind::Accounting,
            false,
            &["currency", "Валюта"],
        ),
        field(
            "amount.vat",
            "НДС",
            DomainKind::Accounting,
            false,
            &["НДС", "В том числе НДС", "Сумма НДС"],
        ),
    ]
}

pub fn universal_v18_fields() -> Vec<FieldDefinition> {
    vec![
        field(
            "counterparty.name",
            "Контрагент",
            DomainKind::Generic,
            false,
            &[
                "Контрагент",
                "Покупатель",
                "Заказчик",
                "Получатель",
                "Адресат",
            ],
        ),
        field(
            "counterparty.inn",
            "ИНН контрагента",
            DomainKind::Generic,
            false,
            &["ИНН контрагента", "ИНН покупателя", "ИНН заказчика"],
        ),
        field(
            "counterparty.kpp",
            "КПП контрагента",
            DomainKind::Generic,
            false,
            &["КПП контрагента", "КПП покупателя", "КПП заказчика"],
        ),
        field(
            "amount.total",
            "Итоговая сумма",
            DomainKind::Generic,
            false,
            &["Итого", "К оплате", "Общая сумма"],
        ),
        field(
            "amount.currency",
            "Валюта суммы",
            DomainKind::Generic,
            false,
            &["Валюта", "Код валюты"],
        ),
        field(
            "subject.snils",
            "СНИЛС",
            DomainKind::Generic,
            false,
            &["СНИЛС", "snils"],
        ),
        field(
            "subject.gender",
            "Пол",
            DomainKind::Generic,
            false,
            &["Пол", "gender"],
        ),
        field(
            "subject.birth_place",
            "Место рождения",
            DomainKind::Generic,
            false,
            &["Место рождения"],
        ),
        field(
            "subject.passport_series",
            "Серия паспорта",
            DomainKind::Generic,
            false,
            &["Серия паспорта"],
        ),
        field(
            "subject.passport_number",
            "Номер паспорта",
            DomainKind::Generic,
            false,
            &["Номер паспорта"],
        ),
        field(
            "subject.passport_issued_by",
            "Кем выдан паспорт",
            DomainKind::Generic,
            false,
            &["Кем выдан"],
        ),
        field(
            "subject.passport_issued_date",
            "Дата выдачи паспорта",
            DomainKind::Generic,
            false,
            &["Дата выдачи"],
        ),
        field(
            "subject.passport_code",
            "Код подразделения",
            DomainKind::Generic,
            false,
            &["Код подразделения"],
        ),
        field(
            "subject.address_registration",
            "Адрес регистрации",
            DomainKind::Generic,
            false,
            &["Адрес регистрации"],
        ),
        field(
            "subject.address_actual",
            "Фактический адрес",
            DomainKind::Generic,
            false,
            &["Фактический адрес"],
        ),
        field(
            "subject.phone",
            "Телефон",
            DomainKind::Generic,
            false,
            &["Телефон", "phone"],
        ),
        field(
            "subject.email",
            "Электронная почта",
            DomainKind::Generic,
            false,
            &["E-mail", "email"],
        ),
        field(
            "subject.inn_person",
            "ИНН физлица",
            DomainKind::Generic,
            false,
            &["ИНН физического лица"],
        ),
        field(
            "org.name",
            "Организация",
            DomainKind::Generic,
            false,
            &["organization.name", "Наименование организации"],
        ),
        field(
            "org.inn",
            "ИНН организации",
            DomainKind::Generic,
            false,
            &["ИНН", "inn"],
        ),
        field(
            "org.ogrn",
            "ОГРН / ОГРНИП",
            DomainKind::Generic,
            false,
            &["ОГРН", "ОГРНИП"],
        ),
        field("org.kpp", "КПП", DomainKind::Generic, false, &["КПП"]),
        field("org.okpo", "ОКПО", DomainKind::Generic, false, &["ОКПО"]),
        field("org.okved", "ОКВЭД", DomainKind::Generic, false, &["ОКВЭД"]),
        field(
            "org.legal_address",
            "Юридический адрес",
            DomainKind::Generic,
            false,
            &["Юридический адрес"],
        ),
        field(
            "org.actual_address",
            "Фактический адрес организации",
            DomainKind::Generic,
            false,
            &["Фактический адрес организации"],
        ),
        field(
            "org.bank_name",
            "Банк",
            DomainKind::Generic,
            false,
            &["Наименование банка", "Банк"],
        ),
        field("org.bank_bik", "БИК", DomainKind::Generic, false, &["БИК"]),
        field(
            "org.bank_account",
            "Расчётный счёт",
            DomainKind::Generic,
            false,
            &["р/с", "Расчетный счет", "Расчётный счёт"],
        ),
        field(
            "org.bank_corr_account",
            "Корреспондентский счёт",
            DomainKind::Generic,
            false,
            &["к/с", "Корреспондентский счет"],
        ),
        field(
            "org.director_name",
            "Руководитель",
            DomainKind::Generic,
            false,
            &["Директор", "Руководитель", "в лице"],
        ),
        field(
            "org.director_position",
            "Должность руководителя",
            DomainKind::Generic,
            false,
            &["Должность руководителя"],
        ),
        field(
            "org.director_basis",
            "Основание полномочий",
            DomainKind::Generic,
            false,
            &["действующего на основании", "Основание полномочий"],
        ),
        field(
            "employee.name",
            "Сотрудник",
            DomainKind::Hr,
            false,
            &["hr.employee_name"],
        ),
        field(
            "employee.tab_number",
            "Табельный номер",
            DomainKind::Hr,
            false,
            &["Табельный номер"],
        ),
        field(
            "employee.position",
            "Должность сотрудника",
            DomainKind::Hr,
            false,
            &["hr.position"],
        ),
        field(
            "employee.department",
            "Подразделение",
            DomainKind::Hr,
            false,
            &["hr.department"],
        ),
        field(
            "employee.hire_date",
            "Дата приёма",
            DomainKind::Hr,
            false,
            &["Дата приема", "Дата приёма"],
        ),
        field(
            "employee.salary",
            "Оклад",
            DomainKind::Hr,
            false,
            &["Оклад", "Заработная плата"],
        ),
        field(
            "employee.contract_number",
            "Номер трудового договора",
            DomainKind::Hr,
            false,
            &["Номер трудового договора"],
        ),
        field(
            "contract.number",
            "Номер договора",
            DomainKind::Legal,
            false,
            &["legal.contract_number"],
        ),
        field(
            "contract.date",
            "Дата договора",
            DomainKind::Legal,
            false,
            &["legal.contract_date"],
        ),
        field(
            "contract.subject",
            "Предмет договора",
            DomainKind::Legal,
            false,
            &["legal.subject"],
        ),
        field(
            "contract.start_date",
            "Начало договора",
            DomainKind::Legal,
            false,
            &["Дата начала"],
        ),
        field(
            "contract.end_date",
            "Окончание договора",
            DomainKind::Legal,
            false,
            &["Дата окончания"],
        ),
        field(
            "contract.amount",
            "Сумма договора",
            DomainKind::Legal,
            false,
            &["legal.amount"],
        ),
        field(
            "contract.currency",
            "Валюта",
            DomainKind::Legal,
            false,
            &["Валюта"],
        ),
        field(
            "contract.penalty_percent",
            "Процент неустойки",
            DomainKind::Legal,
            false,
            &["Неустойка", "Процент пени"],
        ),
        field(
            "realty.cadastral_number",
            "Кадастровый номер",
            DomainKind::Generic,
            false,
            &["Кадастровый номер"],
        ),
        field(
            "realty.address",
            "Адрес объекта",
            DomainKind::Generic,
            false,
            &["Адрес объекта"],
        ),
        field(
            "realty.area",
            "Площадь",
            DomainKind::Generic,
            false,
            &["Площадь"],
        ),
        field(
            "realty.floor",
            "Этаж",
            DomainKind::Generic,
            false,
            &["Этаж"],
        ),
        field(
            "realty.rooms",
            "Количество комнат",
            DomainKind::Generic,
            false,
            &["Количество комнат"],
        ),
        field("vehicle.vin", "VIN", DomainKind::Generic, false, &["VIN"]),
        field(
            "vehicle.gos_number",
            "Государственный номер",
            DomainKind::Generic,
            false,
            &["Госномер", "Государственный номер"],
        ),
        field(
            "vehicle.brand_model",
            "Марка и модель",
            DomainKind::Generic,
            false,
            &["Марка модель", "Марка и модель"],
        ),
        field(
            "vehicle.year",
            "Год выпуска",
            DomainKind::Generic,
            false,
            &["Год выпуска"],
        ),
        field(
            "vehicle.pts_number",
            "Номер ПТС",
            DomainKind::Generic,
            false,
            &["ПТС", "Номер ПТС"],
        ),
        field(
            "doctor.snils",
            "СНИЛС специалиста",
            DomainKind::Medical,
            false,
            &["СНИЛС врача"],
        ),
        field(
            "doctor.position_code",
            "Код должности специалиста",
            DomainKind::Medical,
            false,
            &["Код должности врача"],
        ),
        field(
            "doctor.department",
            "Подразделение специалиста",
            DomainKind::Medical,
            false,
            &["Отделение врача"],
        ),
        field(
            "animal.name",
            "Кличка животного",
            DomainKind::Generic,
            false,
            &["Кличка"],
        ),
        field(
            "animal.species",
            "Вид животного",
            DomainKind::Generic,
            false,
            &["Вид животного"],
        ),
        field(
            "animal.breed",
            "Порода",
            DomainKind::Generic,
            false,
            &["Порода"],
        ),
        field(
            "animal.chip",
            "Номер чипа",
            DomainKind::Generic,
            false,
            &["Чип", "Номер чипа"],
        ),
    ]
}

pub fn all_fields() -> Vec<FieldDefinition> {
    let mut merged = BTreeMap::<String, FieldDefinition>::new();
    for mut definition in generic_fields()
        .into_iter()
        .chain(medical_fields())
        .chain(legal_fields())
        .chain(hr_fields())
        .chain(education_fields())
        .chain(accounting_fields())
        .chain(universal_v18_fields())
    {
        let original_id = definition.id.clone();
        let canonical = crate::canonical_storage_field_id(&original_id);
        definition.id = canonical.clone();
        if original_id != canonical && !definition.aliases.contains(&original_id) {
            definition.aliases.push(original_id);
        }
        let entry = merged
            .entry(canonical)
            .or_insert_with(|| definition.clone());
        entry.required_by_default |= definition.required_by_default;
        if entry.domain == DomainKind::Generic && definition.domain != DomainKind::Generic {
            entry.domain = definition.domain.clone();
        }
        if entry.title_ru.starts_with("Пользовательское поле") || entry.title_ru.trim().is_empty()
        {
            entry.title_ru = definition.title_ru.clone();
        }
        for alias in definition.aliases {
            if alias != entry.id && !entry.aliases.contains(&alias) {
                entry.aliases.push(alias);
            }
        }
    }
    merged.into_values().collect()
}

/// Normalize a human/export-style placeholder for registry comparison.
/// Punctuation, whitespace and camelCase separators are intentionally ignored,
/// while Unicode letters and digits are preserved. `ё` and `е` compare equally.
pub fn normalized_field_alias_key(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .replace('ё', "е")
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

/// Return every canonical registry field matching a canonical id, title or alias.
/// Multiple values are preserved because labels such as «Должность» are
/// legitimately domain-dependent and must not be guessed globally.
pub fn canonical_field_candidates(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let key = normalized_field_alias_key(trimmed);
    let mut matches = all_fields()
        .into_iter()
        .filter(|definition| {
            definition.id == trimmed
                || normalized_field_alias_key(&definition.id) == key
                || normalized_field_alias_key(&definition.title_ru) == key
                || definition
                    .aliases
                    .iter()
                    .any(|alias| normalized_field_alias_key(alias) == key)
        })
        .map(|definition| crate::canonical_storage_field_id(&definition.id))
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();
    matches
}

/// Resolve a placeholder to one canonical field. A preferred domain is used
/// only to disambiguate registry aliases; arbitrary safe custom ids remain valid.
pub fn canonical_field_id_for_domain(
    raw: &str,
    preferred_domain: Option<&DomainKind>,
) -> Option<String> {
    let trimmed = raw.trim();
    let candidates = canonical_field_candidates(trimmed);
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }
    if let Some(domain) = preferred_domain {
        let definitions = all_fields();
        let candidates_for = |target: &DomainKind| {
            let mut values = candidates
                .iter()
                .filter(|candidate| {
                    definitions.iter().any(|definition| {
                        &definition.id == *candidate && &definition.domain == target
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            values.sort();
            values.dedup();
            values
        };
        let preferred = candidates_for(domain);
        if preferred.len() == 1 {
            return preferred.into_iter().next();
        }
        if preferred.is_empty() {
            let generic = candidates_for(&DomainKind::Generic);
            if generic.len() == 1 {
                return generic.into_iter().next();
            }
        }
    }
    if candidates.is_empty() && is_valid_field_id(trimmed) {
        return Some(trimmed.to_string());
    }
    None
}

pub fn canonical_field_id(raw: &str) -> Option<String> {
    canonical_field_id_for_domain(raw, None)
}

pub fn is_valid_or_registered_field(raw: &str) -> bool {
    is_valid_field_id(raw) || !canonical_field_candidates(raw).is_empty()
}

pub fn known_field_ids() -> BTreeSet<String> {
    all_fields()
        .into_iter()
        .map(|field| crate::canonical_storage_field_id(&field.id))
        .collect()
}

/// Universal constructor rule: arbitrary user fields are allowed if they are safe semantic ids.
/// Example: {{custom.contractor}}, {{data.extra_field}}, {{medical.local_note}}.
/// Invalid ids are rejected only to prevent broken placeholder parsing or path-like tricks.
pub fn is_valid_field_id(field_id: &str) -> bool {
    let trimmed = field_id.trim();
    if trimmed.is_empty()
        || trimmed.len() > 96
        || trimmed.starts_with('.')
        || trimmed.ends_with('.')
        || trimmed.contains("..")
    {
        return false;
    }
    trimmed.split('.').all(|part| {
        !part.is_empty()
            && part.len() <= 40
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            && part.chars().next().is_some_and(|c| c.is_alphabetic())
    })
}

pub fn title_for_field(field_id: &str) -> String {
    let canonical = crate::canonical_storage_field_id(field_id);
    if let Some(title) = crate::domains::medical_semantics::title_for_role_scoped_field(&canonical)
    {
        return title.to_string();
    }
    all_fields()
        .into_iter()
        .find(|field| field.id == canonical)
        .map(|field| field.title_ru)
        .unwrap_or_else(|| humanize_custom_field(&canonical))
}

pub fn aliases_for_field(field_id: &str) -> Vec<String> {
    let canonical = crate::canonical_storage_field_id(field_id);
    all_fields()
        .into_iter()
        .find(|field| field.id == canonical)
        .map(|field| field.aliases)
        .unwrap_or_default()
}

fn humanize_custom_field(field_id: &str) -> String {
    let last = field_id.rsplit('.').next().unwrap_or(field_id);
    let mut words = last.replace(['_', '-'], " ");
    if words.trim().is_empty() {
        return field_id.to_string();
    }
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => {
            words = first.to_uppercase().collect::<String>() + chars.as_str();
            format!("Пользовательское поле: {}", words)
        }
        None => field_id.to_string(),
    }
}

fn field(
    id: &str,
    title_ru: &str,
    domain: DomainKind,
    required_by_default: bool,
    aliases: &[&str],
) -> FieldDefinition {
    FieldDefinition {
        id: id.to_string(),
        title_ru: title_ru.to_string(),
        domain,
        required_by_default,
        aliases: aliases.iter().map(|x| x.to_string()).collect(),
    }
}

#[cfg(test)]
mod alias_tests {
    use super::*;

    #[test]
    fn human_and_camel_case_aliases_resolve_without_second_state() {
        assert_eq!(
            canonical_field_id("patientName").as_deref(),
            Some("subject.name")
        );
        assert_eq!(
            canonical_field_id("История болезни №").as_deref(),
            Some("medical.case_number")
        );
        assert_eq!(
            canonical_field_id("dischargeDate").as_deref(),
            Some("medical.discharge_date")
        );
        assert_eq!(
            canonical_field_id("Код МКБ-10").as_deref(),
            Some("medical.icd10")
        );
        assert_eq!(
            canonical_field_id("Дата анализов").as_deref(),
            Some("medical.labs_date")
        );
    }

    #[test]
    fn ambiguous_human_alias_uses_document_domain_instead_of_global_guess() {
        assert!(canonical_field_id("Должность").is_none());
        assert_eq!(
            canonical_field_id_for_domain("Должность", Some(&DomainKind::Hr)).as_deref(),
            Some("employee.position")
        );
        assert_eq!(
            canonical_field_id_for_domain("Должность", Some(&DomainKind::Medical)).as_deref(),
            Some("medical.position")
        );
    }

    #[test]
    fn v18_registry_fields_and_legacy_ids_resolve_to_one_canonical_id() {
        assert_eq!(
            canonical_field_id("medical.diagnosis_code").as_deref(),
            Some("medical.icd10")
        );
        assert_eq!(
            canonical_field_id("organization.name").as_deref(),
            Some("org.name")
        );
        assert_eq!(
            canonical_field_id("hr.employee_name").as_deref(),
            Some("employee.name")
        );
        assert_eq!(
            canonical_field_id("counterparty.name").as_deref(),
            Some("counterparty.name")
        );
        assert_eq!(title_for_field("org.name"), "Организация");
    }

    #[test]
    fn safe_custom_fields_remain_supported() {
        assert_eq!(
            canonical_field_id("custom.contractor").as_deref(),
            Some("custom.contractor")
        );
        assert!(canonical_field_id("../../secret").is_none());
    }
}
