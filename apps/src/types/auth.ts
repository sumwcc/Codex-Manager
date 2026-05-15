export interface LoginStatusResult {
  status: string;
  error: string;
}

export interface DeviceAuthInfo {
  userCodeUrl: string;
  tokenUrl: string;
  verificationUrl: string;
  redirectUri: string;
}

export interface LoginStartResult {
  type: string;
  authUrl?: string | null;
  loginId: string;
  verificationUrl?: string | null;
  userCode?: string | null;
}

export interface CurrentAccessTokenAccount {
  type: string;
  accountId: string;
  email: string;
  planType: string;
  planTypeRaw?: string | null;
  hasSubscription?: boolean | null;
  subscriptionPlan?: string | null;
  subscriptionExpiresAt?: number | null;
  subscriptionRenewsAt?: number | null;
  chatgptAccountId: string | null;
  workspaceId: string | null;
  status: string;
}

export interface CurrentAccessTokenAccountReadResult {
  account: CurrentAccessTokenAccount | null;
  requiresOpenaiAuth: boolean;
}

export interface TokenRefreshOutcome {
  accessTokenChanged: boolean;
  refreshTokenReturned: boolean;
  refreshTokenChanged: boolean;
  idTokenChanged: boolean;
  accessTokenExpiresAt: number | null;
  refreshTokenExpiresAt: number | null;
  nextRefreshAt: number | null;
}

export interface ChatgptAuthTokensRefreshResult extends TokenRefreshOutcome {
  accessToken: string;
  chatgptAccountId: string;
  chatgptPlanType: string | null;
  hasSubscription?: boolean | null;
  subscriptionPlan?: string | null;
  subscriptionExpiresAt?: number | null;
  subscriptionRenewsAt?: number | null;
}

export interface ChatgptAuthTokensRefreshAllItem extends TokenRefreshOutcome {
  accountId: string;
  accountName: string;
  status: "pending" | "running" | "success" | "failed" | "skipped" | string;
  ok: boolean;
  message: string | null;
  startedAt: number | null;
  finishedAt: number | null;
}

export interface ChatgptAuthTokensRefreshAllResult {
  batchId: string | null;
  status: "running" | "completed" | "failed" | string;
  total: number;
  requested: number;
  processed: number;
  succeeded: number;
  failed: number;
  skipped: number;
  refreshTokenReturned: number;
  refreshTokenChanged: number;
  refreshTokenMissing: number;
  startedAt: number | null;
  finishedAt: number | null;
  results: ChatgptAuthTokensRefreshAllItem[];
}
