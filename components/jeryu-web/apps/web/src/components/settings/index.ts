// settings/index.ts — barrel exports for the settings studio (W-FE-12).

export { SettingsLayout, SETTINGS_NAV } from './SettingsLayout';
export type {
  SettingsLayoutProps,
  SettingsNavItem,
} from './SettingsLayout';
export { SettingsSection } from './SettingsSection';
export type { SettingsSectionProps } from './SettingsSection';
export { SettingsDiffPreview } from './SettingsDiffPreview';
export type { SettingsDiffPreviewProps } from './SettingsDiffPreview';
export { BranchProtectionEditor } from './BranchProtectionEditor';
export type { BranchProtectionEditorProps } from './BranchProtectionEditor';
export { MergePolicyEditor } from './MergePolicyEditor';
export type { MergePolicyEditorProps } from './MergePolicyEditor';
export { AgentPolicyEditor } from './AgentPolicyEditor';
export type { AgentPolicyEditorProps } from './AgentPolicyEditor';
export { SecretsMetadataTable } from './SecretsMetadataTable';
export type {
  SecretsMetadataTableProps,
  SecretMetadata,
} from './SecretsMetadataTable';
