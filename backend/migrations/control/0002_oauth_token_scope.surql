-- A refresh with a narrower `scope` (RFC 6749 section 6) issues tokens limited to that subset.
-- NONE means the token carries the grant's scope.
DEFINE FIELD OVERWRITE scope ON oauth_token TYPE option<array<string>>;
