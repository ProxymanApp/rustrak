//! Proxyman customizations (see PROXYMAN.md at the repository root):
//! "Remember me for 2 weeks" on the password sign-in, and the distinct-user
//! count behind the overview's "Total users" card.
//!
//! Kept out of upstream's test files so a merge from upstream never conflicts
//! with them. The SSO case lives with upstream's fake provider, at the end of
//! `sso_flow_test.rs`.

use std::collections::HashMap;

use crate::common::{create_test_project, create_test_token, seed_user, token_for, TestDb};
use actix_session::storage::SessionStore;
use actix_web::cookie::{time::Duration as CookieDuration, Cookie};
use actix_web::test;
use chrono::{Duration, NaiveDate, Utc};
use rustrak::db::DbPool;
use rustrak::models::UserRole;
use rustrak::services::scope::Scope;
use rustrak::services::session::SessionService;
use rustrak::services::session_users::SessionUsersService;
use rustrak::services::{AuthSessionService, DbSessionStore};
use serde_json::{json, Value};

/// `crate::common::seed_user`'s password.
const PASSWORD: &str = "password123";

macro_rules! app_service {
    () => {
        impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >
    };
}

/// Signs in with the box ticked or not, from a browser that may already hold
/// a session, and returns the new session cookie. Unticked without a cookie
/// is `crate::common::login`.
async fn sign_in(
    app: &app_service!(),
    email: &str,
    remember_me: bool,
    holding: Option<&Cookie<'static>>,
) -> Cookie<'static> {
    let mut req = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": email, "password": PASSWORD, "remember_me": remember_me }));
    if let Some(cookie) = holding {
        req = req.cookie(cookie.clone());
    }
    let resp = test::call_service(app, req.to_request()).await;
    assert_eq!(resp.status(), 200, "login as {email}");
    let cookies: Vec<_> = resp.response().cookies().map(|c| c.into_owned()).collect();
    assert_eq!(cookies.len(), 1, "exactly one Set-Cookie: {cookies:?}");
    cookies.into_iter().next().unwrap()
}

async fn me_status(app: &app_service!(), cookie: &Cookie<'static>) -> u16 {
    let req = test::TestRequest::get()
        .uri("/auth/me")
        .cookie(cookie.clone())
        .to_request();
    test::call_service(app, req).await.status().as_u16()
}

