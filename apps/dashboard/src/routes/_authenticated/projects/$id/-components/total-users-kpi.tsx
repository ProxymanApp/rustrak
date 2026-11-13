import { SegmentedControl, SegmentedItem, Skeleton } from '@rustrak/ui';
import { useQuery } from '@tanstack/react-query';
import { type ReactNode, useState } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { Kpi, KpiPlaceholder } from './overview-panels';
import type { OverviewFilters, OverviewReads } from './overview-reads';
import { allTimeUsersRead } from './total-users-read';

/**
 * Proxyman: "Total users" on the overview's KPI row (see PROXYMAN.md).
 *
 * The distinct devices that sent sessions. Sentry's Cocoa SDK fills a
 * session's user with the device's installation id when the app sets none,
 * which is why the card counts devices.
 *
 * Two modes, all time (the default) and the page's period. Both keep the
 * page's environment and release: only the window differs. With the page on
 * all time the two are the same count, so there is nothing to switch. The
 * all-time count is its own small read (`allTimeUsersRead`); the period's
 * comes with the session summary the crash-free cards already read.
 *
 * The mode is the card's own, like the issues panel's view: a glance at
 * another number, not a filter, so it stays out of the address. Where the
 * card sits is `KpiRow`'s to decide (its `extra` slot).
 */
type Mode = 'all' | 'period';

export function TotalUsersKpi({
  reads,
  filters,
  held = false,
}: {
  reads: OverviewReads;
  filters: OverviewFilters;
  /** The page's skeleton: draw the placeholder and ask for nothing. */
  held?: boolean;
}) {
  const t = useTranslations('proxyman');
  const overview = useTranslations('projectPages');
  const format = useFormatter();
  const [mode, setMode] = useState<Mode>('all');

  const allTime = useQuery({
    ...allTimeUsersRead(filters),
    enabled: !held,
    subscribed: !held,
  });
  const inPeriod = useQuery(reads.sessionSummary);

  const period = filters.period;
  const showingPeriod = period !== undefined && mode === 'period';
  const showing = showingPeriod ? inPeriod : allTime;
  const users = showing.data?.users;
  // Failed only with nothing to show, as the cards beside it: a background
  // refetch that fails leaves the last count on screen. A failed period read
  // with nothing cached is the row's to show, which replaces every card.
  const failed = users === undefined && showing.isError;

  // The card appears with the first count to land; until the one it shows
  // is in, its figure is a skeleton rather than the card.
  if (allTime.data === undefined && inPeriod.data === undefined && !failed) {
    return <KpiPlaceholder />;
  }

  let value: ReactNode;
  if (failed) {
    value = '–';
  } else if (users === undefined) {
    value = <Skeleton className="h-8 w-20" />;
  } else if (users === 0) {
    // No device counted says nothing about users: a dash, as the crash-free
    // cards show with no sessions, rather than a zero that reads as a fact.
    value = '–';
  } else {
    value = format.number(users);
  }

  let footnote: string;
  if (failed) {
    footnote = t('loadFailed');
  } else if (showingPeriod) {
    footnote = t('uniqueDevicesIn', {
      period: overview(`overview.periods.${period}`),
    });
  } else {
    footnote = t('uniqueDevices');
  }

  return (
    <Kpi
      label={t('totalUsers')}
      value={value}
      delta={
        period ? (
          <SegmentedControl
            aria-label={t('windowLabel')}
            value={mode}
            onValueChange={(next) => setMode(next as Mode)}
            // Held into the label's line, so the card stays the height of
            // the cards beside it.
            className="-my-1.5"
          >
            <SegmentedItem value="all">
              {overview('overview.periods.allShort')}
            </SegmentedItem>
            <SegmentedItem value="period">{period}</SegmentedItem>
          </SegmentedControl>
        ) : null
      }
      footnote={footnote}
    />
  );
}
