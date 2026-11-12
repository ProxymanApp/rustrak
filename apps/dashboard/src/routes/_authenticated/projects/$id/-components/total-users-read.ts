import { releaseQueries } from '@/features/release/api/queries';
import type { OverviewFilters } from './overview-reads';

/**
 * Proxyman: the all-time count behind the "Total users" card (see
 * PROXYMAN.md): the distinct users with no window, keeping the page's
 * environment and release. The loader starts it with the page's other reads.
 */
export function allTimeUsersRead({
  projectId,
  environment,
  release,
}: OverviewFilters) {
  return releaseQueries.sessionUsers(projectId, { environment, release });
}
