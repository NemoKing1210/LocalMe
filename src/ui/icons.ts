// Icons are data, not markup: each is a list of primitives rendered through `v-for`, so the
// component needs no `v-html` (which the CSP and lint rules forbid).
type Shape =
  | { readonly kind: 'path'; readonly d: string }
  | {
      readonly kind: 'line';
      readonly x1: number;
      readonly y1: number;
      readonly x2: number;
      readonly y2: number;
    }
  | { readonly kind: 'polyline'; readonly points: string }
  | { readonly kind: 'circle'; readonly cx: number; readonly cy: number; readonly r: number }
  | {
      readonly kind: 'rect';
      readonly x: number;
      readonly y: number;
      readonly width: number;
      readonly height: number;
      readonly rx: number;
    }
  | { readonly kind: 'dot'; readonly cx: number; readonly cy: number; readonly r: number };

// Three slider rows, not a cog: gear teeth blur into a ring at 24px, while offset tracks stay legible.
function tuneRows(): Shape[] {
  const rows: readonly (readonly [y: number, handleX: number])[] = [
    [6.5, 15.2],
    [12, 8.8],
    [17.5, 12.4],
  ];
  const shapes: Shape[] = [];
  for (const [y, handleX] of rows) {
    shapes.push({ kind: 'line', x1: 3.6, y1: y, x2: 20.4, y2: y });
    shapes.push({ kind: 'line', x1: handleX, y1: y - 2.3, x2: handleX, y2: y + 2.3 });
  }
  return shapes;
}

function sunRays(): Shape[] {
  const rays: Shape[] = [];
  for (let index = 0; index < 8; index += 1) {
    const angle = (index * Math.PI) / 4;
    rays.push({
      kind: 'line',
      x1: round(12 + Math.cos(angle) * 6.5),
      y1: round(12 + Math.sin(angle) * 6.5),
      x2: round(12 + Math.cos(angle) * 9.2),
      y2: round(12 + Math.sin(angle) * 9.2),
    });
  }
  return rays;
}

function round(value: number): number {
  return Math.round(value * 100) / 100;
}

const BELL: Shape[] = [
  { kind: 'path', d: 'M6 9.5a6 6 0 0 1 12 0c0 3.4 1.2 5.1 1.9 5.9H4.1C4.8 14.6 6 12.9 6 9.5z' },
  { kind: 'path', d: 'M10 18.8a2.2 2.2 0 0 0 4 0' },
];

