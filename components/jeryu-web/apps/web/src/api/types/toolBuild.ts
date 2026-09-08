export interface EcosystemToolAsset {
  name: string;
  className: string;
  conformance: string;
  sideEffects: string[];
  dataClasses: string[];
  repo?: string;
  provider?: string;
  health?: string;
  dependsOn: string[];
  queue?: string;
}

export interface EcosystemResponse {
  tools: EcosystemToolAsset[];
  live: boolean;
  degradedReason: string;
}

export interface ToolBuildOccurrence {
  repo_id: string;
  commit_sha: string;
  path: string;
  start_line: number;
  end_line: number;
  language: string;
  normalized_token_count: number;
}

export interface ToolBuildIgnore {
  cluster_id: string;
  reason: string;
  ignored_by: string;
  ignored_at: string;
}

export interface ToolBuildCluster {
  cluster_id: string;
  repo_id: string;
  commit_sha: string;
  fingerprint: string;
  score: number;
  occurrence_count: number;
  repo_count: number;
  file_count: number;
  total_lines: number;
  language: string;
  insight: string;
  normalized_preview: string;
  occurrences: ToolBuildOccurrence[];
  ignored?: ToolBuildIgnore;
}

export interface ToolBuildClustersResponse {
  schema_version: string;
  repo: string | null;
  include_ignored: boolean;
  clusters: ToolBuildCluster[];
}
