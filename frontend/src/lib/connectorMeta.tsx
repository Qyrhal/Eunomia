import {
  Calendar,
  CheckSquare,
  CreditCard,
  Database,
  GitBranch,
  Landmark,
  Mail,
  MessageCircle,
  MessagesSquare,
  Mic,
  Music,
  Notebook,
  Plug2,
  Workflow,
} from "lucide-react";
import type { Connector } from "@/lib/types";

export type FieldDef = { key: string; label: string; placeholder: string; secret: boolean };

export type ConnectorMeta = {
  label: string;
  description: string;
  icon: React.ReactNode;
  /** CSS custom property name carrying this connector's icon-tile tint. */
  tint: string;
  help?: React.ReactNode;
  fields: FieldDef[];
  /** `sources.list()` key this connector's data lives under, once connected, or null if it has no dedicated workspace. */
  sourceKey: string | null;
  /** This connector's provider supports push webhooks -- shows the per-user webhook URL to register with it. */
  webhooks: boolean;
};

export const CONNECTOR_META: Record<Connector["kind"], ConnectorMeta> = {
  up_bank: {
    label: "Up Bank",
    description: "Track transactions and balances from your Up Bank account.",
    icon: <Landmark size={18} strokeWidth={1.75} />,
    tint: "var(--connector-up-bank)",
    help: (
      <>
        Generate a token at{" "}
        <a href="https://api.up.com.au" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--accent-text)" }}>
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
    icon: <Mic size={18} strokeWidth={1.75} />,
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
    icon: <Plug2 size={18} strokeWidth={1.75} />,
    tint: "var(--connector-open-connector)",
    help: (
      <>
        Points at a self-hosted{" "}
        <a href="https://github.com/oomol-lab/open-connector" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--accent-text)" }}>
          Open Connector
        </a>{" "}
        gateway. It uses its own runtime token, not an app-specific credential. Exposes any app it brokers as
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
  github: {
    label: "GitHub",
    description: "Pull in notifications across every repo your token can see.",
    icon: <GitBranch size={18} strokeWidth={1.75} />,
    tint: "var(--connector-github)",
    help: (
      <>
        Generate a{" "}
        <a href="https://github.com/settings/tokens" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--accent-text)" }}>
          personal access token
        </a>{" "}
        with the <code className="font-mono">notifications</code> scope.
      </>
    ),
    fields: [{ key: "personal_access_token", label: "Personal access token", placeholder: "ghp_…", secret: true }],
    sourceKey: "github",
    webhooks: false,
  },
  slack: {
    label: "Slack",
    description: "Sync channels your bot has joined.",
    icon: <MessagesSquare size={18} strokeWidth={1.75} />,
    tint: "var(--connector-slack)",
    help: (
      <>
        Install a{" "}
        <a href="https://api.slack.com/apps" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--accent-text)" }}>
          Slack app
        </a>{" "}
        with a bot token (<code className="font-mono">xoxb-…</code>).
      </>
    ),
    fields: [{ key: "bot_token", label: "Bot token", placeholder: "xoxb-…", secret: true }],
    sourceKey: "slack",
    webhooks: false,
  },
  notion: {
    label: "Notion",
    description: "Sync pages and databases shared with your integration.",
    icon: <Notebook size={18} strokeWidth={1.75} />,
    tint: "var(--connector-notion)",
    help: (
      <>
        Create an{" "}
        <a href="https://www.notion.so/my-integrations" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--accent-text)" }}>
          internal integration
        </a>{" "}
        and share the pages you want synced with it.
      </>
    ),
    fields: [{ key: "integration_token", label: "Integration token", placeholder: "secret_…", secret: true }],
    sourceKey: "notion",
    webhooks: false,
  },
  linear: {
    label: "Linear",
    description: "Sync issues assigned to you.",
    icon: <Workflow size={18} strokeWidth={1.75} />,
    tint: "var(--connector-linear)",
    help: (
      <>
        Generate an API key from Linear&apos;s Settings, API page.
      </>
    ),
    fields: [{ key: "api_key", label: "API key", placeholder: "lin_api_…", secret: true }],
    sourceKey: "linear",
    webhooks: false,
  },
  gmail: {
    label: "Gmail",
    description: "Sync recent messages.",
    icon: <Mail size={18} strokeWidth={1.75} />,
    tint: "var(--connector-gmail)",
    help: <>Paste a live OAuth access token with the Gmail readonly scope (no refresh is performed here).</>,
    fields: [{ key: "access_token", label: "OAuth access token", placeholder: "ya29.…", secret: true }],
    sourceKey: "gmail",
    webhooks: false,
  },
  google_calendar: {
    label: "Google Calendar",
    description: "Sync upcoming and recent events on your primary calendar.",
    icon: <Calendar size={18} strokeWidth={1.75} />,
    tint: "var(--connector-google-calendar)",
    help: <>Paste a live OAuth access token with the Calendar readonly scope (no refresh is performed here).</>,
    fields: [{ key: "access_token", label: "OAuth access token", placeholder: "ya29.…", secret: true }],
    sourceKey: "google_calendar",
    webhooks: false,
  },
  discord: {
    label: "Discord",
    description: "Sync recent messages from one channel your bot can read.",
    icon: <MessageCircle size={18} strokeWidth={1.75} />,
    tint: "var(--connector-discord)",
    help: (
      <>
        Create a bot in the{" "}
        <a href="https://discord.com/developers/applications" target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--accent-text)" }}>
          Discord Developer Portal
        </a>
        , invite it to your server, then paste its bot token and the channel ID to watch.
      </>
    ),
    fields: [
      { key: "bot_token", label: "Bot token", placeholder: "…", secret: true },
      { key: "channel_id", label: "Channel ID", placeholder: "123456789012345678", secret: false },
    ],
    sourceKey: "discord",
    webhooks: false,
  },
  spotify: {
    label: "Spotify",
    description: "Sync your recently played tracks.",
    icon: <Music size={18} strokeWidth={1.75} />,
    tint: "var(--connector-spotify)",
    help: <>Paste a live OAuth access token with the recently-played scope (no refresh is performed here).</>,
    fields: [{ key: "access_token", label: "OAuth access token", placeholder: "BQ…", secret: true }],
    sourceKey: "spotify",
    webhooks: false,
  },
  todoist: {
    label: "Todoist",
    description: "Sync your active tasks.",
    icon: <CheckSquare size={18} strokeWidth={1.75} />,
    tint: "var(--connector-todoist)",
    help: <>Find your API token under Todoist Settings, Integrations, Developer.</>,
    fields: [{ key: "api_token", label: "API token", placeholder: "…", secret: true }],
    sourceKey: "todoist",
    webhooks: false,
  },
  stripe: {
    label: "Stripe",
    description: "Sync recent charges.",
    icon: <CreditCard size={18} strokeWidth={1.75} />,
    tint: "var(--connector-stripe)",
    help: <>Use a restricted-access secret key scoped to read-only charges, not your full secret key.</>,
    fields: [{ key: "secret_key", label: "Secret key", placeholder: "sk_live_…", secret: true }],
    sourceKey: "stripe",
    webhooks: false,
  },
};

