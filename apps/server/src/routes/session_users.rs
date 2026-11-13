//! Proxyman: `GET /api/projects/{project_id}/sessions/users`, the count
//! behind the overview's "Total users" card (see PROXYMAN.md).

use actix_web::{web, HttpResponse};
use serde::Serialize;

use crate::auth::ApiActor;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::rbac::{Action, Resource};
use crate::routes::period::parse_period_hours;
use crate::routes::sessions::SummaryQuery;
use crate::services::scope::Scope;
use crate::services::session_users::SessionUsersService;

/// How many distinct users sent sessions.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionUsers {
    /// Distinct users (the sessions' `did`, which SDKs fill with a device's
    /// installation id when no user is set) seen in the window.
    pub users: i64,
}

/// Count the distinct users who sent a project sessions.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/projects/{project_id}/sessions/users",
    operation_id = "sessions.getUsers",
    tag = "Sessions",
    params(
        ("project_id" = i32, Path, description = "Project ID"),
        ("period" = Option<String>, Query, description = "Time window, e.g. '24h', '7d'. Omit for all time."),
        ("environment" = Option<String>, Query, description = "Only sessions from this environment. Omit for every environment."),
        ("release" = Option<String>, Query, description = "Only sessions from this release. Omit for every release."),
    ),
    responses(
        (status = 200, description = "Distinct users of the project's sessions", body = SessionUsers),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = [])),
))]
pub async fn get_users(
    pool: web::Data<DbPool>,
    path: web::Path<i32>,
    query: web::Query<SummaryQuery>,
    actor: ApiActor,
) -> AppResult<HttpResponse> {
    let project_id = path.into_inner();

    actor.require(Resource::Sessions, Action::Read, Some(project_id))?;

    let users = SessionUsersService::distinct_users(
        pool.get_ref(),
        project_id,
        parse_period_hours(query.period.as_deref()),
        Scope {
            environment: query.environment.as_deref(),
            release: query.release.as_deref(),
        },
    )
    .await?;

    Ok(HttpResponse::Ok().json(SessionUsers { users }))
}

pub fn configure_project(cfg: &mut web::ServiceConfig) {
    cfg.route("/sessions/users", web::get().to(get_users));
}
