export interface MedicalProfileQuickOptionPreset {
  id: string;
  title: string;
  rvkCommissariats: string[];
}

/** Professional data presets. Universal Core must never contain these local names. */
export const MEDICAL_PROFILE_QUICK_OPTION_PRESETS: MedicalProfileQuickOptionPreset[] = [
  {
    id: 'nizhny-novgorod-legacy-rvk',
    title: 'Нижний Новгород · прежний Dokkomplekt',
    rvkCommissariats: ['Ленинский', 'Канавинский', 'Сормовский и Московский'],
  },
];
