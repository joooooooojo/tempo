/** Reserved plugin-storage key for host-rendered `contributes.settings` values. */
export const TEMPO_SETTINGS_KEY = "__tempo/settings" as const;

/** Host → plugin event when central settings change. */
export const SETTINGS_CHANGED_EVENT = "settings.changed" as const;

/** Host → UI event when theme changes (after `theme.subscribe`). */
export const THEME_CHANGED_EVENT = "theme.changed" as const;