async fn remembered(pool: &DbPool, user_id: i32) -> Vec<bool> {
    sqlx::query_scalar("SELECT remember_me FROM sessions WHERE user_id = $1 ORDER BY created_at")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn idle_for(pool: &DbPool, user_id: i32, idle: Duration) {
    sqlx::query("UPDATE sessions SET last_seen_at = $1 WHERE user_id = $2")
        .bind(Utc::now() - idle)
        .bind(user_id)
        .execute(pool)
        .await
        .unwrap();
}

/// A cookie that lasts two weeks from now, give or take the test's own run.
fn assert_two_weeks(cookie: &Cookie<'_>) {
    let max_age = cookie.max_age().expect("a remembered cookie has a Max-Age");
    assert!(
        max_age > CookieDuration::days(14) - CookieDuration::minutes(1)
            && max_age <= CookieDuration::days(14),
        "Max-Age is two weeks, got {max_age}"
    );
    assert!(cookie.expires_datetime().is_some(), "and an Expires");
}

/// A cookie that ends with the browser.
fn assert_browser_session(cookie: &Cookie<'_>) {
    assert_eq!(cookie.max_age(), None, "ends with the browser");
    assert_eq!(cookie.expires(), None);
}

// ── Remember me ──────────────────────────────────────────────────────────────

#[actix_web::test]
async fn test_login_without_remember_me_keeps_a_browser_session_cookie() {
    let db = TestDb::new().await;
    let user = seed_user(&db.pool, "plain@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;

    // Upstream's sign-in, with no `remember_me` at all, and the box unticked.
    let absent = crate::common::login(&app, "plain@example.com").await;
    let unticked = sign_in(&app, "plain@example.com", false, None).await;
    for cookie in [absent, unticked] {
        assert_browser_session(&cookie);
        assert_eq!(me_status(&app, &cookie).await, 200);
    }
    assert_eq!(remembered(&db.pool, user.id).await, vec![false, false]);
}

#[actix_web::test]
async fn test_login_with_remember_me_sets_a_two_week_cookie() {
    let db = TestDb::new().await;
    let user = seed_user(&db.pool, "remember@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;

    let cookie = sign_in(&app, "remember@example.com", true, None).await;
    assert_two_weeks(&cookie);
    assert!(cookie.http_only().unwrap_or(false), "still httpOnly");
    assert_eq!(cookie.path(), Some("/"));
    assert_eq!(
        me_status(&app, &cookie).await,
        200,
        "the cookie still opens"
    );
    assert_eq!(remembered(&db.pool, user.id).await, vec![true]);
}

/// The browser's previous session was remembered; this sign-in did not tick
/// the box. Renewing keeps the old state, so without the reset in
/// `set_user_session` the new session would inherit the flag.
#[actix_web::test]
async fn test_unticked_sign_in_from_a_remembered_browser_is_not_remembered() {
    let db = TestDb::new().await;
    let first = seed_user(&db.pool, "first@example.com", UserRole::Member).await;
    let second = seed_user(&db.pool, "second@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;
    let remembered_cookie = sign_in(&app, "first@example.com", true, None).await;

    // Someone else at the same browser, and the same person again.
    let other = sign_in(&app, "second@example.com", false, Some(&remembered_cookie)).await;
    assert_browser_session(&other);
    assert_eq!(remembered(&db.pool, second.id).await, vec![false]);

    let again = sign_in(&app, "first@example.com", false, Some(&other)).await;
    assert_browser_session(&again);
    // The first sign-in's row was deleted when `other` rotated it.
    assert_eq!(remembered(&db.pool, first.id).await, vec![false]);
}

#[actix_web::test]
async fn test_invitation_sign_up_from_a_remembered_browser_is_not_remembered() {
    let db = TestDb::new().await;
    seed_user(&db.pool, "owner@example.com", UserRole::Admin).await;
    sqlx::query(
        "INSERT INTO invitations (token, email, role, status, expires_at) \
         VALUES ($1, $2, 'member', 'pending', $3)",
    )
    .bind("proxyman-invite")
    .bind("invitee@example.com")
    .bind(Utc::now() + Duration::days(1))
    .execute(&db.pool)
    .await
    .unwrap();
    let app = crate::common::app(&db.pool).await;
    let remembered_cookie = sign_in(&app, "owner@example.com", true, None).await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/auth/accept-invitation")
            .cookie(remembered_cookie)
            .set_json(json!({ "token": "proxyman-invite", "password": "invitee-password" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 201);
    let cookie = resp.response().cookies().next().unwrap().into_owned();
    let body: Value = test::read_body_json(resp).await;

    assert_browser_session(&cookie);
    let invitee = body["user"]["id"].as_i64().unwrap() as i32;
    assert_eq!(remembered(&db.pool, invitee).await, vec![false]);
}

#[actix_web::test]
async fn test_remembered_session_is_not_signed_out_for_being_idle() {
    let db = TestDb::new().await;
    let user = seed_user(&db.pool, "idle-remember@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = sign_in(&app, "idle-remember@example.com", true, None).await;

    idle_for(&db.pool, user.id, Duration::days(10)).await;
    assert_eq!(me_status(&app, &cookie).await, 200);
}

#[actix_web::test]
async fn test_unremembered_session_is_still_signed_out_after_a_day_idle() {
    let db = TestDb::new().await;
    let user = seed_user(&db.pool, "idle-plain@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = sign_in(&app, "idle-plain@example.com", false, None).await;

    idle_for(&db.pool, user.id, Duration::days(2)).await;
    assert_eq!(me_status(&app, &cookie).await, 401);
}

#[actix_web::test]
async fn test_remembered_session_still_ends_after_two_weeks() {
    let db = TestDb::new().await;
    let user = seed_user(&db.pool, "lifetime@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = sign_in(&app, "lifetime@example.com", true, None).await;

    sqlx::query("UPDATE sessions SET expires_at = $1 WHERE user_id = $2")
        .bind(Utc::now() - Duration::minutes(1))
        .bind(user.id)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(me_status(&app, &cookie).await, 401);
}

#[actix_web::test]
async fn test_purge_keeps_idle_remembered_sessions() {
    let db = TestDb::new().await;
    let user = seed_user(&db.pool, "purge@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;
    let kept = sign_in(&app, "purge@example.com", true, None).await;
    let purged = crate::common::login(&app, "purge@example.com").await;

    idle_for(&db.pool, user.id, Duration::days(3)).await;
    let deleted = AuthSessionService::purge_expired(&db.pool).await.unwrap();

    assert_eq!(deleted, 1, "only the unremembered session goes");
    assert_eq!(remembered(&db.pool, user.id).await, vec![true]);
    assert_eq!(me_status(&app, &kept).await, 200);
    assert_eq!(me_status(&app, &purged).await, 401);
}

#[actix_web::test]
async fn test_rotated_remembered_session_stays_remembered() {
    let db = TestDb::new().await;
    let user = seed_user(&db.pool, "rotate-remember@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = sign_in(&app, "rotate-remember@example.com", true, None).await;

    // A new password rotates the session id; the request carries no checkbox.
    let req = test::TestRequest::post()
        .uri("/auth/me/password")
        .cookie(cookie.clone())
        .set_json(json!({ "current_password": PASSWORD, "new_password": "next-password" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    let renewed = resp.response().cookies().next().unwrap().into_owned();

    assert_ne!(renewed.value(), cookie.value());
    assert_two_weeks(&renewed);
    assert_eq!(remembered(&db.pool, user.id).await, vec![true]);
    assert_eq!(me_status(&app, &renewed).await, 200);
}

#[actix_web::test]
async fn test_rotated_unremembered_session_stays_unremembered() {
    let db = TestDb::new().await;
    let user = seed_user(&db.pool, "rotate-plain@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = crate::common::login(&app, "rotate-plain@example.com").await;

    let req = test::TestRequest::post()
        .uri("/auth/me/password")
        .cookie(cookie)
        .set_json(json!({ "current_password": PASSWORD, "new_password": "next-password" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    let renewed = resp.response().cookies().next().unwrap().into_owned();

    assert_browser_session(&renewed);
    assert_eq!(remembered(&db.pool, user.id).await, vec![false]);
}

/// The column follows the state on every write, not only when a row is made,
/// so the two cannot disagree when the flag changes without a new id.
#[actix_web::test]
async fn test_store_update_keeps_the_column_in_step_with_the_state() {
    let db = TestDb::new().await;
    let store = DbSessionStore::new(db.pool.clone());
    let ttl = CookieDuration::days(1);
    let column = || async {
        sqlx::query_scalar::<_, bool>("SELECT remember_me FROM sessions")
            .fetch_one(&db.pool)
            .await
            .unwrap()
    };

    let key = store.save(HashMap::new(), &ttl).await.unwrap();
    assert!(!column().await);

    let flagged = HashMap::from([("remember_me".to_string(), "true".to_string())]);
    let key = store.update(key, flagged, &ttl).await.unwrap();
    assert!(column().await, "set without a new id");

    store.update(key, HashMap::new(), &ttl).await.unwrap();
    assert!(!column().await, "and cleared");
}

#[actix_web::test]
async fn test_logout_of_a_remembered_session_removes_the_cookie() {
    let db = TestDb::new().await;
    seed_user(&db.pool, "logout-remember@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;
    let cookie = sign_in(&app, "logout-remember@example.com", true, None).await;

    let req = test::TestRequest::post()
        .uri("/auth/logout")
        .cookie(cookie.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204);
    let removal = resp.response().cookies().next().unwrap().into_owned();

    assert_eq!(removal.value(), "", "a removal, not a new expiry");
    assert_eq!(removal.max_age(), Some(CookieDuration::ZERO));
    assert_eq!(me_status(&app, &cookie).await, 401);
}

#[actix_web::test]
async fn test_wrong_password_with_remember_me_is_still_refused() {
    let db = TestDb::new().await;
    seed_user(&db.pool, "wrong@example.com", UserRole::Member).await;
    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({ "email": "wrong@example.com", "password": "nope", "remember_me": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    assert_eq!(resp.response().cookies().count(), 0);
}

// ── Total users ──────────────────────────────────────────────────────────────

async fn seed_device(
    pool: &DbPool,
    project_id: i32,
    release: &str,
    env: &str,
    day: NaiveDate,
    did: &str,
) {
    sqlx::query(
        "INSERT INTO session_users (project_id, release, environment, day, did, crashed) \
         VALUES ($1, $2, $3, $4, $5, FALSE)",
    )
    .bind(project_id)
    .bind(release)
    .bind(env)
    .bind(day)
    .bind(did)
    .execute(pool)
    .await
    .expect("seed session_users");
}

/// Four devices in the project over 60 days, and one in another project.
async fn seed_devices(pool: &DbPool) -> i32 {
    let project_id = create_test_project(pool, "Users Project").await.id;
    let other_project = create_test_project(pool, "Other Project").await.id;
    let today = Utc::now().date_naive();
    let days_ago = |days| today - Duration::days(days);

    // The same device across releases, environments and days is one user.
    seed_device(pool, project_id, "1.0", "prod", today, "device-a").await;
    seed_device(pool, project_id, "2.0", "prod", today, "device-a").await;
    seed_device(pool, project_id, "2.0", "beta", days_ago(1), "device-a").await;
    seed_device(pool, project_id, "2.0", "prod", today, "device-b").await;
    seed_device(pool, project_id, "1.0", "beta", days_ago(2), "device-c").await;
    // Older than any window but all time.
    seed_device(pool, project_id, "0.9", "prod", days_ago(60), "device-d").await;
    // Another project's devices are not this one's.
    seed_device(pool, other_project, "1.0", "prod", today, "device-z").await;
    project_id
}

/// The users-only count and the summary's agree, for every scope the card
/// asks for.
#[actix_web::test]
async fn test_both_counts_count_each_device_once() {
    let db = TestDb::new().await;
    let project_id = seed_devices(&db.pool).await;
    let prod = Scope {
        environment: Some("prod"),
        release: None,
    };
    let release = Scope {
        environment: None,
        release: Some("2.0"),
    };

    for (period, scope, expected, what) in [
        (None, Scope::default(), 4, "all time"),
        (Some(14 * 24), Scope::default(), 3, "the window applies"),
        (None, prod, 3, "the environment filter applies"),
        (None, release, 2, "the release filter applies"),
    ] {
        let users = SessionUsersService::distinct_users(&db.pool, project_id, period, scope)
            .await
            .unwrap();
        let summary = SessionService::project_summary(&db.pool, project_id, period, scope)
            .await
            .unwrap();
        assert_eq!(users, expected, "{what}");
        assert_eq!(summary.users, expected, "{what}, in the summary");
    }

    let empty = create_test_project(&db.pool, "No Sessions").await.id;
    let none = SessionUsersService::distinct_users(&db.pool, empty, None, Scope::default())
        .await
        .unwrap();
    assert_eq!(none, 0);
}

#[actix_web::test]
async fn test_users_endpoint_counts_all_time_or_a_window() {
    let db = TestDb::new().await;
    let project_id = seed_devices(&db.pool).await;
    let token = create_test_token(&db.pool).await;
    let app = crate::common::app(&db.pool).await;

    for (query, expected) in [("", 4), ("?period=14d", 3), ("?environment=prod", 3)] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/projects/{project_id}/sessions/users{query}"))
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "{query}");
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body, json!({ "users": expected }), "{query}");
    }

    // The summary carries the same count, for the card's period mode.
    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{project_id}/sessions/summary?period=14d"
        ))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["users"], 3);
}

#[actix_web::test]
async fn test_users_endpoint_is_hidden_from_people_without_the_project() {
    let db = TestDb::new().await;
    let project_id = seed_devices(&db.pool).await;
    let stranger = seed_user(&db.pool, "stranger@example.com", UserRole::Member).await;
    let token = token_for(&db.pool, stranger.id).await;
    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/projects/{project_id}/sessions/users"))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 404);
}
