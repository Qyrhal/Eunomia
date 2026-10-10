# Documents

Upload a file, and Eunomia keeps the original and makes its text searchable. The original stays downloadable byte for byte, and its text is split into passages that `search`, `recall` and `get` find like synced records. Each hit names its document, so you can always open the source. Entities and facts are extracted from the passages too, when a model is configured.

Upload from the **Documents** page (sidebar) or with the `document_upload` MCP tool. Documents belong to your personal vault and only you can see them.

## What is supported

| | |
|---|---|
| Types | Plain text (`.txt`), Markdown (`.md`), JSON, CSV, and PDFs that contain text. A scanned PDF (only images) uploads, but indexing fails with "no text could be extracted", and the file is still downloadable. Office documents and images are not supported. |
| Size | Up to `EUNOMIA_DOCUMENTS_MAX_BYTES`, 25 MiB by default. An MCP upload also has to fit in the request size limit, `MAX_REQUEST_BODY_BYTES` (1 MiB by default, and base64 makes a file about a third larger). Upload bigger files from the Documents page or over HTTP (below). |
| Passages | About 6,000 characters each, cut at line ends where possible. Each passage records the character range it covers in the extracted text. |
| Status | `indexing` → `ready`, or `failed` with the reason. Indexing runs in the background job queue. A failed document is still downloadable, and **Re-index** on its page tries again. |

## How it works

- The original bytes go into a SurrealDB **file bucket** named `documents`, one per org database. Eunomia picks the object path (`/<document key>/<revision>/<sanitised name>`). Nothing you send becomes a path or a URL.
- The extracted passages are ordinary cache records (source `documents`, type `document.chunk`, id `documents:document.chunk:<key>:<revision>:<part>`). Their payload holds the document id, the revision, the part number, the character range and the filename. They are embedded into the same vector index as everything else, and keyword search works without an embedding key.
- A search, recall or get hit on a passage carries a `document` object: `document_id`, `chunk_id`, `revision`, `part`, `char_start`, `char_end`, `filename` and `download_url`.
- Uploading a new version (`document_id` on `document_upload`, or `?replace=` over HTTP) or re-indexing makes a new **revision**. Once that revision is indexed, the passages of the earlier ones and the facts extracted from them are removed, so no hit points at text that is gone.
- **Deleting** a document takes its passages out of search, recall and get in one transaction, together with the facts and relations extracted from them, then removes the stored file and the record. If removing the file fails, the document stays hidden and deleting it again finishes the job.
- Downloads and exports are written to the audit log with the document id, never the content. So are uploads (name and size only) and deletions.

## MCP tools

| Tool | Does |
|---|---|
| `document_upload` | `filename` plus `text` (UTF-8) or `content_base64` (for a PDF). Optional `content_type` and `document_id` (to upload a new version). Returns the document with status `indexing`. |
| `document_list` | Your documents, newest first, with status and passage count. |
| `document_get` | One document: metadata, the extracted text (up to `max_chars`, default 100,000), and each passage's id and character range. |
| `document_download` | The HTTP route for the original bytes (`GET /api/documents/<id>/download`), with size and SHA-256 to check them. The bytes never go through MCP. |
| `document_export` | The export manifest: every passage with its text, location and embedding state, plus the embedding model and dimension. Vectors are left out unless `include_vectors` is true. `vectors_url` serves the full manifest over HTTP. |
| `document_delete` | Delete the document, its passages and the facts extracted from them. Flagged `destructiveHint`. |

An agent that has an HTTP tool can download with the same `Authorization: Bearer <token>` header it uses for MCP. Paths are relative to the address the agent reaches Eunomia at, the same one as `/mcp`.

## HTTP

All routes take the session cookie or `Authorization: Bearer <token>`. A token restricted to another vault, or one without `memory:write`, cannot upload or delete.

