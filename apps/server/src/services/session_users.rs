//! Proxyman: the distinct-user count behind the overview's "Total users" card
//! (see PROXYMAN.md).
//!
//! The session summary returns the same count, but it also aggregates every
//! `session_counts` bucket in the window and counts crashed users. The card
//! opens on all time, where that is the heaviest read the page makes, so it
//! asks for this one alone.
//!
//! No index of its own: one on `(project_id, did)` makes the all-time count an
//! ordered index scan, but SQLite's planner then prefers it for windowed
//! counts too (the session summary's), which it makes several times slower.

use crate::db::dialect::since_day;
use crate::db::DbPool;
use crate::error::AppResult;
use crate::services::scope::Scope;

pub struct SessionUsersService;

impl SessionUsersService {
    /// Distinct users (the sessions' `did`) of a project in the window, or of
    /// all time when `period_hours` is `None`.
    pub async fn distinct_users(
        pool: &DbPool,
        project_id: i32,
        period_hours: Option<i64>,
        scope: Scope<'_>,
    ) -> AppResult<i64> {
        let sql = format!(
            "SELECT COUNT(DISTINCT did) FROM session_users \
             WHERE project_id = $1 \
               AND ($2 IS NULL OR environment = $2) \
               AND ($3 IS NULL OR release = $3) \
               AND {window}",
            window = since_day("day", period_hours),
        );
        let users: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(&*sql))
            .bind(project_id)
            .bind(scope.environment)
            .bind(scope.release)
            .fetch_one(pool)
            .await?;
        Ok(users)
    }
}
