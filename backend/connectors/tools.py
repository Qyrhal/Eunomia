"""MCP tools exposing the Open Connector gateway — one generic action-call +
one list-connections tool, so Hermes/Claude can drive any app it supports
without a per-app tool here."""

from .clients import OpenConnectorClient
from .models import Connector


def _client() -> OpenConnectorClient:
    connector = Connector.objects.filter(kind=Connector.Kind.OPEN_CONNECTOR, enabled=True).first()
    if not connector:
        raise ValueError("open_connector is not connected")
    return OpenConnectorClient(connector.credentials, connector.config.get("base_url"))


def open_connector_call(action: str, params: dict | None = None) -> dict:
    try:
        return _client().call_action(action, params)
    except Exception as e:
        return {"error": str(e)}


def open_connector_list_connections() -> dict:
    try:
        return _client().list_connections()
    except Exception as e:
        return {"error": str(e)}


SCHEMAS = {
    "open_connector_call": {
        "type": "object",
        "properties": {
            "action": {"type": "string", "description": "'{provider}.{action}', e.g. 'github.get_current_user'"},
            "params": {"type": "object"},
        },
        "required": ["action"],
    },
    "open_connector_list_connections": {"type": "object", "properties": {}},
}

IMPLS = {
    "open_connector_call": open_connector_call,
    "open_connector_list_connections": open_connector_list_connections,
}


def register():
    from tools.registry import register_tool

    for name, impl in IMPLS.items():
        register_tool(name, SCHEMAS[name], impl)
