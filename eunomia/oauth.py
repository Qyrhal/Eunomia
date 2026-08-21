# OAuth2 authorization-code flow, generic across providers.
# Each provider needs an app registered on that platform, with client id/secret passed via env vars.
# GitHub: https://github.com/settings/developers
# Slack:  https://api.slack.com/apps
# Linear: https://linear.app/settings/api/applications

import os

import httpx

PROVIDERS = {
    "github": {
        "name": "GitHub",
        "authorize_url": "https://github.com/login/oauth/authorize",
        "token_url": "https://github.com/login/oauth/access_token",
        "scope": "repo read:user",
        "client_id_env": "EUNOMIA_GITHUB_CLIENT_ID",
        "client_secret_env": "EUNOMIA_GITHUB_CLIENT_SECRET",
    },
    "linear": {
        "name": "Linear",
        "authorize_url": "https://linear.app/oauth/authorize",
        "token_url": "https://api.linear.app/oauth/token",
        "scope": "read",
        "client_id_env": "EUNOMIA_LINEAR_CLIENT_ID",
        "client_secret_env": "EUNOMIA_LINEAR_CLIENT_SECRET",
    },
    "slack": {
        "name": "Slack",
        "authorize_url": "https://slack.com/oauth/v2/authorize",
        "token_url": "https://slack.com/api/oauth.v2.access",
        "scope": "channels:read,chat:write",
        "client_id_env": "EUNOMIA_SLACK_CLIENT_ID",
        "client_secret_env": "EUNOMIA_SLACK_CLIENT_SECRET",
    },
}


def is_configured(service: str) -> bool:
    provider = PROVIDERS[service]
    return bool(os.environ.get(provider["client_id_env"])) and bool(
        os.environ.get(provider["client_secret_env"])
    )


def authorize_url(service: str, redirect_uri: str, state: str) -> str:
    provider = PROVIDERS[service]
    client_id = os.environ[provider["client_id_env"]]
    params = httpx.QueryParams(
        {
            "client_id": client_id,
            "redirect_uri": redirect_uri,
            "scope": provider["scope"],
            "state": state,
        }
    )
    return f"{provider['authorize_url']}?{params}"


def exchange_code_for_token(service: str, code: str, redirect_uri: str) -> str:
    provider = PROVIDERS[service]
    client_id = os.environ[provider["client_id_env"]]
    client_secret = os.environ[provider["client_secret_env"]]
    resp = httpx.post(
        provider["token_url"],
        data={
            "client_id": client_id,
            "client_secret": client_secret,
            "code": code,
            "redirect_uri": redirect_uri,
        },
        headers={"Accept": "application/json"},
        timeout=10,
    )
    resp.raise_for_status()
    data = resp.json()
    token = data.get("access_token")
    if not token:
        raise RuntimeError(f"{provider['name']} token exchange failed: {data}")
    return token
