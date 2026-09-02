"""REST surface (#40): the same tool registry the MCP server exposes, over HTTP.

  GET  /api/tools          -> [{name, schema}]  (the machine-readable catalogue)
  POST /api/tools/<name>   -> run the tool with the JSON body as its arguments

Bearer token is enforced globally (config.auth). Tokens in the request body are
resolved at the boundary inside registry.call().
"""

from rest_framework.response import Response
from rest_framework.views import APIView

from .registry import all_tools, call


class ToolCatalogueView(APIView):
    def get(self, request):
        return Response([
            {"name": name, "schema": spec["schema"]}
            for name, spec in sorted(all_tools().items())
        ])


class ToolCallView(APIView):
    def post(self, request, name):
        if name not in all_tools():
            return Response({"error": f"unknown tool {name}"}, status=404)
        try:
            result = call(name, request.data if isinstance(request.data, dict) else {})
        except TypeError as e:
            return Response({"error": f"bad arguments: {e}"}, status=400)
        return Response(result)
