import { expect, test, type Page } from "@playwright/test";

const SETTINGS_SNAPSHOT = {
  updateAutoCheck: true,
  closeToTrayOnClose: false,
  closeToTraySupported: false,
  lowTransparency: false,
  lightweightModeOnCloseToTray: false,
  codexCliGuideDismissed: true,
  webAccessPasswordConfigured: false,
  locale: "zh-CN",
  localeOptions: ["zh-CN", "en"],
  serviceAddr: "localhost:48760",
  serviceListenMode: "loopback",
  serviceListenModeOptions: ["loopback", "all_interfaces"],
  routeStrategy: "ordered",
  routeStrategyOptions: ["ordered", "balanced"],
  freeAccountMaxModel: "auto",
  freeAccountMaxModelOptions: ["auto", "gpt-5"],
  modelForwardRules: "",
  accountMaxInflight: 1,
  gatewayOriginator: "codex-cli",
  gatewayOriginatorDefault: "codex-cli",
  gatewayUserAgentVersion: "1.0.0",
  gatewayUserAgentVersionDefault: "1.0.0",
  gatewayResidencyRequirement: "",
  gatewayResidencyRequirementOptions: ["", "us"],
  pluginMarketMode: "builtin",
  pluginMarketSourceUrl: "",
  upstreamProxyUrl: "",
  upstreamStreamTimeoutMs: 600000,
  upstreamTotalTimeoutMs: 0,
  sseKeepaliveIntervalMs: 15000,
  backgroundTasks: {
    usagePollingEnabled: true,
    usagePollIntervalSecs: 600,
    gatewayKeepaliveEnabled: true,
    gatewayKeepaliveIntervalSecs: 180,
    tokenRefreshPollingEnabled: true,
    tokenRefreshPollIntervalSecs: 60,
    usageRefreshWorkers: 4,
    httpWorkerFactor: 4,
    httpWorkerMin: 8,
    httpStreamWorkerFactor: 1,
    httpStreamWorkerMin: 2,
  },
  envOverrides: {},
  envOverrideCatalog: [],
  envOverrideReservedKeys: [],
  envOverrideUnsupportedKeys: [],
  theme: "tech",
  appearancePreset: "classic",
};

async function mockRuntimeAndRpc(page: Page) {
  await page.route("**/api/runtime", async (route) => {
    await route.fulfill({
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        mode: "web-gateway",
        rpcBaseUrl: "/api/rpc",
        canManageService: false,
        canSelfUpdate: false,
        canCloseToTray: false,
        canOpenLocalDir: false,
        canUseBrowserFileImport: true,
        canUseBrowserDownloadExport: true,
      }),
    });
  });

  await page.route("**/api/rpc", async (route) => {
    const payload = route.request().postDataJSON();
    const method = typeof payload?.method === "string" ? payload.method : "";
    const id = payload?.id ?? 1;

    const resultByMethod = {
      "appSettings/get": SETTINGS_SNAPSHOT,
      initialize: {
        userAgent: "codex_cli_rs/0.3.0",
        codexHome: "C:/Users/Test/.codex",
        platformFamily: "windows",
        platformOs: "windows",
      },
      "aggregateApi/list": { items: [] },
      "gateway/concurrencyRecommendation/get": {
        usageRefreshWorkers: 4,
        httpWorkerFactor: 4,
        httpWorkerMin: 8,
        httpStreamWorkerFactor: 1,
        httpStreamWorkerMin: 2,
        accountMaxInflight: 1,
      },
    } satisfies Record<string, unknown>;

    if (!(method in resultByMethod)) {
      await route.fulfill({
        status: 500,
        contentType: "application/json; charset=utf-8",
        body: JSON.stringify({
          jsonrpc: "2.0",
          id,
          error: {
            code: -32000,
            message: `Unhandled RPC method in test: ${method}`,
          },
        }),
      });
      return;
    }

    await route.fulfill({
      contentType: "application/json; charset=utf-8",
      body: JSON.stringify({
        jsonrpc: "2.0",
        id,
        result: resultByMethod[method],
      }),
    });
  });
}

test("author page splits sponsor content and contact content into two tabs", async ({
  page,
}) => {
  await mockRuntimeAndRpc(page);

  await page.goto("/author/");

  await expect(
    page.getByRole("heading", { name: "赞助与推荐", level: 2 }),
  ).toBeVisible();
  await expect(page.getByRole("tab", { name: "赞助 / 推荐" })).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "星思研中转站" }),
  ).toBeVisible();
  await expect(page.getByRole("heading", { name: "RackNerd" })).toBeVisible();

  await page.getByRole("tab", { name: "联系作者" }).click();

  await expect(page.getByRole("heading", { name: "联系作者" })).toBeVisible();
  await expect(page.getByText("ProsperGao", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "加入 TG 群聊" })).toBeVisible();
});
