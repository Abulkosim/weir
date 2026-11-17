// Pure argv builders and parsers for the lab's shaped link. Nothing here runs a
// command; lab/run.ts prefixes each argv with sudo.

export const NS = { client: 'weir-c', router: 'weir-r', server: 'weir-s' } as const;
export const IFACE = { client: 'c0', routerToClient: 'rc', routerToServer: 'rs', server: 's0' } as const;
export const ADDR = {
  client: '10.77.1.2',
  routerClient: '10.77.1.1',
  routerServer: '10.77.2.1',
  server: '10.77.2.2',
} as const;
export const PORT = 8080;
export const MSS = 1448;
export const FRAME = 1514;

export type JitterMode = 'slot' | 'pareto';

export interface Jitter {
  /** One-way spread in ms: the paretonormal sigma, or the slot window. */
  ms: number;
}

export interface Shape {
  down: number;
  up: number;
  rttMs: number;
  lossPct: number;
  bufferMs: number;
  jitter?: Jitter;
}

export type Argv = string[];

export function inNs(ns: string, ...argv: string[]): Argv {
  return ['ip', 'netns', 'exec', ns, ...argv];
}

const sysctl = (ns: string, key: string, value: string): Argv => inNs(ns, 'sysctl', '-q', '-w', `${key}=${value}`);

export const CLIENT_SYSCTLS: ReadonlyArray<readonly [string, string]> = [
  ['net.ipv4.tcp_no_metrics_save', '1'],
  ['net.ipv4.tcp_congestion_control', 'cubic'],
  ['net.ipv4.tcp_timestamps', '1'],
  ['net.ipv4.tcp_rmem', '4096 131072 6291456'],
  ['net.ipv4.tcp_wmem', '4096 16384 4194304'],
];

export function topologyUp(): Argv[] {
  const { client, router, server } = NS;
  return [
    ['ip', 'netns', 'add', client],
    ['ip', 'netns', 'add', router],
    ['ip', 'netns', 'add', server],
    ['ip', 'link', 'add', IFACE.client, 'netns', client, 'type', 'veth', 'peer', 'name', IFACE.routerToClient, 'netns', router],
    ['ip', 'link', 'add', IFACE.server, 'netns', server, 'type', 'veth', 'peer', 'name', IFACE.routerToServer, 'netns', router],
    ...[client, router, server].flatMap((ns) => [
      sysctl(ns, 'net.ipv6.conf.all.disable_ipv6', '1'),
      sysctl(ns, 'net.ipv6.conf.default.disable_ipv6', '1'),
      inNs(ns, 'ip', 'link', 'set', 'lo', 'up'),
    ]),
    inNs(client, 'ip', 'addr', 'add', `${ADDR.client}/24`, 'dev', IFACE.client),
    inNs(router, 'ip', 'addr', 'add', `${ADDR.routerClient}/24`, 'dev', IFACE.routerToClient),
    inNs(router, 'ip', 'addr', 'add', `${ADDR.routerServer}/24`, 'dev', IFACE.routerToServer),
    inNs(server, 'ip', 'addr', 'add', `${ADDR.server}/24`, 'dev', IFACE.server),
    inNs(client, 'ip', 'link', 'set', IFACE.client, 'up'),
    inNs(router, 'ip', 'link', 'set', IFACE.routerToClient, 'up'),
    inNs(router, 'ip', 'link', 'set', IFACE.routerToServer, 'up'),
    inNs(server, 'ip', 'link', 'set', IFACE.server, 'up'),
    inNs(client, 'ip', 'route', 'add', 'default', 'via', ADDR.routerClient),
    inNs(server, 'ip', 'route', 'add', 'default', 'via', ADDR.routerServer),
    sysctl(router, 'net.ipv4.ip_forward', '1'),
    ...[client, server].flatMap((ns) => CLIENT_SYSCTLS.map(([key, value]) => sysctl(ns, key, value))),
  ];
}

export function teardown(): Argv[] {
  return [NS.client, NS.router, NS.server].map((ns) => ['ip', 'netns', 'del', ns]);
}

export function offloads(on: boolean): Argv[] {
  const state = on ? 'on' : 'off';
  return [
    [NS.client, IFACE.client],
    [NS.router, IFACE.routerToClient],
    [NS.router, IFACE.routerToServer],
    [NS.server, IFACE.server],
  ].map(([ns, dev]) => inNs(ns!, 'ethtool', '-K', dev!, 'tso', state, 'gso', state, 'gro', state));
}

const ms = (value: number) => `${Number(value.toFixed(3))}ms`;

export function tbfBurst(mbps: number): number {
  return Math.max(2 * FRAME, Math.round((mbps * 1e6) / 8 * 0.001));
}