export const CONNECTOR_ORDER: Connector["kind"][] = [
  "up_bank",
  "pocketai",
  "open_connector",
  "github",
  "slack",
  "notion",
  "linear",
  "gmail",
  "google_calendar",
  "discord",
  "spotify",
  "todoist",
  "stripe",
];

export function connectorStatus(c: Connector | undefined): "demo" | "connected" | "disconnected" {
  if (!c) return "disconnected";
  if (c.config?.demo) return "demo";
  if (c.enabled && c.credentials_set) return "connected";
  return "disconnected";
}

/** Connector kind whose data lives under a `sources.list()` key, if any. */
export function kindForSource(sourceKey: string): Connector["kind"] | undefined {
  return CONNECTOR_ORDER.find((k) => CONNECTOR_META[k].sourceKey === sourceKey);
}

/** Short relative age for a sync timestamp: "4m ago", "never". */
export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return "never";
  const mins = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/** Connector identity: a small icon tile in the connector's own tint, never the accent. */
export function ConnectorTile({ kind, size = 28 }: { kind: Connector["kind"] | null; size?: number }) {
  const meta = kind ? CONNECTOR_META[kind] : null;
  const tint = meta?.tint ?? "var(--ink-faint)";
  return (
    <span
      aria-hidden
      className="inline-flex items-center justify-center shrink-0"
      style={{
        width: size,
        height: size,
        borderRadius: size >= 40 ? 10 : 7,
        color: tint,
        background: `color-mix(in oklab, ${tint} 16%, var(--surface))`,
        boxShadow: `inset 0 0 0 1px color-mix(in oklab, ${tint} 32%, transparent)`,
      }}
    >
      <span className="inline-flex" style={{ transform: `scale(${Math.max(0.75, size / 34)})` }}>
        {meta?.icon ?? <Database size={18} strokeWidth={1.75} />}
      </span>
    </span>
  );
}
