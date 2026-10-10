-- Per-user chat model for extraction, consolidation, reflect and chat ("" = automatic).
DEFINE FIELD OVERWRITE chat_model ON app_settings TYPE string DEFAULT "";

-- Pocket recordings stored whole: everything Pocket returned (`raw`) plus the speaker-labelled transcript,
-- summary, action items, tags and speakers. The cache holds the same recording chunked for search.
DEFINE TABLE OVERWRITE pocket_recording SCHEMAFULL;
DEFINE FIELD OVERWRITE owner ON pocket_recording TYPE record<user>;
DEFINE FIELD OVERWRITE recording_id ON pocket_recording TYPE string;
DEFINE FIELD OVERWRITE title ON pocket_recording TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE recorded_at ON pocket_recording TYPE option<datetime>;
DEFINE FIELD OVERWRITE duration_seconds ON pocket_recording TYPE number DEFAULT 0;
DEFINE FIELD OVERWRITE tags ON pocket_recording TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE speakers ON pocket_recording TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE summary ON pocket_recording TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE action_items ON pocket_recording TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE transcript ON pocket_recording TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE raw ON pocket_recording TYPE object FLEXIBLE DEFAULT {};
DEFINE FIELD OVERWRITE synced_at ON pocket_recording TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE pocket_recording_owner_idx ON pocket_recording FIELDS owner;
-- Pocket syncs before this table stored no transcript or summary: restart those cursors so the next
-- sync refetches every recording. Runs once, with the table still empty.
-- (record::id is outside the hardened server's function allow-list: match the id's text instead)
UPDATE sync_status SET cursor = "" WHERE cursor != "" AND string::contains(<string> id, ":heypocket");
