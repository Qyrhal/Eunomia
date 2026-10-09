# Connectors

Connectors pull your data from other services into Eunomia, where agents read it with the `search`, `get`, `list`, `links` and `recall` tools. Set one up on the **Connectors** page: paste the credential, **Save**, then **Test connection** (one real call to the provider) and **Sync now**. Credentials are encrypted at rest and only ever sent to that provider.

After the first sync, each connector syncs again on its interval (changeable on its setup page). A failed sync shows the provider's error on the connector's page and the dashboard, and retries with backoff (15 min → 6 h).

| Connector | What it syncs | Credential (where to get it) | Scopes / permissions | Default interval |
|---|---|---|---|---|
| Up Bank | Transactions (from 30 days back, then incremental), accounts, categories; webhooks optional | Personal access token — [api.up.com.au](https://api.up.com.au/getting_started) or Up app → Profile → Data sharing | Read-only by design | 15 min |
| PocketAI (HeyPocket) | Meeting recordings: full transcript with speakers, summary, action items, tags (30 days back, then incremental). Everything Pocket returns is stored whole (below) | API key from your Pocket account's developer settings | — | 24 h |
| GitHub | Issues and pull requests you created, are assigned or mentioned in (90 days back, then incremental) | [Fine-grained token](https://github.com/settings/personal-access-tokens/new), or a classic token | Fine-grained: Issues + Pull requests (read) on chosen repos. Classic: `repo` | 15 min |
| Slack | Messages in channels the app has joined (30 days back, then incremental; thread replies not included) | Bot token `xoxb-…` from your app at [api.slack.com/apps](https://api.slack.com/apps), then `/invite` it to channels | `channels:read`, `channels:history`, `groups:read`, `groups:history` | 15 min |
| Notion | Pages (title + top-level text) and databases shared with the integration | Internal integration secret from [notion.so/profile/integrations](https://www.notion.so/profile/integrations); add it to pages via ••• → Connections | Read content | 15 min |
| Linear | Issues assigned to you | Personal API key — Linear → Settings → Security & access | Read | 15 min |
| Gmail | Messages from the last 30 days onwards: subject, sender, plain-text body (200 per sync, oldest first; a larger backlog drains over the next syncs) | Your own Google OAuth client ID + secret and a refresh token (below) | `https://www.googleapis.com/auth/gmail.readonly` | 15 min |
| Google Calendar | Primary-calendar events, 30 days back to 6 months ahead, then every change (cancellations remove the event) | Same as Gmail | `https://www.googleapis.com/auth/calendar.readonly` | 15 min |
| Discord | Messages in the channels you list (comma-separated channel IDs) | Bot token from the [Developer Portal](https://discord.com/developers/applications) | Message Content Intent; View Channels + Read Message History | 15 min |
| Spotify | Recently played tracks (Spotify keeps only the last 50, so history builds up from when you connect) | Your own Spotify app's client ID + secret and a refresh token (below) | `user-read-recently-played` | 15 min |
| Todoist | Active tasks with project, due date, labels | API token — Todoist → Settings → Integrations → Developer | Full API token (Todoist has no read-only token) | 15 min |
| Stripe | Charges (incremental) | [Restricted key](https://dashboard.stripe.com/apikeys) `rk_…` | Charges: Read only | 15 min |

## Pocket recordings

Each recording is stored whole — everything Pocket's API returns, verbatim, plus the full speaker-labelled transcript — so an agent can always read the entire meeting: `get` on a Pocket record (the recording or any of its transcript chunks) returns the whole stored recording. For search and memory, the recording is also cached as a summary record (summary, action items, speakers, tags) plus transcript chunks small enough to embed. With a chat model available (Settings → OpenAI), people, organisations and their relations are extracted from each chunk into the memory graph.

## Deleting a connector's data

**Delete all data** on a connector's page (two confirmations) permanently removes everything synced from it: its records and their search index, Pocket's stored recordings, and every fact and relation extracted from them. Observations built on those facts are rebuilt from what remains. Entities themselves, the connection and its sync position are kept, so later syncs bring in new data only. API: `DELETE /api/connectors/{kind}/data`.

## Google (Gmail, Google Calendar): getting a refresh token

Google has no personal access tokens, and a self-hosted server has no public redirect URL, so you use your own OAuth client and mint a long-lived refresh token once:

1. In [Google Cloud Console](https://console.cloud.google.com/apis/library), enable the Gmail API and/or Google Calendar API.
2. Configure the OAuth consent screen (External), add yourself as a test user, then **publish** it ("In production") — refresh tokens of apps left in "Testing" expire after 7 days.
3. Create an OAuth client ID, type **Web application**, with redirect URI `https://developers.google.com/oauthplayground`.
4. In the [OAuth 2.0 Playground](https://developers.google.com/oauthplayground), click ⚙ → "Use your own OAuth credentials" and enter the client ID and secret. Authorize the scope(s) above (both at once if you want one token for both connectors), then "Exchange authorization code for tokens".
5. Paste the client ID, client secret and refresh token into the connector. Eunomia exchanges the refresh token for a fresh access token on every sync.

## Spotify: getting a refresh token

1. Create an app at [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard) (Web API) with redirect URI `http://127.0.0.1:8888/callback`.
2. Open `https://accounts.spotify.com/authorize?response_type=code&scope=user-read-recently-played&redirect_uri=http://127.0.0.1:8888/callback&client_id=CLIENT_ID`, approve, and copy the `code` parameter from the address bar (the page itself won't load — nothing is listening there).
3. Exchange it:
   `curl -u CLIENT_ID:CLIENT_SECRET -d grant_type=authorization_code -d code=CODE -d redirect_uri=http://127.0.0.1:8888/callback https://accounts.spotify.com/api/token`
4. Paste the client ID, client secret and the response's `refresh_token` into the connector.

## Troubleshooting

- **HTTP 401 / 403** — the token is wrong, revoked or missing a scope above. Fix it on the connector's page and **Test connection**.
- **HTTP 429** — the provider rate-limited the sync; it retries automatically.
- **`not configured: missing …`** — a required field is empty.
- **Slack `not_in_channel` or nothing synced** — `/invite` the app to the channels.
- **Discord messages with no text** — enable Message Content Intent for the bot.
- **Google `invalid_grant`** — the refresh token expired or was revoked (an app still in "Testing" expires it after 7 days); mint a new one.

## Self-hosted providers (e.g. GitHub Enterprise)

Connectors talk to each provider's public API. To point one at a different
host (a GitHub Enterprise server, or a mock in tests), the server operator
sets `EUNOMIA_ALLOW_CONNECTOR_BASE_URL=1` on the backend, and the connector's
config can then carry a `base_url`. It's off by default because on a shared
server it would let any user make the backend call internal addresses.
