"use client";

import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { appClient } from "@/lib/api/app-client";
import { useI18n } from "@/lib/i18n/provider";
import {
  ExternalLink,
  HeartHandshake,
  Info,
  Send,
  Server,
  Sparkles,
} from "lucide-react";
import { toast } from "sonner";

const AUTHOR_WECHAT_ID = "ProsperGao";
const AUTHOR_TELEGRAM_GROUP_URL = "https://t.me/+OdpFa9GvjxhjMDhl";
const AUTHOR_SUPPORT_IMAGES = [
  {
    key: "alipay",
    title: "支付宝赞助码",
    description: "如果这个项目帮你省了时间，可以请作者喝杯咖啡。",
    src: "/author-alipay.jpg",
  },
  {
    key: "wechat-pay",
    title: "微信赞助码",
    description: "项目持续维护、修问题和做适配，欢迎随缘支持。",
    src: "/author-wechat-pay.jpg",
  },
] as const;

const README_SPONSORS = [
  {
    key: "visioncoder",
    name: "VisionCoder",
    description:
      "VisionCoder 是一款高颜值、可灵活切换模型的桌面 AI 编程工具。它支持 Claude、Gemini、GPT，并集成 Claude Code、Gemini CLI、Codex、OpenCode 等多种 CLI 能力。",
    href: "https://coder.visioncoder.cn",
    imageSrc: "https://coder.visioncoder.cn/logo.png",
    imageAlt: "VisionCoder",
    actionLabel: "访问官网",
  },
  {
    key: "xingsiyan",
    name: "星思研中转站",
    description:
      "星思研中转站为 Claude Code、Codex、Gemini 等模型调用场景提供稳定中转与配套服务，适合需要高可用接口、便捷接入和持续交付支持的开发者与团队。",
    href: "https://gzxsy.vip/register?aff=eapz",
    imageSrc: "/sponsors/xingsiyan.jpg",
    imageAlt: "星思研中转站",
    actionLabel: "立即注册",
  },
] as const;

const SERVER_RECOMMENDATIONS = [
  {
    key: "racknerd",
    name: "RackNerd",
    description:
      "适合部署 CodexManager、网关转发服务和常规开发环境的 VPS 选择，适合需要稳定海外节点和可控成本的个人开发者或小团队。",
    href: "https://my.racknerd.com/aff.php?aff=19058",
    imageSrc: "https://racknerd.com/banners/125x125.gif",
    imageAlt: "RackNerd Square Banner",
    actionLabel: "查看套餐",
  },
] as const;

type PartnerRow = {
  key: string;
  name: string;
  description: string;
  href: string;
  actionLabel: string;
  imageSrc?: string;
  imageAlt?: string;
};

