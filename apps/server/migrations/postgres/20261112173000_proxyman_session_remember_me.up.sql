-- Proxyman: "Remember me for 2 weeks" on the password sign-in
-- (src/auth/remember_me.rs). A remembered session is not signed out for being
-- idle: it lasts its whole absolute lifetime, and its cookie expires with it.
-- The version is off upstream's `...000000` grid on purpose, so a migration
-- merged from upstream can never take the same number.
ALTER TABLE sessions ADD COLUMN remember_me BOOLEAN NOT NULL DEFAULT FALSE;
