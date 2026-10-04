from tools import generic, registry


def test_all_tools_merges_generic_tools():
    tools = registry.all_tools()
    for name in ("search", "get", "list", "links"):
        assert name in tools
        assert tools[name]["schema"] is generic.SCHEMAS[name]
        assert tools[name]["impl"] is generic.IMPLS[name]


def test_register_tool_adds_extra_tool():
    async def fake_impl(owner, **kwargs):
        return {"ok": True}

    registry.register_tool("fake_tool", {"type": "object"}, fake_impl)
    try:
        tools = registry.all_tools()
        assert "fake_tool" in tools
        assert tools["fake_tool"]["impl"] is fake_impl
    finally:
        registry._EXTRA.pop("fake_tool", None)


async def test_call_dispatches_to_registered_impl(owner):
    calls = {}

    async def fake_impl(owner, **kwargs):
        calls["kwargs"] = kwargs
        return {"result": "ok"}

    registry.register_tool("fake_tool", {"type": "object"}, fake_impl)
    try:
        result = await registry.call("fake_tool", {"x": 1}, owner)
        assert result == {"result": "ok"}
        assert calls["kwargs"] == {"x": 1}
    finally:
        registry._EXTRA.pop("fake_tool", None)


async def test_call_unknown_tool_returns_error_dict(owner):
    result = await registry.call("does_not_exist", {}, owner)
    assert result == {"error": "unknown tool does_not_exist"}


async def test_call_with_none_args(owner):
    async def fake_impl(owner, **kwargs):
        return {"kwargs": kwargs}

    registry.register_tool("fake_tool", {"type": "object"}, fake_impl)
    try:
        result = await registry.call("fake_tool", None, owner)
        assert result == {"kwargs": {}}
    finally:
        registry._EXTRA.pop("fake_tool", None)


async def test_generic_search_bad_mode_returns_error_dict(surreal_db, owner):
    # @safe must catch bad-input exceptions and return {"error": ...} rather
    # than raising -- int("not a number") raises ValueError.
    result = await generic.search(owner, "q", limit="not a number")
    assert "error" in result
