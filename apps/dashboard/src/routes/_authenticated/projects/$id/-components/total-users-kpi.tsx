import { useQuery } from '@tanstack/react-query';
import { useFormatter, useTranslations } from 'use-intl';
import { Kpi, KpiPlaceholder } from './overview-panels';
import type { OverviewReads } from './overview-reads';

/**
 * Proxyman: "Total users" on the overview's KPI row (see PROXYMAN.md).
 *
 * The distinct devices that sent sessions, from the session summary the
 * crash-free cards already read (`SessionSummary.users`), so it follows the
 * page's window, environment and release like every card beside it: "All
 * time" is the project's total. Sentry's Cocoa SDK fills a session's user
 * with the device's installation id when the app sets none, which is why the
 * card counts devices.
 *
 * On a phone it takes the row's full width, so five cards in two columns
 * leave no hole.
 */
export function TotalUsersKpi({ reads }: { reads: OverviewReads }) {
  const t = useTranslations('proxyman');
  const format = useFormatter();
  // A failed read is the row's to show: it replaces every card with one
  // failure, this one included.
  const health = useQuery(reads.sessionSummary);

  return (
    <div className="col-span-2 grid xl:col-span-1">
      {health.data ? (
        <Kpi
          label={t('totalUsers')}
          // No sessions at all says nothing about users: a dash, as the
          // crash-free cards show, rather than a zero that reads as a fact.
          value={
            health.data.total === 0 ? '–' : format.number(health.data.users)
          }
          footnote={t('uniqueDevices')}
        />
      ) : (
        <KpiPlaceholder />
      )}
    </div>
  );
}
