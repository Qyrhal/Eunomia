FROM oven/bun:1 AS build
WORKDIR /app
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile
COPY . .
ARG NEXT_PUBLIC_API_URL=http://localhost:8001
ENV NEXT_PUBLIC_API_URL=$NEXT_PUBLIC_API_URL
ARG NEXT_PUBLIC_MCP_URL=http://localhost:8765/mcp
ENV NEXT_PUBLIC_MCP_URL=$NEXT_PUBLIC_MCP_URL
RUN bun run build

FROM oven/bun:1
WORKDIR /app
COPY --from=build /app ./
EXPOSE 3000
CMD ["bun", "run", "start"]