export function tbfLimit(mbps: number, bufferMs: number): number {
  return Math.max(2 * FRAME, Math.round((mbps * 1e6) / 8 * bufferMs / 1000));
}

export function netemArgs(shape: Shape, mode: JitterMode): string[] {
  const oneWay = shape.rttMs / 2;
  const args = ['netem', 'limit', '1000000'];
  if (shape.jitter && mode === 'pareto') {
    args.push('delay', ms(oneWay), ms(shape.jitter.ms), 'distribution', 'paretonormal', 'rate', '10gbit');
  } else {
    args.push('delay', ms(oneWay));
  }
  if (shape.jitter && mode === 'slot') {
    args.push('slot', '1ms', ms(Math.max(2, 2 * shape.jitter.ms)));
  }
  if (shape.lossPct > 0) args.push('loss', 'random', `${shape.lossPct}%`);
  return args;
}

function egress(ns: string, dev: string, mbps: number, shape: Shape, mode: JitterMode): Argv[] {
  return [
    inNs(ns, 'tc', 'qdisc', 'add', 'dev', dev, 'root', 'handle', '1:', ...netemArgs(shape, mode)),
    inNs(
      ns, 'tc', 'qdisc', 'add', 'dev', dev, 'parent', '1:1', 'handle', '10:',
      'tbf', 'rate', `${mbps}mbit`, 'burst', String(tbfBurst(mbps)), 'limit', String(tbfLimit(mbps, shape.bufferMs)),
    ),
  ];
}

/** Commands that delete the router's root qdiscs. Failures ("no such qdisc") are expected. */
export function unshape(): Argv[] {
  return [IFACE.routerToClient, IFACE.routerToServer].map((dev) => inNs(NS.router, 'tc', 'qdisc', 'del', 'dev', dev, 'root'));
}

/** Fresh qdiscs on both router egresses. Run `unshape()` first. */
export function shape(shape: Shape, mode: JitterMode): Argv[] {
  return [
    ...egress(NS.router, IFACE.routerToClient, shape.down, shape, mode),
    ...egress(NS.router, IFACE.routerToServer, shape.up, shape, mode),
  ];
}

export function qdiscStats(dev: string): Argv {
  return inNs(NS.router, 'tc', '-s', '-j', 'qdisc', 'show', 'dev', dev);
}

export interface QdiscStats {
  kind: string;
  handle: string;
  bytes: number;
  packets: number;
  drops: number;
  overlimits: number;
  backlog: number;
  qlen: number;
}

export function parseQdiscStats(text: string): QdiscStats[] {
  const parsed: unknown = JSON.parse(text);
  if (!Array.isArray(parsed)) return [];
  return parsed.map((entry: Record<string, unknown>) => ({
    kind: String(entry.kind ?? ''),
    handle: String(entry.handle ?? ''),
    bytes: Number(entry.bytes ?? 0),
    packets: Number(entry.packets ?? 0),
    drops: Number(entry.drops ?? 0),
    overlimits: Number(entry.overlimits ?? 0),
    backlog: Number(entry.backlog ?? 0),
    qlen: Number(entry.qlen ?? 0),
  }));
}

export interface Softnet {
  processed: number;
  dropped: number;
  squeezed: number;
}

/** /proc/net/softnet_stat: one hex row per CPU; columns processed, dropped, time_squeeze. */
export function parseSoftnet(text: string): Softnet {
  const total = { processed: 0, dropped: 0, squeezed: 0 };
  for (const line of text.split('\n')) {
    const columns = line.trim().split(/\s+/);
    if (columns.length < 3) continue;
    total.processed += Number.parseInt(columns[0]!, 16);
    total.dropped += Number.parseInt(columns[1]!, 16);
    total.squeezed += Number.parseInt(columns[2]!, 16);
  }
  return total;
}

/** `nstat -asz` text: "Name value rate" per line. */
export function parseNstat(text: string): Record<string, number> {
  const counters: Record<string, number> = {};
  for (const line of text.split('\n')) {
    const match = /^(\w+)\s+(\d+)/.exec(line.trim());
    if (match) counters[match[1]!] = Number(match[2]);
  }
  return counters;
}

export const REORDER_COUNTERS = ['TcpExtTCPSACKReorder', 'TcpExtTCPTSReorder', 'TcpExtTCPRenoReorder', 'TcpExtTCPReordSeen'] as const;

export function reorderDelta(before: Record<string, number>, after: Record<string, number>): number {
  return REORDER_COUNTERS.reduce((sum, name) => sum + Math.max(0, (after[name] ?? 0) - (before[name] ?? 0)), 0);
}