```bash
# upload (the body is the file; the name goes in the query)
curl -H "Authorization: Bearer $TOKEN" -H "Content-Type: text/markdown" \
  --data-binary @project-notes.md "http://localhost:8001/api/documents?filename=project-notes.md"

curl -H "Authorization: Bearer $TOKEN" http://localhost:8001/api/documents                         # list
curl -H "Authorization: Bearer $TOKEN" http://localhost:8001/api/documents/document:abc            # text and passages
curl -OJ -H "Authorization: Bearer $TOKEN" http://localhost:8001/api/documents/document:abc/download  # original
curl -OJ -H "Authorization: Bearer $TOKEN" http://localhost:8001/api/documents/document:abc/export    # manifest with vectors
curl -X POST -H "Authorization: Bearer $TOKEN" http://localhost:8001/api/documents/document:abc/reindex
curl -X DELETE -H "Authorization: Bearer $TOKEN" http://localhost:8001/api/documents/document:abc
```

The export manifest has `format: "eunomia.document-export"` and `version: 1`. It includes the document (with its SHA-256) and an `embedding` block with `dimension` (1536), `provider`, `model`, `chunks_embedded` and `chunks_missing`. It also has one entry per passage with `chunk_id`, `part`, `char_start`, `char_end`, `text`, `embedding_state` (`present` or `missing`) and `embedding` (the vector, or `null`). The model is the one your settings name now: vectors do not yet record which model made them.

## Setup

Storage is SurrealDB's file bucket feature. In SurrealDB 3.3 it is **experimental**, so the database server must allow it, and it must also allow the `file` functions:

```
surreal start ... --allow-experimental=files --allow-funcs=time,string,search,count,array,vector,math,file,type::file ...
```

Write `--allow-experimental=files` with the `=`. Without it, the flag takes the next argument (the database path) as a second feature name, and the server refuses to start. The environment variable `SURREAL_CAPS_ALLOW_EXPERIMENTAL=files` does the same.

The backend reads two settings:

| Variable | Default | Meaning |
|---|---|---|
| `EUNOMIA_DOCUMENTS_BACKEND` | empty (off) outside compose; `file:/documents` in `docker-compose.yml` | Where the database keeps the files: `file:/absolute/path`, `s3://bucket?region=...`, `s3+https://host/bucket`, `s3+http://host:9000/bucket`, `gs://bucket`, `az://container`, or `memory` (tests only, lost on restart). Optional `prefix=` for S3, GCS and Azure. Empty turns documents off: uploads answer `document.storage_unavailable`, and everything else works. |
| `EUNOMIA_DOCUMENTS_MAX_BYTES` | `26214400` (25 MiB) | The largest upload accepted. |

Eunomia appends each org's database name to the folder or key prefix (`/documents/org_<id>/...`, or `prefix=<yours>/org_<id>`), so orgs never share keys. Every org's bucket is defined when the org is created, and again at every backend start, so a changed setting applies after a restart. A bucket can only be defined by the root user, never by the org's own database user.

### The default: a folder on the database server

`docker-compose.yml` mounts the volume `eunomia-documents` at `/documents` in the `surrealdb` container. It allows that folder with `SURREAL_BUCKET_FOLDER_ALLOWLIST=/documents` and sets `EUNOMIA_DOCUMENTS_BACKEND=file:/documents`. The path is on the **database server**, not the backend. A file backend outside the allowlist is refused.

### S3, MinIO, Cloudflare R2 and other S3-compatible stores

1. Create a private bucket.
2. Give the **SurrealDB server** credentials in its environment, never in the URL. `DEFINE BUCKET` text is visible to `INFO FOR DB`, so Eunomia refuses a URL with credentials in it. Add to the `surrealdb` service's `environment` in a `docker-compose.override.yml`:
   ```yaml
   services:
     surrealdb:
       environment:
         - AWS_ACCESS_KEY_ID=${DOCS_S3_KEY}
         - AWS_SECRET_ACCESS_KEY=${DOCS_S3_SECRET}
         - AWS_REGION=us-east-1
   ```
   Use a key that can only `GetObject`, `PutObject`, `DeleteObject` and `ListBucket` on that one bucket. On AWS an instance or workload role works too: the standard AWS credential chain is used.
