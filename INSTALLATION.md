# Installing Eunomia (for the Hermes agent doing the install)

You're a Hermes agent, installing Eunomia onto the box you're running on, next
to yourself. This doc is written for you to execute top to bottom. Where it
says "ask the operator," stop and actually ask — don't guess.

Eunomia and Hermes must end up on the **same host**, reachable over
tailscale/netbird only. Never bind anything to a public interface.

## Step 0 — pick Docker or k3s

This box has both. Ask the operator which one to use. If they don't care,
here's the difference in one line each:

- **Docker Compose** — simpler, one `docker compose up`, easiest to debug and
  tear down. Pick this unless you already run other workloads on k3s.
- **k3s** — use this if Eunomia should live next to other things you already
  manage as k8s workloads on this box.

Then jump to [Docker path](#docker-path) or [k3s path](#k3s-path) below.
Both end at [Step 3 — point Hermes at it](#step-3--point-hermes-at-it), which
is the same either way.

## Step 1 — get the repo

```bash
git clone https://github.com/Qyrhal/Eunomia.git
cd Eunomia
```

(Skip if it's already checked out — just `cd` into it.)

## Step 2 — generate secrets

Do this once regardless of path; both paths consume the same values.

```bash
JWT_SECRET=$(openssl rand -base64 32)
ENCRYPTION_KEY=$(openssl rand -base64 32)
```

If the operator has Up Bank / heypocket creds ready, ask for them now too.
Not required to get Eunomia running — those connectors can be added later
from the frontend.

---

## Docker path

```bash
cat > .env <<EOF
SECRET_KEY=${SECRET_KEY}
ENCRYPTION_KEY=${ENCRYPTION_KEY}
EUNOMIA_API_TOKEN=${EUNOMIA_API_TOKEN}
FRONTEND_URL=http://localhost:3000
NEXT_PUBLIC_API_URL=http://localhost:8000
NEXT_PUBLIC_API_TOKEN=${EUNOMIA_API_TOKEN}
EOF

docker compose up -d --build
```

That brings up `backend` (:8000), `worker`, `mcp` (:8765), `frontend`
(:3000). Verify:

```bash
curl -s http://localhost:8000/api/ -H "Authorization: Bearer ${EUNOMIA_API_TOKEN}"
docker compose ps   # all four should be "running"/"healthy"
```

**Rebind off 0.0.0.0** — Compose publishes on all interfaces by default. On a
tailscale box, edit `docker-compose.yml` and prefix each `ports:` entry with
your tailscale IP, e.g. `"100.x.y.z:8765:8765"`, then `docker compose up -d`
again to apply.

**Uninstall:** `docker compose down -v` (the `-v` also drops the sqlite
volume — drop it only if the operator wants the data gone too).

Now go to [Step 3](#step-3--point-hermes-at-it).

---

## k3s path

k3s uses containerd, not the Docker daemon — build with `docker build` as
usual, then hand the image to k3s directly (no registry needed for a
single-node box):

```bash
docker build -t eunomia-backend:local ./backend
docker build -t eunomia-frontend:local \
  --build-arg NEXT_PUBLIC_API_URL=http://eunomia-backend:8000 \
  --build-arg NEXT_PUBLIC_API_TOKEN="${EUNOMIA_API_TOKEN}" \
  ./frontend

docker save eunomia-backend:local  | sudo k3s ctr images import -
docker save eunomia-frontend:local | sudo k3s ctr images import -
```

Apply the manifest (namespace, secret, storage, the four workloads, and
ClusterIP services — nothing here touches a public interface; reach it
through `kubectl port-forward` over tailscale, or your existing tailscale
Kubernetes operator if you run one):

```bash
kubectl apply -f - <<EOF
apiVersion: v1
kind: Namespace
metadata: { name: eunomia }
---
apiVersion: v1
kind: Secret
metadata: { name: eunomia-env, namespace: eunomia }
stringData:
  SECRET_KEY: "${SECRET_KEY}"
  ENCRYPTION_KEY: "${ENCRYPTION_KEY}"
  EUNOMIA_API_TOKEN: "${EUNOMIA_API_TOKEN}"
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata: { name: eunomia-data, namespace: eunomia }
spec:
  accessModes: [ReadWriteOnce]
  resources: { requests: { storage: 2Gi } }
---
apiVersion: apps/v1
kind: Deployment
metadata: { name: backend, namespace: eunomia }
spec:
  replicas: 1
  selector: { matchLabels: { app: backend } }
  template:
    metadata: { labels: { app: backend } }
    spec:
      containers:
      - name: backend
        image: eunomia-backend:local
        imagePullPolicy: Never
        ports: [{ containerPort: 8000 }]
        envFrom: [{ secretRef: { name: eunomia-env } }]
        env: [{ name: SQLITE_PATH, value: /data/db.sqlite3 }]
        volumeMounts: [{ name: data, mountPath: /data }]
      volumes: [{ name: data, persistentVolumeClaim: { claimName: eunomia-data } }]
---
apiVersion: apps/v1
kind: Deployment
metadata: { name: worker, namespace: eunomia }
spec:
  replicas: 1
  selector: { matchLabels: { app: worker } }
  template:
    metadata: { labels: { app: worker } }
    spec:
      containers:
      - name: worker
        image: eunomia-backend:local
        imagePullPolicy: Never
        command: ["sh", "-c", "python manage.py migrate --noinput && python manage.py run_worker"]
        envFrom: [{ secretRef: { name: eunomia-env } }]
        env: [{ name: SQLITE_PATH, value: /data/db.sqlite3 }]
        volumeMounts: [{ name: data, mountPath: /data }]
      volumes: [{ name: data, persistentVolumeClaim: { claimName: eunomia-data } }]
---
apiVersion: apps/v1
kind: Deployment
metadata: { name: mcp, namespace: eunomia }
spec:
  replicas: 1
  selector: { matchLabels: { app: mcp } }
  template:
    metadata: { labels: { app: mcp } }
    spec:
      containers:
      - name: mcp
        image: eunomia-backend:local
        imagePullPolicy: Never
        command: ["python", "mcp_server.py", "--http", "--host", "0.0.0.0", "--port", "8765"]
        ports: [{ containerPort: 8765 }]
        envFrom: [{ secretRef: { name: eunomia-env } }]
        env: [{ name: SQLITE_PATH, value: /data/db.sqlite3 }]
        volumeMounts: [{ name: data, mountPath: /data }]
      volumes: [{ name: data, persistentVolumeClaim: { claimName: eunomia-data } }]
---
apiVersion: apps/v1
kind: Deployment
metadata: { name: frontend, namespace: eunomia }
spec:
  replicas: 1
  selector: { matchLabels: { app: frontend } }
  template:
    metadata: { labels: { app: frontend } }
    spec:
      containers:
      - name: frontend
        image: eunomia-frontend:local
        imagePullPolicy: Never
        ports: [{ containerPort: 3000 }]
---
apiVersion: v1
kind: Service
metadata: { name: eunomia-backend, namespace: eunomia }
spec: { selector: { app: backend }, ports: [{ port: 8000, targetPort: 8000 }] }
---
apiVersion: v1
kind: Service
metadata: { name: eunomia-mcp, namespace: eunomia }
spec: { selector: { app: mcp }, ports: [{ port: 8765, targetPort: 8765 }] }
---
apiVersion: v1
kind: Service
metadata: { name: eunomia-frontend, namespace: eunomia }
spec: { selector: { app: frontend }, ports: [{ port: 3000, targetPort: 3000 }] }
EOF
```

Verify:

```bash
kubectl -n eunomia get pods -w   # ctrl-c once all are Running
kubectl -n eunomia port-forward svc/eunomia-mcp 8765:8765 &
curl -s http://localhost:8765/mcp -H "Authorization: Bearer ${EUNOMIA_API_TOKEN}"
```

To actually reach it from Hermes over tailscale without a manual
port-forward every time, either put `kubectl port-forward` in a systemd unit
bound to the tailscale interface, or use whatever ingress/LoadBalancer setup
already exists on this k3s box — that part is site-specific, ask the
operator if one isn't already there.

**Uninstall:** `kubectl delete namespace eunomia` (drops the PVC and its data
too).

Now go to [Step 3](#step-3--point-hermes-at-it).

---

## Step 3 — point Hermes at it

Add the MCP server to your own config, `~/.hermes/config.yaml`:

```yaml
mcp_servers:
  eunomia:
    url: http://127.0.0.1:8765/mcp        # or the k3s service address you exposed
    headers: { Authorization: "Bearer <EUNOMIA_API_TOKEN from step 2>" }
```

Then reload: `/reload-mcp` (or the `reload.mcp` gateway RPC).

If Eunomia should be able to notify you (task due, trigger fired), set these
in Eunomia's Settings page (`http://localhost:3000` → Settings), pointing at
your own gateway webhook:

- `hermes_webhook_url` — `http://<this-host>:8644/webhooks/<route-name>`
- `hermes_webhook_secret` — a shared secret you also put in your own
  `config.yaml` under `platforms.webhook.extra.routes.<route-name>.secret`

Full webhook route shape and the reasoning behind this design:
`docs/research/hermes-integration.md`.

## Step 4 — hand back to the operator

Tell them: what got installed (Docker or k3s), where the frontend is
(`http://localhost:3000`), and that Up Bank/heypocket connectors are still
empty until they paste creds in on the Connectors page.
