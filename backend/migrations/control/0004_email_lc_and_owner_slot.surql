-- Sign-in looks a user up by a normalized email with a UNIQUE index instead of scanning with string::lowercase().
-- Backfill: every row first gets a placeholder that cannot collide, then the oldest user of each
-- lowercased email takes the real value. A later duplicate keeps the placeholder (it was never
-- reachable by email before either: the lookup returned one arbitrary row).
DEFINE FIELD OVERWRITE email_lc ON user TYPE option<string>;
UPDATE user SET email_lc = string::concat('duplicate:', <string>id);
FOR $u IN (SELECT id, email FROM user ORDER BY created_at, id) {
    LET $lc = string::lowercase(string::trim($u.email));
    IF array::len((SELECT VALUE id FROM user WHERE email_lc = $lc)) = 0 { UPDATE $u.id SET email_lc = $lc; };
};
DEFINE INDEX OVERWRITE user_email_lc_unique ON user FIELDS email_lc UNIQUE;

-- One owner per org when two signups race for it: owners share the slot `<org>/owner`, members have a
-- slot of their own. Existing rows get member-style slots, so an install that already has two owners keeps them.
DEFINE FIELD OVERWRITE slot ON membership TYPE option<string>;
UPDATE membership SET slot = string::concat(<string>org, '/', <string>user);
DEFINE INDEX OVERWRITE membership_slot_unique ON membership FIELDS slot UNIQUE;