function PartnerTable({
  items,
  onOpenLink,
  translate,
  emptyVisualLabel,
}: {
  items: readonly PartnerRow[];
  onOpenLink: (url: string) => Promise<void>;
  translate: (message: string) => string;
  emptyVisualLabel: string;
}) {
  return (
    <div className="overflow-x-auto rounded-3xl border border-border/50 bg-background/40">
      <table className="min-w-full border-collapse">
        <tbody>
          {items.map((item, index) => (
            <tr
              key={item.key}
              className={index === 0 ? "" : "border-t border-border/50"}
            >
              <td className="w-[180px] p-5 align-middle">
                <div className="flex items-center justify-center rounded-3xl border border-border/50 bg-white/95 p-4">
                  {item.imageSrc ? (
                    <img
                      src={item.imageSrc}
                      alt={translate(item.imageAlt ?? item.name)}
                      className="max-h-20 w-auto object-contain"
                    />
                  ) : (
                    <div className="flex h-20 w-full max-w-[180px] items-center justify-center rounded-2xl bg-gradient-to-br from-primary/15 via-background to-primary/5 px-4 text-center">
                      <span className="text-lg font-semibold tracking-tight text-foreground">
                        {translate(emptyVisualLabel)}
                      </span>
                    </div>
                  )}
                </div>
              </td>
              <td className="p-5 align-middle">
                <div className="space-y-3">
                  <div className="space-y-1">
                    <h3 className="text-base font-semibold text-foreground">
                      {translate(item.name)}
                    </h3>
                    <p className="text-sm leading-7 text-muted-foreground">
                      {translate(item.description)}
                    </p>
                  </div>
                  <div className="flex flex-wrap items-center gap-3">
                    <button
                      type="button"
                      onClick={() => {
                        void onOpenLink(item.href);
                      }}
                      className="inline-flex items-center gap-2 rounded-full border border-primary/20 bg-primary/10 px-4 py-2 text-sm font-medium text-primary transition-all duration-200 hover:bg-primary/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                    >
                      {translate(item.actionLabel)}
                      <ExternalLink className="h-4 w-4" />
                    </button>
                  </div>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default function AuthorPage() {
  const { t } = useI18n();

  const handleOpenLink = async (url: string) => {
    try {
      await appClient.openInBrowser(url);
    } catch (error) {
      toast.error(
        t("打开链接失败：{message}", {
          message: error instanceof Error ? error.message : t("未知错误"),
        }),
      );
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2 text-primary">
          <Sparkles className="h-4 w-4" />
          <span className="text-xs font-medium uppercase tracking-[0.24em]">
            {t("赞助与推荐")}
          </span>
        </div>
        <div className="space-y-2">
          <h2 className="text-xl font-bold tracking-tight">{t("赞助与推荐")}</h2>
          <p className="text-sm leading-6 text-muted-foreground">
            {t("这里集中展示 README 里的赞助信息、推荐服务，以及作者联系入口。")}
          </p>
        </div>
      </div>

      <Tabs defaultValue="sponsor">
        <TabsList className="glass-card flex h-11 w-full justify-start overflow-x-auto rounded-xl border-none p-1 no-scrollbar lg:w-fit">
          <TabsTrigger value="sponsor" className="gap-2 px-5 shrink-0">
            {t("赞助 / 推荐")}
          </TabsTrigger>
          <TabsTrigger value="contact" className="gap-2 px-5 shrink-0">
            {t("联系作者")}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="sponsor" className="space-y-6">
          <Card className="glass-card border-none shadow-md">
            <CardHeader className="gap-3">
              <div className="flex items-center gap-2">
                <HeartHandshake className="h-4 w-4 text-primary" />
                <CardTitle className="text-base">{t("赞助商")}</CardTitle>
              </div>
              <CardDescription>
                {t("沿用 README 的展示内容，并同步星思研邀请链接。")}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <PartnerTable
                items={README_SPONSORS}
                onOpenLink={handleOpenLink}
                translate={t}
                emptyVisualLabel="Sponsor"
              />
            </CardContent>
          </Card>

          <Card className="glass-card border-none shadow-md">
            <CardHeader className="gap-3">
              <div className="flex items-center gap-2">
                <Server className="h-4 w-4 text-primary" />
                <CardTitle className="text-base">{t("服务器推荐")}</CardTitle>
              </div>
              <CardDescription>
                {t("补充一个常用服务器选择，便于直接部署或长期运行服务。")}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <PartnerTable
                items={SERVER_RECOMMENDATIONS}
                onOpenLink={handleOpenLink}
                translate={t}
                emptyVisualLabel="RackNerd"
              />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="contact" className="space-y-6">
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-primary">
              <Info className="h-4 w-4" />
              <span className="text-xs font-medium uppercase tracking-[0.24em]">
                {t("联系作者")}
              </span>
            </div>
            <h3 className="text-lg font-semibold tracking-tight">
              {t("联系作者")}
            </h3>
          </div>

          <Card className="glass-card border-none shadow-md">
            <CardHeader className="gap-3">
                <div className="flex items-center justify-between gap-3">
                  <div className="flex items-center gap-2">
                    <HeartHandshake className="h-4 w-4 text-primary" />
                    <CardTitle className="text-base">{t("赞助支持")}</CardTitle>
                  </div>
                <Badge variant="secondary">{t("支持")}</Badge>
              </div>
            </CardHeader>
            <CardContent className="grid gap-4 md:grid-cols-2">
              {AUTHOR_SUPPORT_IMAGES.map((item) => (
                <div
                  key={item.key}
                  className="rounded-3xl border border-border/50 bg-background/40 p-5"
                >
                  <div className="space-y-1">
                    <h3 className="text-sm font-semibold text-foreground">
                      {t(item.title)}
                    </h3>
                    <p className="text-xs leading-6 text-muted-foreground">
                      {t(item.description)}
                    </p>
                  </div>
                  <div className="mt-4 overflow-hidden rounded-3xl border border-border/50 bg-white p-3">
                    <img
                      src={item.src}
                      alt={item.title}
                      className="mx-auto aspect-square w-full max-w-[220px] rounded-2xl object-cover"
                    />
                  </div>
                </div>
              ))}
            </CardContent>
          </Card>

          <Card className="glass-card border-none shadow-md">
            <CardHeader className="gap-3">
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                  <Send className="h-4 w-4 text-primary" />
                  <CardTitle className="text-base">{t("联系方式")}</CardTitle>
                </div>
                <Badge variant="secondary">{t("持续维护中")}</Badge>
              </div>
              <CardDescription>
                {t("需要反馈问题或进一步沟通时，可以通过微信或 TG 群联系作者。")}
              </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 md:grid-cols-2">
              <div className="rounded-3xl border border-border/50 bg-background/40 p-5">
                <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
                  {t("微信")}
                </p>
                <p className="mt-3 text-2xl font-semibold tracking-tight text-foreground">
                  {AUTHOR_WECHAT_ID}
                </p>
                <p className="mt-3 text-xs leading-6 text-muted-foreground">
                  {t("扫码可直接添加作者微信，也可以手动搜索上面的微信号。")}
                </p>
                <div className="mt-4 overflow-hidden rounded-3xl border border-border/50 bg-white p-3">
                  <img
                    src="/author-wechat.jpg"
                    alt="作者微信二维码"
                    className="mx-auto aspect-square w-full max-w-[180px] rounded-2xl object-cover"
                  />
                </div>
              </div>

              <div className="rounded-3xl border border-border/50 bg-background/40 p-5">
                <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
                  Telegram
                </p>
                <button
                  type="button"
                  onClick={() => {
                    void handleOpenLink(AUTHOR_TELEGRAM_GROUP_URL);
                  }}
                  className="mt-3 inline-flex items-center gap-2 text-sm font-semibold text-primary transition-opacity hover:opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                >
                  {t("加入 TG 群聊")}
                  <ExternalLink className="h-4 w-4" />
                </button>
                <p className="mt-3 text-xs leading-6 text-muted-foreground">
                  {t("README 里维护的官方群链接，打开后即可加入讨论。")}
                </p>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
