import { releaseQueries } from '@/features/release/api/queries';
import type { OverviewFilters } from './overview-reads';

/**
 * Proxyman: the all-time count behind the "Total users" card (see
 * PROXYMAN.md): the session summary with no window, keeping the page's
 * environment and release. The loader starts it, and it is the page's own
 * summary read when the page is on all time.
 */
export function allTimeUsersRead({
  projectId,
  environment,
  release,
}: OverviewFilters) {
  return releaseQueries.sessionSummary(projectId, { environment, release });
}
