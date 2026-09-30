/**
 * Route names.
 *
 * A module of their own rather than a constant beside the router, because the components that
 * navigate (the list, the conversation, the settings page) are also the components the router
 * imports: keeping the names here means the two can reference each other without a cycle.
 */
export const ROUTE = {
  /** The detail pane: an open conversation, or the "pick someone" placeholder. */
  chat: 'chat',
  /** The settings page, in the detail pane. */
  settings: 'settings',
} as const;

/** One of the page names above. */
export type RouteName = (typeof ROUTE)[keyof typeof ROUTE];
