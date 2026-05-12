"use client";

import { useMemo } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  BadgeDollarSign,
  Boxes,
  CircleDollarSign,
  KeyRound,
  RefreshCw,
  Users,
} from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { quotaClient } from "@/lib/api/quota-client";
import { useI18n } from "@/lib/i18n/provider";
import { useAppStore } from "@/lib/store/useAppStore";
import { cn } from "@/lib/utils";
import { formatCompactNumber, formatTsFromSeconds } from "@/lib/utils/usage";
import type {
  QuotaApiKeyUsageItem,
  QuotaModelUsageItem,
  QuotaOverviewResult,
  QuotaSourceSummary,
} from "@/types/quota";

function formatTokens(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "-";
  return formatCompactNumber(Math.max(0, value), "0.00", 2, true);
}

function formatUsd(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "-";
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(Math.max(0, value));
}

function formatPercent(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "-";
  return `${Math.max(0, Math.min(100, value)).toFixed(0)}%`;
}

function metricValue(source: QuotaSourceSummary) {
  if (source.metricKind === "token_limit") {
    return `${formatTokens(source.remaining)} / ${formatTokens(source.total)}`;
  }
  if (source.metricKind === "money_balance") {
    const unit = source.unit || "USD";
    return unit.toUpperCase() === "USD"
      ? formatUsd(source.remaining)
      : `${source.remaining ?? "-"} ${unit}`;
  }
  if (source.metricKind === "window_percent") {
    return `${formatPercent(source.remaining)} 剩余`;
  }
  return source.remaining == null ? "-" : String(source.remaining);
}

function statusBadge(status: string, priceStatus?: string) {
  const normalized = (priceStatus || status || "unknown").toLowerCase();
  if (normalized === "ok" || normalized === "success" || normalized === "enabled") {
    return <Badge className="border-green-500/20 bg-green-500/10 text-green-600">OK</Badge>;
  }
  if (normalized === "missing" || normalized === "unknown") {
    return <Badge variant="secondary">未知</Badge>;
  }
  if (normalized === "warning") {
    return <Badge className="border-amber-500/20 bg-amber-500/10 text-amber-600">注意</Badge>;
  }
  return <Badge className="border-red-500/20 bg-red-500/10 text-red-600">异常</Badge>;
}

function OverviewCard({
  title,
  value,
  caption,
  icon: Icon,
}: {
  title: string;
  value: string;
  caption: string;
  icon: typeof KeyRound;
}) {
  return (
    <Card className="glass-card border-none shadow-md">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle className="text-sm font-medium">{title}</CardTitle>
        <Icon className="h-4 w-4 text-primary" />
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold">{value}</div>
        <p className="mt-1 text-[11px] text-muted-foreground">{caption}</p>
      </CardContent>
    </Card>
  );
}

function OverviewGrid({
  overview,
  loading,
}: {
  overview?: QuotaOverviewResult;
  loading: boolean;
}) {
  if (loading) {
    return (
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        {Array.from({ length: 4 }).map((_, index) => (
          <Skeleton key={index} className="h-28 rounded-xl" />
        ))}
      </div>
    );
  }

  return (
    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
      <OverviewCard
        title="API Key 剩余"
        value={formatTokens(overview?.apiKey.totalRemainingTokens)}
        caption={`${overview?.apiKey.limitedKeyCount ?? 0} 个限额 Key，已用 ${formatTokens(
          overview?.apiKey.totalUsedTokens,
        )}`}
        icon={KeyRound}
      />
      <OverviewCard
        title="上游余额"
        value={formatUsd(overview?.aggregateApi.totalBalanceUsd)}
        caption={`${overview?.aggregateApi.okCount ?? 0} 个正常，${
          overview?.aggregateApi.errorCount ?? 0
        } 个异常`}
        icon={CircleDollarSign}
      />
      <OverviewCard
        title="账号池剩余"
        value={`${formatPercent(overview?.openaiAccount.primaryRemainPercent)} / ${formatPercent(
          overview?.openaiAccount.secondaryRemainPercent,
        )}`}
        caption={`${overview?.openaiAccount.availableCount ?? 0} 个可用，${
          overview?.openaiAccount.lowQuotaCount ?? 0
        } 个低额度`}
        icon={Users}
      />
      <OverviewCard
        title="今日消耗"
        value={formatTokens(overview?.todayUsage.totalTokens)}
        caption={`${formatUsd(overview?.todayUsage.estimatedCostUsd)} 估算成本`}
        icon={BadgeDollarSign}
      />
    </div>
  );
}

