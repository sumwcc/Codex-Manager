export interface QuotaOverviewResult {
  apiKey: {
    keyCount: number;
    limitedKeyCount: number;
    totalLimitTokens: number | null;
    totalUsedTokens: number;
    totalRemainingTokens: number | null;
    estimatedCostUsd: number;
  };
  aggregateApi: {
    sourceCount: number;
    enabledBalanceQueryCount: number;
    okCount: number;
    errorCount: number;
    totalBalanceUsd: number | null;
    lastRefreshedAt: number | null;
  };
  openaiAccount: {
    accountCount: number;
    availableCount: number;
    lowQuotaCount: number;
    primaryRemainPercent: number | null;
    secondaryRemainPercent: number | null;
    lastRefreshedAt: number | null;
  };
  todayUsage: {
    inputTokens: number;
    cachedInputTokens: number;
    outputTokens: number;
    reasoningOutputTokens: number;
    totalTokens: number;
    estimatedCostUsd: number;
  };
}

export interface QuotaModelUsageItem {
  model: string;
  provider: string | null;
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
  totalTokens: number;
  estimatedCostUsd: number | null;
  priceStatus: string;
  apiKeyRemainingTokens: number | null;
  aggregateEstimatedRemainingTokens: number | null;
  aggregateBalanceUsd: number | null;
  openaiAvailableAccountCount: number;
  openaiPrimaryRemainPercent: number | null;
  openaiSecondaryRemainPercent: number | null;
  openaiEstimatedRemainingTokens: number | null;
  openaiEstimateEnabled: boolean;
}

export interface QuotaApiKeyModelUsageItem {
  model: string;
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningOutputTokens: number;
  totalTokens: number;
  estimatedCostUsd: number | null;
  priceStatus: string;
}

export interface QuotaApiKeyUsageItem {
  keyId: string;
  name: string | null;
  modelSlug: string | null;
  quotaLimitTokens: number | null;
  usedTokens: number;
  remainingTokens: number | null;
  estimatedCostUsd: number;
  models: QuotaApiKeyModelUsageItem[];
}

export interface QuotaSourceSummary {
  id: string;
  kind: "api_key" | "aggregate_api" | "openai_account" | string;
  name: string;
  status: string;
  metricKind: "token_limit" | "money_balance" | "window_percent" | string;
  remaining: number | null;
  total: number | null;
  used: number | null;
  unit: string | null;
  models: string[];
  provider: string | null;
  capturedAt: number | null;
  error: string | null;
}

export interface QuotaRefreshSourcesParams {
  kinds?: Array<"aggregate_api" | "openai_account">;
  sourceIds?: string[];
}

export interface QuotaRefreshSourceResult {
  id: string;
  kind: string;
  ok: boolean;
  error: string | null;
}
