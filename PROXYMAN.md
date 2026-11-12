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

- The flag is session state (`remember_me`), so it follows the session through
  every id rotation (a new password, for one).
- The store copies it to `sessions.remember_me` and skips the idle checks for
  such a row; the 14-day absolute lifetime still applies.
- `RememberMeCookie`, a middleware wrapped outside actix-session's, gives the
  cookie `Max-Age` and `Expires` from that row whenever the session cookie is
  set.
- Password sign-in only. SSO sign-ins keep upstream's behaviour.

| | Files |
|---|---|
| New | `apps/server/src/auth/remember_me.rs`, `apps/server/migrations/{sqlite,postgres}/20261112173000_proxyman_session_remember_me.*`, `apps/dashboard/src/routes/-components/remember-me-field.tsx` |
| Touched | `apps/server/src/auth/mod.rs`, `apps/server/src/app.rs` (the `.wrap`), `apps/server/src/services/auth_session.rs` (load, purge, save), `apps/server/src/models/user.rs` (`LoginRequest.remember_me`), `apps/server/src/routes/auth.rs` (`login`), `apps/dashboard/src/routes/-components/login-form.tsx` |

### 2. "Total users" on the project overview

A KPI card on the overview: the distinct devices that sent sessions (the
sessions' `did`; Sentry's Cocoa SDK fills it with the installation id when the
app sets no user). The session summary already counted those users for the
crash-free rate; it now also returns the count (`SessionSummary.users`).

- **Two modes**, switched on the card: **All** (the default, the project's
  total) and the page's period (`14d` and so on). Both keep the page's
  environment and release filters; only the window differs. With the page on
  *All time* the two are the same count and the switch is hidden.
- The all-time count is the session summary with no `period`. The loader
  starts it with the page's other reads. It counts every `session_users` row of
  the project, which is what the page already does when *All time* is picked.
- **Layout**: a row of its own under the four upstream cards, joining their
  row as a fifth card only from 1700 px wide. Any narrower, five columns push
  the crash-free sessions sparkline out of its card.

| | Files |
|---|---|
| New | `apps/dashboard/src/routes/_authenticated/projects/$id/-components/total-users-kpi.tsx`, `…/-components/total-users-read.ts` |
| Touched | `apps/server/src/models/session.rs`, `apps/server/src/services/session.rs`, `apps/dashboard/src/routes/_authenticated/projects/$id/-components/overview-panels.tsx` (exports `Kpi` and `KpiPlaceholder`, an `extra` slot on `KpiRow`, five columns from 1700 px), `apps/dashboard/src/routes/_authenticated/projects/$id/index.tsx` (the card, and the all-time read in the loader) |

### Shared by both

- Copy: a top-level `proxyman` namespace at the top of each
  `apps/dashboard/src/shared/i18n/messages/*.json` (all five locales).
- Tests: `apps/server/tests/integration/proxyman_test.rs`, registered with one
  line in `tests/integration/mod.rs`.
- Generated: `apps/server/openapi.json` and
  `packages/client/src/generated/*` carry the two new fields.

## Merging from upstream

```bash
git remote add upstream https://github.com/rustrak/rustrak.git   # once
git fetch upstream
git switch -c sync/upstream-$(date +%Y%m%d) main
git merge upstream/main
```

When it conflicts:

- **Generated files** (`openapi.json`, `packages/client/src/generated/*`):
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