function ModelUsageTable({ items }: { items: QuotaModelUsageItem[] }) {
  if (!items.length) {
    return <div className="py-10 text-center text-sm text-muted-foreground">暂无模型用量</div>;
  }
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>模型</TableHead>
          <TableHead>价格</TableHead>
          <TableHead className="text-right">总 token</TableHead>
          <TableHead className="text-right">输入 / 缓存 / 输出</TableHead>
          <TableHead className="text-right">成本</TableHead>
          <TableHead className="text-right">上游预计剩余</TableHead>
          <TableHead className="text-right">账号池</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {items.map((item) => (
          <TableRow key={item.model}>
            <TableCell>
              <div className="font-medium">{item.model}</div>
              <div className="text-xs text-muted-foreground">{item.provider || "-"}</div>
            </TableCell>
            <TableCell>{statusBadge(item.priceStatus, item.priceStatus)}</TableCell>
            <TableCell className="text-right font-mono">{formatTokens(item.totalTokens)}</TableCell>
            <TableCell className="text-right font-mono text-xs">
              {formatTokens(item.inputTokens)} / {formatTokens(item.cachedInputTokens)} /{" "}
              {formatTokens(item.outputTokens)}
            </TableCell>
            <TableCell className="text-right font-mono">{formatUsd(item.estimatedCostUsd)}</TableCell>
            <TableCell className="text-right font-mono">
              {formatTokens(item.aggregateEstimatedRemainingTokens)}
            </TableCell>
            <TableCell className="text-right text-xs">
              {item.openaiAvailableAccountCount} 个账号，5h{" "}
              {formatPercent(item.openaiPrimaryRemainPercent)}，7d{" "}
              {formatPercent(item.openaiSecondaryRemainPercent)}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function SourceTable({ items }: { items: QuotaSourceSummary[] }) {
  if (!items.length) {
    return <div className="py-10 text-center text-sm text-muted-foreground">暂无额度来源</div>;
  }
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>来源</TableHead>
          <TableHead>类型</TableHead>
          <TableHead>状态</TableHead>
          <TableHead className="text-right">额度</TableHead>
          <TableHead>模型</TableHead>
          <TableHead>刷新时间</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {items.map((item) => (
          <TableRow key={`${item.kind}-${item.id}`}>
            <TableCell>
              <div className="font-medium">{item.name || item.id}</div>
              <div className="max-w-[220px] truncate font-mono text-[10px] text-muted-foreground">
                {item.id}
              </div>
            </TableCell>
            <TableCell>{item.kind}</TableCell>
            <TableCell>
              <div className="flex items-center gap-2">
                {statusBadge(item.status)}
                {item.error ? (
                  <span className="max-w-[180px] truncate text-xs text-red-500">{item.error}</span>
                ) : null}
              </div>
            </TableCell>
            <TableCell className="text-right font-mono">{metricValue(item)}</TableCell>
            <TableCell className="max-w-[180px] truncate">
              {item.models.length ? item.models.join(", ") : "-"}
            </TableCell>
            <TableCell>{formatTsFromSeconds(item.capturedAt, "-")}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function ApiKeyUsageTable({ items }: { items: QuotaApiKeyUsageItem[] }) {
  if (!items.length) {
    return <div className="py-10 text-center text-sm text-muted-foreground">暂无 API Key 用量</div>;
  }
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Key</TableHead>
          <TableHead>模型</TableHead>
          <TableHead className="text-right">限额</TableHead>
          <TableHead className="text-right">已用</TableHead>
          <TableHead className="text-right">剩余</TableHead>
          <TableHead className="text-right">成本</TableHead>
          <TableHead>模型明细</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {items.map((item) => (
          <TableRow key={item.keyId}>
            <TableCell>
              <div className="font-medium">{item.name || item.keyId}</div>
              <div className="max-w-[180px] truncate font-mono text-[10px] text-muted-foreground">
                {item.keyId}
              </div>
            </TableCell>
            <TableCell>{item.modelSlug || "跟随请求"}</TableCell>
            <TableCell className="text-right font-mono">{formatTokens(item.quotaLimitTokens)}</TableCell>
            <TableCell className="text-right font-mono">{formatTokens(item.usedTokens)}</TableCell>
            <TableCell className="text-right font-mono">{formatTokens(item.remainingTokens)}</TableCell>
            <TableCell className="text-right font-mono">{formatUsd(item.estimatedCostUsd)}</TableCell>
            <TableCell>
              <details>
                <summary className="cursor-pointer text-xs text-primary">
                  {item.models.length} 个模型
                </summary>
                <div className="mt-2 grid gap-1 text-xs">
                  {item.models.map((model) => (
                    <div
                      key={`${item.keyId}-${model.model}`}
                      className="flex items-center justify-between gap-3 rounded bg-muted/50 px-2 py-1"
                    >
                      <span className="truncate">{model.model}</span>
                      <span className="font-mono">{formatTokens(model.totalTokens)}</span>
                    </div>
                  ))}
                </div>
              </details>
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

export default function QuotaPage() {
  const { t } = useI18n();
  const serviceAddr = useAppStore((state) => state.serviceStatus.addr);
  const isPageActive = useDesktopPageActive("/quota");
  const isDeferredActive = useDeferredDesktopActivation(isPageActive);
  usePageTransitionReady("/quota", isDeferredActive);
  const queryClient = useQueryClient();

  const enabled = Boolean(serviceAddr) && isDeferredActive;
  const overviewQuery = useQuery({
    queryKey: ["quota-overview", serviceAddr || null],
    queryFn: () => quotaClient.overview(),
    enabled,
  });
  const modelUsageQuery = useQuery({
    queryKey: ["quota-model-usage", serviceAddr || null],
    queryFn: () => quotaClient.modelUsage(),
    enabled,
  });
  const sourceQuery = useQuery({
    queryKey: ["quota-source-list", serviceAddr || null],
    queryFn: () => quotaClient.sourceList(),
    enabled,
  });
  const apiKeyUsageQuery = useQuery({
    queryKey: ["quota-api-key-usage", serviceAddr || null],
    queryFn: () => quotaClient.apiKeyUsage(),
    enabled,
  });

  const refreshMutation = useMutation({
    mutationFn: () => quotaClient.refreshSources({ kinds: ["aggregate_api", "openai_account"] }),
    onSuccess: (items) => {
      const failed = items.filter((item) => !item.ok).length;
      if (failed > 0) {
        toast.warning(`刷新完成，${failed} 个来源失败`);
      } else {
        toast.success("额度来源已刷新");
      }
      void queryClient.invalidateQueries({ queryKey: ["quota-overview"] });
      void queryClient.invalidateQueries({ queryKey: ["quota-source-list"] });
      void queryClient.invalidateQueries({ queryKey: ["quota-model-usage"] });
    },
    onError: (error) => {
      toast.error(`刷新额度失败: ${error instanceof Error ? error.message : String(error)}`);
    },
  });

  const modelItems = useMemo(
    () => [...(modelUsageQuery.data || [])].sort((a, b) => b.totalTokens - a.totalTokens),
    [modelUsageQuery.data],
  );
  const sourceItems = sourceQuery.data || [];
  const apiKeyItems = apiKeyUsageQuery.data || [];

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div className="glass-header flex shrink-0 items-center justify-between gap-3 border-b px-6 py-4">
        <div>
          <h1 className="text-xl font-semibold tracking-normal">{t("额度中心")}</h1>
          <p className="text-xs text-muted-foreground">
            {t("API Key、聚合 API 与 OpenAI 账号池额度")}
          </p>
        </div>
        <Button
          className="gap-2"
          disabled={!enabled || refreshMutation.isPending}
          onClick={() => refreshMutation.mutate()}
        >
          <RefreshCw
            className={cn("h-4 w-4", refreshMutation.isPending && "animate-spin")}
          />
          {t("刷新来源")}
        </Button>
      </div>

      <main className="flex-1 overflow-auto p-6">
        <div className="mx-auto flex w-full max-w-7xl flex-col gap-5">
          <OverviewGrid overview={overviewQuery.data} loading={overviewQuery.isPending} />

          <Tabs defaultValue="models" className="w-full">
            <TabsList className="w-fit">
              <TabsTrigger value="models">
                <Boxes className="h-4 w-4" />
                模型维度
              </TabsTrigger>
              <TabsTrigger value="sources">
                <CircleDollarSign className="h-4 w-4" />
                上游来源
              </TabsTrigger>
              <TabsTrigger value="keys">
                <KeyRound className="h-4 w-4" />
                API Key
              </TabsTrigger>
            </TabsList>
            <TabsContent value="models">
              <Card className="glass-card border-none shadow-md">
                <CardContent className="pt-1">
                  {modelUsageQuery.isPending ? (
                    <Skeleton className="h-72 rounded-lg" />
                  ) : (
                    <ModelUsageTable items={modelItems} />
                  )}
                </CardContent>
              </Card>
            </TabsContent>
            <TabsContent value="sources">
              <Card className="glass-card border-none shadow-md">
                <CardContent className="pt-1">
                  {sourceQuery.isPending ? (
                    <Skeleton className="h-72 rounded-lg" />
                  ) : (
                    <SourceTable items={sourceItems} />
                  )}
                </CardContent>
              </Card>
            </TabsContent>
            <TabsContent value="keys">
              <Card className="glass-card border-none shadow-md">
                <CardContent className="pt-1">
                  {apiKeyUsageQuery.isPending ? (
                    <Skeleton className="h-72 rounded-lg" />
                  ) : (
                    <ApiKeyUsageTable items={apiKeyItems} />
                  )}
                </CardContent>
              </Card>
            </TabsContent>
          </Tabs>
        </div>
      </main>
    </div>
  );
}
