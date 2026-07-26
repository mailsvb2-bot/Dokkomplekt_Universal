export interface StarterTemplateAsset {
  documentId: string;
  label: string;
  fileName: string;
  url: string;
  sha256: string;
}

export interface StarterPackAsset {
  id: string;
  name: string;
  description: string;
  usageMode: 'draft_only';
  templates: StarterTemplateAsset[];
}

export const STARTER_PACKS: StarterPackAsset[] = [
  {
    "id": "tier1.accounting.ru",
    "name": "Бухгалтерия",
    "description": "Пакет содержит работающие draft-only starter-шаблоны с каноническими полями. Перед пилотом каждая форма должна быть заменена или утверждена уполномоченным специалистом организации.",
    "usageMode": "draft_only",
    "templates": [
      {
        "documentId": "accounting.invoice",
        "label": "Счёт",
        "fileName": "invoice.docx",
        "url": "/starter-packs/tier1-accounting-ru/templates/invoice.docx",
        "sha256": "1ef3ea72094fa5550a5f9616e7eb32df059a34958d0f37673eaef89d511ba19e"
      },
      {
        "documentId": "accounting.service_act",
        "label": "Акт оказанных услуг",
        "fileName": "service_act.docx",
        "url": "/starter-packs/tier1-accounting-ru/templates/service_act.docx",
        "sha256": "3a681f549aad7c928e226dda3931b7a7a5d76c3adad005ec4c4e515223cc442e"
      },
      {
        "documentId": "accounting.reconciliation",
        "label": "Акт сверки",
        "fileName": "reconciliation.docx",
        "url": "/starter-packs/tier1-accounting-ru/templates/reconciliation.docx",
        "sha256": "aa0dc7690e260f1c348818001db98bc07630c181cdd607082eb8fae818aca0d5"
      }
    ]
  },
  {
    "id": "tier1.hr.ru",
    "name": "Кадры",
    "description": "Пакет содержит работающие draft-only starter-шаблоны с каноническими полями. Перед пилотом каждая форма должна быть заменена или утверждена уполномоченным специалистом организации.",
    "usageMode": "draft_only",
    "templates": [
      {
        "documentId": "hr.employment_contract",
        "label": "Трудовой договор",
        "fileName": "employment_contract.docx",
        "url": "/starter-packs/tier1-hr-ru/templates/employment_contract.docx",
        "sha256": "c88071f4eec9dc5ea2d8da00d6ecedc5c597ae220808960afd8da600d1ded00d"
      },
      {
        "documentId": "hr.employment_order",
        "label": "Приказ о приёме",
        "fileName": "employment_order.docx",
        "url": "/starter-packs/tier1-hr-ru/templates/employment_order.docx",
        "sha256": "18cf5b1d556d1e85677e900ed3d2f0c376eed138331f13541c05032bbb9e1628"
      },
      {
        "documentId": "hr.personal_data_consent",
        "label": "Согласие на обработку данных",
        "fileName": "personal_data_consent.docx",
        "url": "/starter-packs/tier1-hr-ru/templates/personal_data_consent.docx",
        "sha256": "db9d055ae23dc84ea6b4969fdf3bfc5c978fe6882fe7e06225c6212f9c25b9a5"
      },
      {
        "documentId": "hr.familiarization_sheet",
        "label": "Лист ознакомления",
        "fileName": "familiarization_sheet.docx",
        "url": "/starter-packs/tier1-hr-ru/templates/familiarization_sheet.docx",
        "sha256": "9cfb1737c9928c0d7b217c3f2b4b0f63a1279650137c048a27fc884764ffd8a2"
      }
    ]
  },
  {
    "id": "tier1.legal.ru",
    "name": "Право",
    "description": "Пакет содержит работающие draft-only starter-шаблоны с каноническими полями. Перед пилотом каждая форма должна быть заменена или утверждена уполномоченным специалистом организации.",
    "usageMode": "draft_only",
    "templates": [
      {
        "documentId": "legal.contract",
        "label": "Договор",
        "fileName": "contract.docx",
        "url": "/starter-packs/tier1-legal-ru/templates/contract.docx",
        "sha256": "cb3ace13dcd1d3ec54f4399d2bdf53b045b94fda817cee97943d97d9020dbe50"
      },
      {
        "documentId": "legal.acceptance_act",
        "label": "Акт",
        "fileName": "acceptance_act.docx",
        "url": "/starter-packs/tier1-legal-ru/templates/acceptance_act.docx",
        "sha256": "5c7fa678f2ed215214376b4c389a7eee058019e28dfa2889bcae8e5f59dc8682"
      },
      {
        "documentId": "legal.claim",
        "label": "Претензия",
        "fileName": "claim.docx",
        "url": "/starter-packs/tier1-legal-ru/templates/claim.docx",
        "sha256": "0b2d93f8f133b245aa60752b73f8ba1ec5a6b3516bb10987cd58d4edc3592a37"
      },
      {
        "documentId": "legal.cover_letter",
        "label": "Сопроводительное письмо",
        "fileName": "cover_letter.docx",
        "url": "/starter-packs/tier1-legal-ru/templates/cover_letter.docx",
        "sha256": "0c146de27b9c8ed0c71be8998a5b31f5788f589f7372f1a343f5bc79ce87d2c2"
      }
    ]
  }
] as StarterPackAsset[];
