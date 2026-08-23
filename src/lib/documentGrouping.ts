import type { DocumentTemplateSpec, DomainKind } from './types';

export interface DocumentDomainGroup {
  key: string;
  title: string;
  documents: DocumentTemplateSpec[];
}

export function groupDocumentsByDomain(documents: DocumentTemplateSpec[]): DocumentDomainGroup[] {
  const groups = new Map<string, DocumentDomainGroup>();
  for (const document of documents) {
    const key = domainKey(document.category);
    const existing = groups.get(key);
    if (existing) existing.documents.push(document);
    else groups.set(key, { key, title: domainTitle(document.category), documents: [document] });
  }
  return [...groups.values()].sort((a, b) => a.title.localeCompare(b.title, 'ru'));
}

function domainKey(domain: DomainKind): string {
  if (typeof domain === 'object' && 'Custom' in domain) return `custom-${domain.Custom.trim().toLowerCase() || 'profile'}`;
  return String(domain).toLowerCase();
}

function domainTitle(domain: DomainKind): string {
  if (typeof domain === 'object' && 'Custom' in domain) return domain.Custom.trim() || 'Свой профиль';
  return ({ Medical: 'Медицина', Legal: 'Юридическая работа', Hr: 'Кадры', Accounting: 'Бухгалтерия', Education: 'Образование', Generic: 'Общие документы' } as Record<string, string>)[domain] ?? String(domain);
}