export const ICONS: Record<string, readonly Shape[]> = {
  search: [
    { kind: 'circle', cx: 11, cy: 11, r: 6.6 },
    { kind: 'line', x1: 15.9, y1: 15.9, x2: 20.5, y2: 20.5 },
  ],
  send: [
    { kind: 'path', d: 'M21.5 2.5 11 13' },
    { kind: 'path', d: 'M21.5 2.5 15 21.5l-4-8.5-8.5-4z' },
  ],
  close: [
    { kind: 'line', x1: 6, y1: 6, x2: 18, y2: 18 },
    { kind: 'line', x1: 18, y1: 6, x2: 6, y2: 18 },
  ],
  back: [
    { kind: 'line', x1: 20, y1: 12, x2: 4.5, y2: 12 },
    { kind: 'polyline', points: '11.5,19 4.5,12 11.5,5' },
  ],
  forward: [{ kind: 'polyline', points: '9.5,5 16.5,12 9.5,19' }],
  down: [{ kind: 'polyline', points: '5,9.5 12,16.5 19,9.5' }],
  check: [{ kind: 'polyline', points: '4.5,12.5 9.5,17.5 19.5,6' }],
  'check-all': [
    { kind: 'polyline', points: '1.5,12.5 6.5,17.5 15,6' },
    { kind: 'polyline', points: '9.5,12.5 14.5,17.5 23,6' },
  ],
  people: [
    { kind: 'circle', cx: 9, cy: 8, r: 3.4 },
    { kind: 'path', d: 'M2.6 19.8a6.4 6.4 0 0 1 12.8 0' },
    { kind: 'path', d: 'M16 5.2a3.4 3.4 0 0 1 0 5.9' },
    { kind: 'path', d: 'M17.6 19.8a6.4 6.4 0 0 0-1.7-4.3' },
  ],
  person: [
    { kind: 'circle', cx: 12, cy: 8, r: 3.8 },
    { kind: 'path', d: 'M4.5 20.2a7.5 7.5 0 0 1 15 0' },
  ],
  'more-vertical': [
    { kind: 'dot', cx: 12, cy: 5.2, r: 1.7 },
    { kind: 'dot', cx: 12, cy: 12, r: 1.7 },
    { kind: 'dot', cx: 12, cy: 18.8, r: 1.7 },
  ],
  error: [
    { kind: 'circle', cx: 12, cy: 12, r: 9 },
    { kind: 'line', x1: 12, y1: 7.2, x2: 12, y2: 13.4 },
    { kind: 'dot', cx: 12, cy: 16.6, r: 1.1 },
  ],
  info: [
    { kind: 'circle', cx: 12, cy: 12, r: 9 },
    { kind: 'line', x1: 12, y1: 16.8, x2: 12, y2: 10.6 },
    { kind: 'dot', cx: 12, cy: 7.4, r: 1.1 },
  ],
  warning: [
    { kind: 'path', d: 'M12 3.2 2.4 20h19.2z' },
    { kind: 'line', x1: 12, y1: 9.2, x2: 12, y2: 14 },
    { kind: 'dot', cx: 12, cy: 17, r: 1.05 },
  ],
  bell: BELL,
  'bell-off': [...BELL, { kind: 'line', x1: 4, y1: 4, x2: 20, y2: 20 }],
  offline: [
    { kind: 'circle', cx: 12, cy: 12, r: 9 },
    { kind: 'line', x1: 5.6, y1: 5.6, x2: 18.4, y2: 18.4 },
  ],
  clock: [
    { kind: 'circle', cx: 12, cy: 12, r: 9 },
    { kind: 'polyline', points: '12,6.8 12,12.4 16.2,14.8' },
  ],
  chat: [
    {
      kind: 'path',
      d: 'M3.5 6.6A2.6 2.6 0 0 1 6.1 4h11.8a2.6 2.6 0 0 1 2.6 2.6v7a2.6 2.6 0 0 1-2.6 2.6H9.4L4 20z',
    },
  ],
  trash: [
    { kind: 'line', x1: 3.8, y1: 6.8, x2: 20.2, y2: 6.8 },
    { kind: 'path', d: 'M9.4 6.8V4.6h5.2v2.2' },
    { kind: 'path', d: 'M6.2 6.8V19a2 2 0 0 0 2 2h7.6a2 2 0 0 0 2-2V6.8' },
    { kind: 'line', x1: 10.2, y1: 10.6, x2: 10.2, y2: 17.2 },
    { kind: 'line', x1: 13.8, y1: 10.6, x2: 13.8, y2: 17.2 },
  ],
  // Two sheets, not a filled block: an outline stays readable at the 18px a menu row uses.
  copy: [
    { kind: 'rect', x: 8.8, y: 8.8, width: 12.4, height: 12.4, rx: 2.6 },
    {
      kind: 'path',
      d: 'M15.6 5.2a2.6 2.6 0 0 0-2.6-2.4H5.4A2.6 2.6 0 0 0 2.8 5.4v7.8a2.6 2.6 0 0 0 2.6 2.6',
    },
  ],
  tune: tuneRows(),
  palette: [
    { kind: 'circle', cx: 12, cy: 12, r: 9 },
    { kind: 'dot', cx: 8.2, cy: 9.2, r: 1.35 },
    { kind: 'dot', cx: 12, cy: 7.4, r: 1.35 },
    { kind: 'dot', cx: 15.8, cy: 9.2, r: 1.35 },
    { kind: 'dot', cx: 9.4, cy: 15.4, r: 1.35 },
  ],
  globe: [
    { kind: 'circle', cx: 12, cy: 12, r: 9 },
    { kind: 'path', d: 'M12 3a13 13 0 0 1 0 18 13 13 0 0 1 0-18z' },
    { kind: 'line', x1: 3.4, y1: 9.2, x2: 20.6, y2: 9.2 },
    { kind: 'line', x1: 3.4, y1: 14.8, x2: 20.6, y2: 14.8 },
  ],
  volume: [
    { kind: 'path', d: 'M4 9.2v5.6h3.4L12 19V5L7.4 9.2z' },
    { kind: 'path', d: 'M15.4 9.4a3.8 3.8 0 0 1 0 5.2' },
    { kind: 'path', d: 'M18.2 6.8a7.4 7.4 0 0 1 0 10.4' },
  ],
  sun: [{ kind: 'circle', cx: 12, cy: 12, r: 4.1 }, ...sunRays()],
  moon: [{ kind: 'path', d: 'M20.4 14.6A8.6 8.6 0 0 1 9.4 3.6a8.6 8.6 0 1 0 11 11z' }],
  display: [
    { kind: 'rect', x: 2.8, y: 4.2, width: 18.4, height: 12.4, rx: 2 },
    { kind: 'line', x1: 9, y1: 20, x2: 15, y2: 20 },
    { kind: 'line', x1: 12, y1: 16.6, x2: 12, y2: 20 },
  ],
  power: [
    { kind: 'path', d: 'M7.4 6.4a7.4 7.4 0 1 0 9.2 0' },
    { kind: 'line', x1: 12, y1: 3, x2: 12, y2: 11.4 },
  ],
  window: [
    { kind: 'rect', x: 3, y: 4.5, width: 18, height: 15, rx: 2.2 },
    { kind: 'line', x1: 3, y1: 9.5, x2: 21, y2: 9.5 },
  ],
  refresh: [
    { kind: 'path', d: 'M4.4 12a7.6 7.6 0 0 1 12.9-5.4' },
    { kind: 'path', d: 'M19.6 12a7.6 7.6 0 0 1-12.9 5.4' },
    { kind: 'polyline', points: '17.1,2.6 17.4,6.9 13.1,7.2' },
    { kind: 'polyline', points: '6.9,21.4 6.6,17.1 10.9,16.8' },
  ],
  database: [
    { kind: 'path', d: 'M4 6.4c0-1.4 3.6-2.6 8-2.6s8 1.2 8 2.6-3.6 2.6-8 2.6-8-1.2-8-2.6z' },
    { kind: 'path', d: 'M4 6.4v11.2c0 1.4 3.6 2.6 8 2.6s8-1.2 8-2.6V6.4' },
    { kind: 'path', d: 'M4 12c0 1.4 3.6 2.6 8 2.6s8-1.2 8-2.6' },
  ],
  'chevron-down': [{ kind: 'polyline', points: '6,9.5 12,15.5 18,9.5' }],
  folder: [
    {
      kind: 'path',
      d: 'M3.3 6.8A2 2 0 0 1 5.3 4.8h3.5l1.9 2.3h8a2 2 0 0 1 2 2v7.9a2 2 0 0 1-2 2H5.3a2 2 0 0 1-2-2z',
    },
    { kind: 'line', x1: 3.3, y1: 9.5, x2: 20.7, y2: 9.5 },
  ],
  paperclip: [
    {
      kind: 'path',
      d: 'M19.4 11.3 12.2 18.5a4.6 4.6 0 0 1-6.5-6.5l7.6-7.6a3 3 0 0 1 4.3 4.3l-7.6 7.6a1.5 1.5 0 0 1-2.1-2.1l7-7',
    },
  ],
  file: [
    { kind: 'path', d: 'M6.2 3.4h7.2l4.4 4.4v12.8H6.2z' },
    { kind: 'polyline', points: '13.4,3.4 13.4,7.8 17.8,7.8' },
  ],
  image: [
    { kind: 'rect', x: 3.2, y: 4.8, width: 17.6, height: 14.4, rx: 2.4 },
    { kind: 'dot', cx: 8.8, cy: 9.8, r: 1.6 },
    { kind: 'polyline', points: '4.6,17.6 10.2,12.4 13.8,15.4 17.4,11.4 20.4,14.4' },
  ],
  download: [
    { kind: 'line', x1: 12, y1: 3.6, x2: 12, y2: 15.2 },
    { kind: 'polyline', points: '7.4,10.8 12,15.4 16.6,10.8' },
    { kind: 'path', d: 'M4.6 18.4v1.4a1.6 1.6 0 0 0 1.6 1.6h11.6a1.6 1.6 0 0 0 1.6-1.6v-1.4' },
  ],
  launch: [
    { kind: 'path', d: 'M14.4 4.2h5.4v5.4' },
    { kind: 'line', x1: 19.6, y1: 4.4, x2: 11.4, y2: 12.6 },
    {
      kind: 'path',
      d: 'M18.2 13.8v5a1.6 1.6 0 0 1-1.6 1.6H5.4a1.6 1.6 0 0 1-1.6-1.6V7.6A1.6 1.6 0 0 1 5.4 6h5',
    },
  ],
};

export type IconName = keyof typeof ICONS;
