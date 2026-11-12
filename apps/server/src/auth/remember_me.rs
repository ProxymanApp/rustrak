//! Proxyman: "Remember me for 2 weeks" on the password sign-in.
//!
//! Upstream signs a dashboard session out after a day without a request, and
//! its cookie lasts until the browser closes. A sign-in that ticks "Remember
//! me" keeps its session for the store's whole absolute lifetime (14 days)
//! however idle, and its cookie expires with the session rather than with the
//! browser.
//!
//! - The flag is session state ([`REMEMBER_ME_KEY`]), so it follows the
//!   session through every rotation (`Session::renew`) the way `user_id` does.
//! - `DbSessionStore` copies it to `sessions.remember_me` when it writes a
//!   row, and leaves such a row out of the idle checks.
//! - [`RememberMeCookie`] gives the cookie its expiry, from that row.
//!
//! Kept in one module so a merge from upstream touches as little of theirs as
//! possible. `PROXYMAN.md` lists every place this is wired in.

use std::collections::HashMap;
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;

use actix_session::Session;
use actix_web::{
    body::MessageBody,
    cookie::{
        time::{Duration as CookieDuration, OffsetDateTime},
        Cookie, CookieJar, Key,
    },
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{HeaderValue, SET_COOKIE},
    Error,
};
use chrono::{DateTime, Utc};

use crate::app::SESSION_COOKIE;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};

/// The session state key that marks a remembered sign-in.
pub(crate) const REMEMBER_ME_KEY: &str = "remember_me";

/// Marks the session just signed in as remembered. Call it after
/// `set_user_session`, which rotates the id: the flag goes into the new one.
pub fn remember(session: &Session) -> AppResult<()> {
    session
        .insert(REMEMBER_ME_KEY, true)
        .map_err(|e| AppError::Internal(format!("Failed to set session: {e}")))
}

/// Whether a session's state is a remembered sign-in. Values are JSON, and a
/// `bool` is its own JSON.
pub(crate) fn is_remembered(state: &HashMap<String, String>) -> bool {
    state.get(REMEMBER_ME_KEY).is_some_and(|v| v == "true")
}

/// Gives the session cookie an expiry when the session it carries is a
/// remembered one. Wrapped outside `SessionMiddleware`, so it sees the cookie
/// that middleware set; a response that sets none costs nothing.
///
/// The row, not the request, says whether the session is remembered: the
/// cookie is also set again when a remembered session's state changes later
/// (a GitHub connection in progress, a new password), and those requests
/// carry no checkbox.
#[derive(Clone)]
pub struct RememberMeCookie {
    pool: DbPool,
    key: Key,
}

impl RememberMeCookie {
    pub fn new(pool: DbPool, key: Key) -> Self {
        Self { pool, key }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RememberMeCookie
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = RememberMeCookieService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RememberMeCookieService {
            service: Rc::new(service),
            config: Rc::new(self.clone()),
        }))
    }
}

pub struct RememberMeCookieService<S> {
    service: Rc<S>,
    config: Rc<RememberMeCookie>,
}

impl<S, B> Service<ServiceRequest> for RememberMeCookieService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);
        let config = Rc::clone(&self.config);
        Box::pin(async move {
            let mut res = service.call(req).await?;
            // Best effort: a cookie left without an expiry is a session that
            // ends with the browser, which is upstream's behaviour, not an
            // error worth failing a sign-in over.
            if let Err(e) = persist(&mut res, &config).await {
                log::warn!("remember me: session cookie left as is: {e}");
            }
            Ok(res)
        })
    }
}

/// The session cookie this response sets, if it sets one. A removal (logout)
/// has an empty value and is left alone.
fn session_cookie<B>(res: &ServiceResponse<B>) -> Option<Cookie<'static>> {
    res.response()
        .headers()
        .get_all(SET_COOKIE)
        .filter_map(|v| Cookie::parse_encoded(v.to_str().ok()?.to_owned()).ok())
        .find(|c| c.name() == SESSION_COOKIE && !c.value().is_empty())
}

async fn persist<B>(res: &mut ServiceResponse<B>, config: &RememberMeCookie) -> AppResult<()> {
    let Some(mut cookie) = session_cookie(res) else {
        return Ok(());
    };

    // The cookie is encrypted with the session key (actix-session's default,
    // `CookieContentSecurity::Private`); what it carries is the session id.
    let mut jar = CookieJar::new();
    jar.add_original(cookie.clone());
    let Some(id) = jar.private(&config.key).get(SESSION_COOKIE) else {
        return Ok(());
    };

    let expires_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id_hash = $1 AND remember_me")
            .bind(crate::services::auth_session::hash(id.value()))
            .fetch_optional(&config.pool)
            .await?;
    let Some(expires_at) = expires_at else {
        return Ok(());
    };

    let expires = OffsetDateTime::from_unix_timestamp(expires_at.timestamp())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let remaining = (expires_at - Utc::now()).num_seconds().max(0);
    cookie.set_expires(expires);
    cookie.set_max_age(CookieDuration::seconds(remaining));

    // Encoded the way actix-session wrote it, so it parses back the same.
    let value = HeaderValue::from_str(&cookie.encoded().to_string())
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let response = res.response_mut();
    response.del_cookie(SESSION_COOKIE);
    response.headers_mut().append(SET_COOKIE, value);
    Ok(())
}
