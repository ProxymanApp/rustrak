import { Checkbox } from '@rustrak/ui';
import { useTranslations } from 'use-intl';

/**
 * Proxyman: "Remember me for 2 weeks" on the password form (see PROXYMAN.md).
 *
 * Ticked, the server keeps the session for two weeks however idle, and its
 * cookie outlives the browser (`apps/server/src/auth/remember_me.rs`).
 * Unticked is upstream's sign-in: a day without a request, or closing the
 * browser, signs out.
 */
export function RememberMeField({
  checked,
  onCheckedChange,
  disabled,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  const t = useTranslations('proxyman');
  const label = t('rememberMe');

  return (
    // The whole line is the target, as a native checkbox's label would be.
    <label className="flex w-fit cursor-pointer select-none items-center gap-2.5 has-data-disabled:cursor-default">
      <Checkbox
        aria-label={label}
        checked={checked}
        onCheckedChange={(next) => onCheckedChange(next)}
        disabled={disabled}
      />
      <span className="text-control text-fg-muted">{label}</span>
    </label>
  );
}
