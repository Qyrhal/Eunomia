import { defineConfig } from "@hey-api/openapi-ts";

export default defineConfig({
  input: "../backend/openapi.json",
  output: "src/lib/gen",
  plugins: [
    { name: "@hey-api/client-fetch", runtimeConfigPath: "@/lib/api-client" },
    "@hey-api/typescript",
    "@hey-api/sdk",
  ],
});