3. Point the backend at the bucket: `EUNOMIA_DOCUMENTS_BACKEND=s3://my-bucket?region=eu-west-1` (AWS), `s3+https://<account>.r2.cloudflarestorage.com/my-bucket?region=auto` (R2), or `s3+http://minio:9000/eunomia-docs` (MinIO in the same compose network). A host in the URL is treated as a custom endpoint with path-style addressing. `s3+http` allows plain HTTP, so use it only on a private network.
4. Restart: `docker compose up -d`.

Downloads go through the backend, which checks the owner every time. There are no pre-signed URLs, so revoking access is immediate, and the bucket needs no CORS rules and no public access.

What has been tested: the embedded engine and a SurrealDB 3.3.1 server (hardened flags as in `docker-compose.yml`) against a MinIO server over `s3+http`, the `file:` backend, and `memory`. AWS S3, R2, Backblaze B2, Wasabi, Google Cloud Storage and Azure use the same SurrealDB code path (the `object_store` library), but Eunomia has not been tested against them. Try an upload and a download before you rely on one.

**Local MinIO for testing.** The `minio` compose profile starts a MinIO container, a community build because the upstream `minio/minio` image is no longer published. It also creates the bucket `eunomia-docs`:

```bash
# .env
COMPOSE_PROFILES=minio
MINIO_ROOT_USER=eunomia-docs
MINIO_ROOT_PASSWORD=<a long random string>
EUNOMIA_DOCUMENTS_BACKEND=s3+http://minio:9000/eunomia-docs
```

Give the `surrealdb` service `AWS_ACCESS_KEY_ID=${MINIO_ROOT_USER}` and `AWS_SECRET_ACCESS_KEY=${MINIO_ROOT_PASSWORD}` with an override file as in step 2, then run `docker compose up -d`. Use it for trying things out. A production store should have its own least-privilege key.

## Backups

**A SurrealDB export does not contain the files.** It holds the document records, the passages and their vectors, and the bucket definition, but not the bytes. You need both to restore documents.

- With the default `file:` backend, the nightly `backup` service also writes `documents.tar.enc`: the `/documents` folder, encrypted the same way as the database exports. It is written into the same backup directory. `backend/scripts/restore.sh` restores it into the volume when it is present. Check for it after your first backup.
- With S3 or another object store, back the bucket up with the provider's own tools: versioning, replication, or `aws s3 sync` / `rclone sync` to a second location. Eunomia does not copy object-store buckets.

A database restored without its files lists the documents and finds their passages, but downloads answer `document.storage_unavailable` ("the stored file is missing"). Delete those documents or upload them again.

## Errors

| Code | When |
|---|---|
| `document.not_found` (404) | Not your document, never existed, or already deleted. Another user's or org's id answers exactly like a missing one. |
| `document.too_large` (413) | Over `EUNOMIA_DOCUMENTS_MAX_BYTES`. |
| `document.unsupported_type` (415) | Not text, Markdown, JSON, CSV or PDF. |
| `validation.invalid` (400) | Empty, not UTF-8 text, invalid JSON, a "PDF" without a PDF header, bad base64, no filename. |
| `document.storage_unavailable` (503) | Storage is off, the server lacks `--allow-experimental=files` or the `file` functions, or the store cannot be reached. The backend log has the cause. |

## Limits and what is not there yet

- Personal vault only. Documents in shared vaults are not supported.
- No OCR, Office documents, images, public links or importing from a URL.
- Upload goes through the backend, which holds the file in memory while storing it. The default 25 MiB limit keeps that small.
- Bucket files are not written in the same transaction as the database. A crash between storing the file and writing its record can leave an unreferenced file in the bucket. It is harmless and nothing points at it.
- Extracted text is untrusted content. Agents should treat it as data, not instructions.
