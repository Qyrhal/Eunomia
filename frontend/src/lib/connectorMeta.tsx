import { Landmark, Mic, Plug2 } from "lucide-react";
import type { Connector } from "@/lib/api";

export type FieldDef = { key: string; label: string; placeholder: string; secret: boolean };

export type ConnectorMeta = {
  label: string;
  description: string;
  icon: React.ReactNode;
  /** CSS custom property name carrying this connector's icon-tile tint. */
  tint: string;
  help?: React.ReactNode;
  fields: FieldDef[];
  /** `sources.list()` key this connector's data lives under, once connected — null if it has no dedicated workspace. */
  sourceKey: string | null;
  /** This connector's provider supports push webhooks -- shows the per-user webhook URL to register with it. */
  webhooks: boolean;
};

export const CONNECTOR_META: Record<Connector["kind"], ConnectorMeta> = {
  up_bank: {
    label: "Up Bank",
    description: "Track transactions and balances from your Up Bank account.",
    icon: <Landmark size={18} />,
    tint: "var(--connector-up-bank)",
    help: (
      <>
        Generate a token at{" "}
        <a href="https://api.up.com.au" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--ink)" }}>
          api.up.com.au
        </a>
        .
      </>
    ),
    fields: [
      { key: "personal_access_token", label: "Personal access token", placeholder: "up:yeah:…", secret: true },
      { key: "webhook_secret_key", label: "Webhook secret key", placeholder: "paste the secretKey Up gave you", secret: true },
    ],
    sourceKey: "up_bank",
    webhooks: true,
  },
  pocketai: {
    label: "PocketAI",
    description: "Sync meeting recordings and transcripts from HeyPocket.",
    icon: <Mic size={18} />,
    tint: "var(--connector-pocketai)",
    fields: [
      { key: "base_url", label: "Base URL", placeholder: "https://public.heypocketai.com/api/v1", secret: false },
      { key: "api_key", label: "API key", placeholder: "pk_…", secret: true },
    ],
    sourceKey: "heypocket",
    webhooks: false,
  },
  open_connector: {
    label: "Open Connector",
    description: "Bridge to a self-hosted gateway for any app it brokers.",
    icon: <Plug2 size={18} />,
    tint: "var(--connector-open-connector)",
    help: (
      <>
        Points at a self-hosted{" "}
        <a href="https://github.com/oomol-lab/open-connector" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--ink)" }}>
          Open Connector
        </a>{" "}
        gateway — its own runtime token, not an app-specific credential. Exposes any app it brokers as
        the <code className="font-mono">open_connector_call</code> tool.
      </>
    ),
    fields: [
      { key: "base_url", label: "Base URL", placeholder: "http://localhost:3000", secret: false },
      { key: "api_key", label: "Runtime token", placeholder: "…", secret: true },
    ],
    sourceKey: null,
    webhooks: false,
  },
};

export const CONNECTOR_ORDER: Connector["kind"][] = ["up_bank", "pocketai", "open_connector"];

export function connectorStatus(c: Connector | undefined): "demo" | "connected" | "disconnected" {
  if (!c) return "disconnected";
  if (c.config?.demo) return "demo";
  if (c.enabled && c.credentials_set) return "connected";
  return "disconnected";
}
