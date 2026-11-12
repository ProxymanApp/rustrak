/**
 * Reads for the release feature.
 *
 * Releases and sessions are one slice because "release health" *is* sessions
 * grouped by release: the two were never separate concepts, only separate
 * endpoints.
 */
import type {
  ReleaseHealthRow,
  ReleasesGetData,
  ReleasesListData,
  ReleasesNewIssuesData,
  SessionsGetSummaryData,
  SessionsGetTimeseriesData,
  // Proxyman: the "Total users" count.
  SessionsGetUsersData,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

type ReleasesListQuery = NonNullable<ReleasesListData['query']>;
type ReleaseQuery = NonNullable<ReleasesGetData['query']>;
type NewIssuesQuery = NonNullable<ReleasesNewIssuesData['query']>;
type SessionSummaryQuery = NonNullable<SessionsGetSummaryData['query']>;
type SessionTimeseriesQuery = NonNullable<SessionsGetTimeseriesData['query']>;
// Proxyman: the "Total users" count.
type SessionUsersQuery = NonNullable<SessionsGetUsersData['query']>;

/** Page size used when walking every row of a single release. */
const RELEASE_ROWS_PER_PAGE = 100;

// Backstop on a bound the server supplies. At 100 rows a page this still
// allows 2000 release/environment rows, which is far past any real project.
const MAX_RELEASE_PAGES = 20;

export const releaseQueries = {
  all: (projectId: number) => [...projectScope(projectId), 'releases'] as const,
  /**
   * One page of a project's releases, newest first, each with its events, new
   * issues, environments and session health in the window.
   */
  list: (projectId: number, query: ReleasesListQuery) =>
    queryOptions({
      queryKey: [...releaseQueries.all(projectId), 'list', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().releases.list({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /** One release, with the same figures as its row in the list. */
  detail: (projectId: number, release: string, query: ReleaseQuery) =>
    queryOptions({
      queryKey: [...releaseQueries.all(projectId), 'detail', release, query],
      queryFn: ({ signal }) =>
        orThrow(
          api().releases.get({
            path: { project_id: projectId, release },
            query,
            signal,
          }),
        ),
    }),
  /**
   * Issues first seen in a release, most recently introduced first.
   *
   * A failure fails the read. An empty array renders "no new issues
   * introduced in this release", which is a claim about the release; a fetch
   * that failed makes no claim at all, and the two must not look the same.
   */
  newIssues: (projectId: number, release: string, query: NewIssuesQuery = {}) =>
    queryOptions({
      queryKey: [
        ...releaseQueries.all(projectId),
        'new-issues',
        release,
        query,
      ],
      queryFn: ({ signal }) =>
        orThrow(
          api().releases.newIssues({
            path: { project_id: projectId, release },
            query,
            signal,
          }),
        ),
    }),
  /**
   * Every health row for one release, across all its environments.
   *
   * The release detail page needs the complete set: a row missing from the
   * response renders an environment's cards blank even though the list linked
   * to it. One page holds every realistic environment count, so the loop
   * normally runs once, but it keeps going when a project has more.
   *
   * A page that fails fails the read. Returning the rows gathered so far would
   * be the same silent truncation the complete set exists to avoid.
   *
   * `total_pages` comes from the response body, so the loop is bounded by
   * something the server controls. `MAX_RELEASE_PAGES` is the backstop: a
   * server bug reporting an inflated count would otherwise hold the page open
   * for as many sequential round-trips as it cared to name.
   */
  healthRows: (projectId: number, release: string, period?: string) =>
    queryOptions({
      queryKey: [
        ...releaseQueries.all(projectId),
        'health-rows',
        release,
        { period },
      ],
      queryFn: async ({ signal }): Promise<ReleaseHealthRow[]> => {
        const rows: ReleaseHealthRow[] = [];
        let page = 1;
        let totalPages = 1;
        do {
          const response = await orThrow(
            api().sessions.getStats({
              path: { project_id: projectId },
              query: { release, period, page, per_page: RELEASE_ROWS_PER_PAGE },
              signal,
            }),
          );
          rows.push(...response.items);
          totalPages = Math.min(response.total_pages, MAX_RELEASE_PAGES);
          page += 1;
        } while (page <= totalPages);
        return rows;
      },
    }),
  /**
   * Session health aggregated across the releases and environments the query
   * leaves open.
   *
   * A failure fails the read. A zeroed summary is the one fallback that cannot
   * be spotted by reading a render: zero sessions, zero crashed, zero abnormal
   * is a real window a healthy project can have, so an outage would render as
   * a confident "nothing is wrong".
   */
  sessionSummary: (projectId: number, query: SessionSummaryQuery) =>
    queryOptions({
      queryKey: [
        ...releaseQueries.all(projectId),
        'sessions',
        'summary',
        query,
      ],
      queryFn: ({ signal }) =>
        orThrow(
          api().sessions.getSummary({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /**
   * A time-bucketed session trend. A failure fails the read for the same
   * reason as `sessionSummary`: an empty series says "no sessions in this
   * window", which an unreachable API is in no position to say.
   */
  sessionTimeseries: (projectId: number, query: SessionTimeseriesQuery) =>
    queryOptions({
      queryKey: [
        ...releaseQueries.all(projectId),
        'sessions',
        'timeseries',
        query,
      ],
      queryFn: ({ signal }) =>
        orThrow(
          api().sessions.getTimeseries({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /**
   * Proxyman: the distinct users of a project's sessions, for the overview's
   * "Total users" card (see PROXYMAN.md). The summary carries the same count;
   * this read is only that count, so its all-time default does not also sum
   * every session bucket the project has.
   */
  sessionUsers: (projectId: number, query: SessionUsersQuery) =>
    queryOptions({
      queryKey: [...releaseQueries.all(projectId), 'sessions', 'users', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().sessions.getUsers({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
};
