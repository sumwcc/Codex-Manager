"use client";

import { useSyncExternalStore } from "react";
import { ShieldAlert, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

const DISCLAIMER_DISMISSED_KEY = "codexmanager.disclaimer.dismissed";
const disclaimerListeners = new Set<() => void>();

const DISCLAIMER_ITEMS = [
  "本项目仅用于学习与开发目的。",
  "使用者必须遵守相关平台的服务条款，例如 OpenAI、Anthropic。",
  "作者不提供或分发任何账号、API Key 或代理服务，也不对本软件的具体使用方式负责。",
  "请勿使用本项目绕过速率限制或服务限制。",
] as const;

function subscribeDisclaimer(listener: () => void) {
  disclaimerListeners.add(listener);
  if (typeof window !== "undefined") {
    const handleStorage = (event: StorageEvent) => {
      if (event.key === DISCLAIMER_DISMISSED_KEY) {
        listener();
      }
    };
    window.addEventListener("storage", handleStorage);
    return () => {
      disclaimerListeners.delete(listener);
      window.removeEventListener("storage", handleStorage);
    };
  }
  return () => {
    disclaimerListeners.delete(listener);
  };
}

function getDisclaimerDismissedSnapshot() {
  if (typeof window === "undefined") {
    return true;
  }
  return window.localStorage.getItem(DISCLAIMER_DISMISSED_KEY) === "1";
}

function getDisclaimerDismissedServerSnapshot() {
  return true;
}

function setDisclaimerDismissed(dismissed: boolean) {
  if (typeof window !== "undefined") {
    if (dismissed) {
      window.localStorage.setItem(DISCLAIMER_DISMISSED_KEY, "1");
    } else {
      window.localStorage.removeItem(DISCLAIMER_DISMISSED_KEY);
    }
  }
  for (const listener of disclaimerListeners) {
    listener();
  }
}

export function DisclaimerBanner() {
  const dismissed = useSyncExternalStore(
    subscribeDisclaimer,
    getDisclaimerDismissedSnapshot,
    getDisclaimerDismissedServerSnapshot
  );

  if (dismissed) {
    return null;
  }

  return (
    <Card className="mb-5 border-amber-500/25 bg-amber-500/8 shadow-sm backdrop-blur-md">
      <CardHeader className="flex flex-row items-start justify-between gap-3 space-y-0 pb-3">
        <div className="flex min-w-0 items-center gap-2">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-2xl bg-amber-500/15 text-amber-700 dark:text-amber-300">
            <ShieldAlert className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <CardTitle className="text-sm font-semibold">免责声明</CardTitle>
            <p className="mt-1 text-xs text-muted-foreground">
              这段内容来自 README，适合作为首次使用时的统一提示。
            </p>
          </div>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8 shrink-0 text-muted-foreground hover:text-foreground"
          onClick={() => setDisclaimerDismissed(true)}
          title="关闭免责声明"
        >
          <X className="h-4 w-4" />
        </Button>
      </CardHeader>
      <CardContent className="pt-0">
        <ul className="space-y-1.5 pl-5 text-xs leading-5 text-muted-foreground">
          {DISCLAIMER_ITEMS.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
