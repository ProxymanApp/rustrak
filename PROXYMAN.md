# Proxyman's Rustrak

This repository (`ProxymanApp/rustrak`) is a fork of
[`rustrak/rustrak`](https://github.com/rustrak/rustrak) with a few changes
Proxyman needs. They are **not** sent upstream: pull requests go to this
repository only, never to `rustrak/rustrak`.

Everything here is written to survive merges from upstream: new behaviour lives
in new files, and every line changed in an upstream file carries a `Proxyman:`
comment, so

```bash
git grep -n "Proxyman"
```

lists every place a merge can conflict with us.

## The customizations

### 1. "Remember me for 2 weeks" on the login page

Upstream signs a dashboard session out after a day without a request, and its
cookie ends when the browser closes. Ticking **Remember me for 2 weeks** keeps
the session for the store's full 14-day lifetime however idle, and gives the
cookie the same expiry, so a browser restart does not sign out either.
Unticked, sign-in is exactly upstream's.

- The flag is session state (`remember_me`). **Every sign-in clears it**
  (`set_user_session` calls `remember_me::forget`), and `login` sets it again
  only when the box is ticked. Renewing a session keeps its state, so without
  that reset an unticked, SSO or invitation sign-in on a browser that still
  held a remembered session would inherit it. A new password rotates the id
  without signing in, so `change_password` carries the flag across.
- The store writes the flag to `sessions.remember_me` on every write (`save`
  and `update`) and skips the idle checks for such a row. The 14-day absolute
  lifetime still applies.
- `RememberMeCookie`, a middleware wrapped outside actix-session's, gives the
  cookie `Max-Age` and `Expires` from that row whenever the session cookie is
  set. A cookie it cannot decrypt is logged, not skipped quietly.
- The checkbox is on the password form only; SSO sign-ins are never
  remembered.

| | Files |
|---|---|
| New | `apps/server/src/auth/remember_me.rs`, `apps/server/migrations/{sqlite,postgres}/20261112173000_proxyman_session_remember_me.*`, `apps/dashboard/src/routes/-components/remember-me-field.tsx` |
| Touched | `apps/server/src/auth/mod.rs`, `apps/server/src/auth/session.rs` (`set_user_session` clears the flag), `apps/server/src/app.rs` (the `.wrap`), `apps/server/src/services/auth_session.rs` (load, purge, save, update), `apps/server/src/models/user.rs` (`LoginRequest.remember_me`), `apps/server/src/routes/auth.rs` (`login`, `change_password`), `apps/dashboard/src/routes/-components/login-form.tsx` |

### 2. "Total users" on the project overview

A KPI card on the overview: the distinct devices that sent sessions (the
sessions' `did`; Sentry's Cocoa SDK fills it with the installation id when the
app sets no user).

- **Two modes**, switched on the card: **All** (the default, the project's
  total) and the page's period (`14d` and so on). Both keep the page's
  environment and release filters; only the window differs. With the page on
  *All time* the two are the same count and the switch is hidden.
- **The period count** comes with the session summary the crash-free cards
  already read: it counted those users for the crash-free rate and now also
  returns them (`SessionSummary.users`).
- **The all-time count** is its own endpoint,
  `GET /api/projects/{id}/sessions/users` (`client.sessions.getUsers`). It
  counts `session_users` only, so the card's default does not also sum every
  `session_counts` bucket the project has. The loader starts it with the
  page's other reads.
- **No index for it, on purpose.** One on `(project_id, did)` makes the
  all-time count about 13× faster, but SQLite's planner then picks it for
  windowed counts too, upstream's summary among them, and those got about 8×
  slower in a 2M-row test.
- With the page on a period that has no sessions, the crash-free note says
  "No sessions in this period" rather than upstream's "No sessions yet",
  because the card beside it may be counting all time.
- **Layout**: a row of its own under the four upstream cards, joining their
  row as a fifth card only from 1700 px wide. Any narrower, five columns push
  the crash-free sessions sparkline out of its card.

| | Files |
|---|---|
| New | `apps/server/src/services/session_users.rs`, `apps/server/src/routes/session_users.rs`, `apps/dashboard/src/routes/_authenticated/projects/$id/-components/total-users-kpi.tsx`, `…/-components/total-users-read.ts` |
| Touched | `apps/server/src/models/session.rs`, `apps/server/src/services/session.rs`, `apps/server/src/services/mod.rs`, `apps/server/src/routes/mod.rs`, `apps/server/src/openapi.rs`, `apps/dashboard/src/features/release/api/queries.ts` (`sessionUsers`), `apps/dashboard/src/routes/_authenticated/projects/$id/-components/overview-panels.tsx` (exports `Kpi` and `KpiPlaceholder`; the `extra` slot on `KpiRow` with its layout, `ROW_WITH_EXTRA` and `EXTRA_SLOT`; the note's window wording), `apps/dashboard/src/routes/_authenticated/projects/$id/index.tsx` (the card, and the all-time read in the loader) |

### Shared by both

- Copy: a top-level `proxyman` namespace at the top of each
  `apps/dashboard/src/shared/i18n/messages/*.json` (all five locales).
- Tests: `apps/server/tests/integration/proxyman_test.rs`, registered with one
  line in `tests/integration/mod.rs`, and one SSO test appended to the end of
  `tests/integration/sso_flow_test.rs` (it needs that file's fake provider).
- Generated: `apps/server/openapi.json`, `packages/client/src/generated/*`, and
  the API reference in `packages/client/README.md` and
  `apps/docs/content/sdks/client.mdx` carry the new fields and the new
  endpoint.

## Merging from upstream

```bash
git remote add upstream https://github.com/rustrak/rustrak.git   # once
git fetch upstream
git switch -c sync/upstream-$(date +%Y%m%d) main
git merge upstream/main
```

When it conflicts:

- **Generated files** (`openapi.json`, `packages/client/src/generated/*`, and
  the API reference in the client README and docs):
  take upstream's version, then regenerate rather than merging by hand:
  ```bash
  (cd apps/server && cargo run --bin gen_openapi --features openapi)
  pnpm --filter @rustrak/client generate
  ```
- **A `Proxyman:` line**: keep upstream's change and re-apply ours on top. The
  sections above say what each one is for.
- **Migrations**: ours use off-grid versions (`...173000`, never upstream's
  `...000000` or `...000001`), so a migration merged from upstream cannot take
  the same number. sqlx applies a migration it has not seen whatever its
  place in the order. Keep that rule for any migration added here.
- **Versions and changesets**: leave upstream's alone. These changes add no
  changeset, so a merge never fights over the release number.

Then run what CI runs: `pnpm run ci` (or at least `cargo test` in
`apps/server` and `pnpm test --filter=@rustrak/dashboard`). The tests in
`proxyman_test.rs` fail if a merge quietly drops either customization's
server side.
