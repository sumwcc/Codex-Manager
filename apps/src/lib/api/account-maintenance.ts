function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readStringField(payload: unknown, key: string, fallback = ""): string {
  const source = asRecord(payload);
  const value = source?.[key];
  return typeof value === "string" ? value.trim() : fallback;
}

function readBooleanField(payload: unknown, key: string, fallback = false): boolean {
  const source = asRecord(payload);
  const value = source?.[key];
  return typeof value === "boolean" ? value : fallback;
}

function readNumberField(payload: unknown, key: string, fallback = 0): number {
  const source = asRecord(payload);
  const value = source?.[key];
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return fallback;
}

function readStringArrayField(payload: unknown, key: string): string[] {
  const source = asRecord(payload);
  const value = source?.[key];
  return Array.isArray(value)
    ? value
        .map((item) => (typeof item === "string" ? item.trim() : ""))
        .filter(Boolean)
    : [];
}

export interface AccountImportError {
  index: number;
  message: string;
}

export interface AccountImportResult {
  canceled?: boolean;
  total?: number;
  created?: number;
  updated?: number;
  failed?: number;
  errors?: AccountImportError[];
  fileCount?: number;
  directoryPath?: string;
  contents?: string[];
}

export interface AccountExportResult {
  canceled?: boolean;
  exported?: number;
  outputDir?: string;
}

export interface DeleteUnavailableFreeResult {
  deleted?: number;
}

export interface DeleteAccountsByStatusesResult {
  scanned?: number;
  deleted?: number;
  skippedStatus?: number;
  targetStatuses?: string[];
  deletedAccountIds?: string[];
}

export interface AccountWarmupItemResult {
  accountId: string;
  accountName: string;
  status: string;
  ok: boolean;
  message: string;
  startedAt: number | null;
  finishedAt: number | null;
}

export interface AccountWarmupResult {
  batchId?: string | null;
  status?: string;
  total?: number;
  requested?: number;
  processed?: number;
  succeeded?: number;
  failed?: number;
  skipped?: number;
  startedAt?: number | null;
  finishedAt?: number | null;
  results?: AccountWarmupItemResult[];
}

export function readAccountImportResult(payload: unknown): AccountImportResult {
  const source = asRecord(payload);
  const errors = Array.isArray(source?.errors)
    ? source.errors
        .map((item) => {
          const entry = asRecord(item);
          if (!entry) {
            return null;
          }
          return {
            index: readNumberField(entry, "index"),
            message: readStringField(entry, "message"),
          };
        })
        .filter((item): item is AccountImportError => Boolean(item))
    : [];

  return {
    canceled: readBooleanField(payload, "canceled"),
    total: readNumberField(payload, "total"),
    created: readNumberField(payload, "created"),
    updated: readNumberField(payload, "updated"),
    failed: readNumberField(payload, "failed"),
    errors,
    fileCount: readNumberField(payload, "fileCount"),
    directoryPath: readStringField(payload, "directoryPath"),
    contents: readStringArrayField(payload, "contents"),
  };
}

export function readAccountExportResult(payload: unknown): AccountExportResult {
  return {
    canceled: readBooleanField(payload, "canceled"),
    exported: readNumberField(payload, "exported"),
    outputDir: readStringField(payload, "outputDir"),
  };
}

export function readDeleteUnavailableFreeResult(
  payload: unknown
): DeleteUnavailableFreeResult {
  return {
    deleted: readNumberField(payload, "deleted"),
  };
}

export function readDeleteAccountsByStatusesResult(
  payload: unknown
): DeleteAccountsByStatusesResult {
  return {
    scanned: readNumberField(payload, "scanned"),
    deleted: readNumberField(payload, "deleted"),
    skippedStatus: readNumberField(payload, "skippedStatus"),
    targetStatuses: readStringArrayField(payload, "targetStatuses"),
    deletedAccountIds: readStringArrayField(payload, "deletedAccountIds"),
  };
}

export function readAccountWarmupResult(payload: unknown): AccountWarmupResult {
  const source = asRecord(payload);
  const results = Array.isArray(source?.results)
    ? source.results
        .map((item) => {
          const entry = asRecord(item);
          if (!entry) {
            return null;
          }
          return {
            accountId: readStringField(entry, "accountId"),
            accountName: readStringField(entry, "accountName"),
            status: readStringField(entry, "status"),
            ok: readBooleanField(entry, "ok"),
            message: readStringField(entry, "message"),
            startedAt: readNumberField(entry, "startedAt", 0) || null,
            finishedAt: readNumberField(entry, "finishedAt", 0) || null,
          };
        })
        .filter((item): item is AccountWarmupItemResult => Boolean(item))
    : [];

  return {
    batchId: readStringField(payload, "batchId") || null,
    status: readStringField(payload, "status"),
    total: readNumberField(payload, "total"),
    requested: readNumberField(payload, "requested"),
    processed: readNumberField(payload, "processed"),
    succeeded: readNumberField(payload, "succeeded"),
    failed: readNumberField(payload, "failed"),
    skipped: readNumberField(payload, "skipped"),
    startedAt: readNumberField(payload, "startedAt", 0) || null,
    finishedAt: readNumberField(payload, "finishedAt", 0) || null,
    results,
  };
}

export function readApiKeySecret(payload: unknown): string {
  return readStringField(payload, "key");
}
