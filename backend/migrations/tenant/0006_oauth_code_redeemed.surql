-- A redeemed authorization code stays as a short-lived marker so a replay can revoke the grant it produced.
DEFINE FIELD OVERWRITE redeemed_at ON oauth_code TYPE option<datetime>;
DEFINE FIELD OVERWRITE grant_id ON oauth_code TYPE option<record<oauth_grant>>;
