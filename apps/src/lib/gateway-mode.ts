export const GATEWAY_MODE_ENV_KEY = "CODEXMANAGER_GATEWAY_MODE";
export const DEFAULT_GATEWAY_MODE = "transparent";

export type GatewayMode = "transparent" | "enhanced";

export function normalizeGatewayMode(value: unknown): GatewayMode {
  return String(value || "").trim().toLowerCase() === "enhanced"
    ? "enhanced"
    : "transparent";
}

export function toGatewayModeOverride(mode: GatewayMode): string {
  return mode === DEFAULT_GATEWAY_MODE ? "" : mode;
}
