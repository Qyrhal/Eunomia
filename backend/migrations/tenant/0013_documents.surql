-- Uploaded documents (docs/documents.md). The original bytes live in the org database's `documents`
-- file bucket at `object_path` (defined at provisioning and boot from EUNOMIA_DOCUMENTS_BACKEND, outside
-- the ledger); the extracted text is indexed as `cache_record`s of source "documents", type
-- "document.chunk", whose payload names this record, its revision and the chunk's character range.
DEFINE TABLE OVERWRITE document SCHEMAFULL;
DEFINE FIELD OVERWRITE owner ON document TYPE record<user>;
DEFINE FIELD OVERWRITE vault ON document TYPE record<vault>;
DEFINE FIELD OVERWRITE filename ON document TYPE string;
DEFINE FIELD OVERWRITE media_type ON document TYPE string;
DEFINE FIELD OVERWRITE size_bytes ON document TYPE int;
DEFINE FIELD OVERWRITE sha256 ON document TYPE string;
DEFINE FIELD OVERWRITE object_path ON document TYPE string;
-- bumped by a re-upload or a re-index; chunks of any other revision are retired once this one is indexed
DEFINE FIELD OVERWRITE revision ON document TYPE int DEFAULT 1;
DEFINE FIELD OVERWRITE status ON document TYPE string ASSERT $value IN ["indexing", "ready", "failed", "deleted"];
DEFINE FIELD OVERWRITE error ON document TYPE option<string>;
DEFINE FIELD OVERWRITE chunk_count ON document TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE created_at ON document TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE updated_at ON document TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE indexed_at ON document TYPE option<datetime>;
DEFINE INDEX OVERWRITE document_owner_idx ON document FIELDS owner;
