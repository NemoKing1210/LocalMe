export const ROUTE = {
  chat: 'chat',
  settings: 'settings',
} as const;

export type RouteName = (typeof ROUTE)[keyof typeof ROUTE];
