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
 * The distinct devices that sent sessions (`SessionSummary.users`). Sentry's
 * Cocoa SDK fills a session's user with the device's installation id when the
 * app sets none, which is why the card counts devices.
 *
 * Two modes, all time (the default) and the page's period. Both keep the
 * page's environment and release: only the window differs. With the page on
 * all time the two are the same count, so there is nothing to switch.
 *
 * The mode is the card's own, like the issues panel's view: a glance at
 * another number, not a filter, so it stays out of the address.
 *
 * It takes a row of its own under the four cards, and joins their row only
 * from 1700 px: a fifth column any narrower squeezes the crash-free card's
 * sparkline out of its card, and this card's label behind its toggle.
 */
type Mode = 'all' | 'period';

/** Its own row, until the row is wide enough for a fifth card. */
// The ranges do not overlap: Tailwind emits `min-[...]` before `xl:`.
const SLOT =
  'col-span-2 grid xl:max-[1700px]:col-span-4 min-[1700px]:col-span-1';

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
  // A failed period read is the row's to show: it replaces every card with
  // one failure, this one included.
  const inPeriod = useQuery(reads.sessionSummary);

  const period = filters.period;
  const showing = period && mode === 'period' ? inPeriod : allTime;

  // The card appears with the first count to land; until the one it shows
  // is in, its figure is a skeleton rather than the card.
  if (!allTime.data && !inPeriod.data && !showing.isError) {
    return (
      <div className={SLOT}>
        <KpiPlaceholder />
      </div>
    );
  }

  let value: ReactNode;
  if (showing.isError) {
    value = '–';
  } else if (!showing.data) {
    value = <Skeleton className="h-8 w-20" />;
  } else if (showing.data.total === 0) {
    // No sessions at all says nothing about users: a dash, as the crash-free
    // cards show, rather than a zero that reads as a fact.
    value = '–';
  } else {
    value = format.number(showing.data.users);
  }

  return (
    <div className={SLOT}>
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
        footnote={
          showing.isError
            ? t('loadFailed')
            : period && mode === 'period'
              ? t('uniqueDevicesIn', {
                  period: overview(`overview.periods.${period}`),
                })
              : t('uniqueDevices')
        }
      />
    </div>
  );
}
