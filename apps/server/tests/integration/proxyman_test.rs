//! Proxyman customizations (see PROXYMAN.md at the repository root):
//! "Remember me for 2 weeks" on the password sign-in, and the distinct-user
//! count on the project's session summary.
//!
//! Kept out of upstream's test files so a merge from upstream never conflicts
//! with them.

use crate::common::{create_test_project, create_test_token, TestDb};
use actix_web::cookie::{time::Duration as CookieDuration, Cookie};
use actix_web::test;
use chrono::{Duration, NaiveDate, Utc};
use rustrak::db::DbPool;
use rustrak::models::{CreateUserRequest, User, UserRole};
use rustrak::services::scope::Scope;
use rustrak::services::session::SessionService;
use rustrak::services::{AuthSessionService, UsersService};
use serde_json::{json, Value};

const PASSWORD: &str = "password123";

async fn create_user(pool: &DbPool, email: &str) -> User {
    UsersService::create_user(
        pool,
        &CreateUserRequest {
            email: email.to_string(),
            password: PASSWORD.to_string(),
        },
        UserRole::Member,
    )
    .await
    .expect("create user")
}

macro_rules! app_service {
    () => {
        impl actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >
    };
}

/// Signs in, ticking "Remember me" or not, and returns the session cookie.
async fn login(app: &app_service!(), email: &str, remember_me: Option<bool>) -> Cookie<'static> {
    let mut body = json!({ "email": email, "password": PASSWORD });
    if let Some(remember_me) = remember_me {
        body["remember_me"] = json!(remember_me);
    }
    let req = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(body)
        .to_request();
    let resp = test::call_service(app, req).await;
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

// ── Remember me ──────────────────────────────────────────────────────────────

#[actix_web::test]
async fn test_login_without_remember_me_keeps_a_browser_session_cookie() {
    let db = TestDb::new().await;
    let user = create_user(&db.pool, "plain@example.com").await;
    let app = crate::common::app(&db.pool).await;

    for remember_me in [None, Some(false)] {
        let cookie = login(&app, "plain@example.com", remember_me).await;
        assert_eq!(cookie.max_age(), None, "ends with the browser");
        assert_eq!(cookie.expires(), None);
        assert_eq!(me_status(&app, &cookie).await, 200);
    }
    assert_eq!(remembered(&db.pool, user.id).await, vec![false, false]);
}

#[actix_web::test]
async fn test_login_with_remember_me_sets_a_two_week_cookie() {
    let db = TestDb::new().await;
    let user = create_user(&db.pool, "remember@example.com").await;
    let app = crate::common::app(&db.pool).await;

    let cookie = login(&app, "remember@example.com", Some(true)).await;
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

#[actix_web::test]
async fn test_remembered_session_is_not_signed_out_for_being_idle() {
    let db = TestDb::new().await;
    let user = create_user(&db.pool, "idle-remember@example.com").await;
    let app = crate::common::app(&db.pool).await;
    let cookie = login(&app, "idle-remember@example.com", Some(true)).await;

    idle_for(&db.pool, user.id, Duration::days(10)).await;
    assert_eq!(me_status(&app, &cookie).await, 200);
}

#[actix_web::test]
async fn test_unremembered_session_is_still_signed_out_after_a_day_idle() {
    let db = TestDb::new().await;
    let user = create_user(&db.pool, "idle-plain@example.com").await;
    let app = crate::common::app(&db.pool).await;
    let cookie = login(&app, "idle-plain@example.com", Some(false)).await;

    idle_for(&db.pool, user.id, Duration::days(2)).await;
    assert_eq!(me_status(&app, &cookie).await, 401);
}

#[actix_web::test]
async fn test_remembered_session_still_ends_after_two_weeks() {
    let db = TestDb::new().await;
    let user = create_user(&db.pool, "lifetime@example.com").await;
    let app = crate::common::app(&db.pool).await;
    let cookie = login(&app, "lifetime@example.com", Some(true)).await;

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
    let user = create_user(&db.pool, "purge@example.com").await;
    let app = crate::common::app(&db.pool).await;
    let kept = login(&app, "purge@example.com", Some(true)).await;
    let purged = login(&app, "purge@example.com", None).await;

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
    let user = create_user(&db.pool, "rotate-remember@example.com").await;
    let app = crate::common::app(&db.pool).await;
    let cookie = login(&app, "rotate-remember@example.com", Some(true)).await;

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
async fn test_logout_of_a_remembered_session_removes_the_cookie() {
    let db = TestDb::new().await;
    create_user(&db.pool, "logout-remember@example.com").await;
    let app = crate::common::app(&db.pool).await;
    let cookie = login(&app, "logout-remember@example.com", Some(true)).await;

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
    create_user(&db.pool, "wrong@example.com").await;
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

async fn seed_user(
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

#[actix_web::test]
async fn test_summary_counts_each_device_once() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Users Project").await.id;
    let other_project = create_test_project(&db.pool, "Other Project").await.id;
    let today = Utc::now().date_naive();

    // The same device across releases, environments and days is one user.
    seed_user(&db.pool, project_id, "1.0", "prod", today, "device-a").await;
    seed_user(&db.pool, project_id, "2.0", "prod", today, "device-a").await;
    seed_user(
        &db.pool,
        project_id,
        "2.0",
        "beta",
        today - Duration::days(1),
        "device-a",
    )
    .await;
    seed_user(&db.pool, project_id, "2.0", "prod", today, "device-b").await;
    seed_user(
        &db.pool,
        project_id,
        "1.0",
        "beta",
        today - Duration::days(2),
        "device-c",
    )
    .await;
    // Older than any window but all time.
    seed_user(
        &db.pool,
        project_id,
        "0.9",
        "prod",
        today - Duration::days(60),
        "device-d",
    )
    .await;
    // Another project's devices are not this one's.
    seed_user(&db.pool, other_project, "1.0", "prod", today, "device-z").await;

    let all_time = SessionService::project_summary(&db.pool, project_id, None, Scope::default())
        .await
        .unwrap();
    assert_eq!(all_time.users, 4);

    let two_weeks =
        SessionService::project_summary(&db.pool, project_id, Some(14 * 24), Scope::default())
            .await
            .unwrap();
    assert_eq!(two_weeks.users, 3, "the window applies");

    let prod = SessionService::project_summary(
        &db.pool,
        project_id,
        None,
        Scope {
            environment: Some("prod"),
            release: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(prod.users, 3, "the environment filter applies");

    let empty = create_test_project(&db.pool, "No Sessions").await.id;
    let none = SessionService::project_summary(&db.pool, empty, None, Scope::default())
        .await
        .unwrap();
    assert_eq!(none.users, 0);
}

#[actix_web::test]
async fn test_summary_endpoint_sends_users() {
    let db = TestDb::new().await;
    let project_id = create_test_project(&db.pool, "Users Endpoint").await.id;
    let token = create_test_token(&db.pool).await;
    let today = Utc::now().date_naive();
    seed_user(&db.pool, project_id, "1.0", "prod", today, "device-a").await;
    seed_user(&db.pool, project_id, "1.0", "prod", today, "device-b").await;
    let app = crate::common::app(&db.pool).await;

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/projects/{project_id}/sessions/summary?period=14d"
        ))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["users"], 2);
}
