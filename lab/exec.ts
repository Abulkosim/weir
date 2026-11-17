// Process helpers for the lab orchestrator. Every privileged command goes
// through sudo; clients run as the invoking user inside the client netns.

import { spawn, type ChildProcess } from 'node:child_process';
import { NS, type Argv } from './net.ts';

export interface Done {
  code: number | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
  ms: number;
}

const USER = process.env.USER ?? process.env.LOGNAME ?? 'runner';

export function userEnv(extra: Record<string, string> = {}): string[] {
  const keep = ['PATH', 'HOME', 'PLAYWRIGHT_BROWSERS_PATH', 'LANG'];
  const pairs = keep.filter((key) => process.env[key]).map((key) => `${key}=${process.env[key]}`);
  return [...pairs, ...Object.entries(extra).map(([key, value]) => `${key}=${value}`)];
}

/** argv that runs `argv` unprivileged inside `ns`. */
export function asUser(ns: string, argv: Argv, env: Record<string, string> = {}): Argv {
  return ['sudo', 'ip', 'netns', 'exec', ns, 'sudo', '-u', USER, 'env', ...userEnv(env), ...argv];
}

export function asRoot(argv: Argv): Argv {
  return ['sudo', ...argv];
}

export function run(argv: Argv, timeoutMs = 30_000): Promise<Done> {
  return started(argv, timeoutMs).done;
}

export async function must(argv: Argv, timeoutMs = 30_000): Promise<string> {
  const done = await run(argv, timeoutMs);
  if (done.code !== 0) throw new Error(`${argv.slice(0, 8).join(' ')} failed (${done.code}): ${done.stderr.slice(0, 400)}`);
  return done.stdout;
}

export interface Background {
  child: ChildProcess;
  done: Promise<Done>;
  stop: (signal?: NodeJS.Signals) => Promise<Done>;
}

/** Spawns in its own process group so sudo relays our signals to the command. */
export function started(argv: Argv, timeoutMs = 0): Background {
  const begin = Date.now();
  const child = spawn(argv[0]!, argv.slice(1), { detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '';
  let stderr = '';
  let timedOut = false;
  child.stdout?.setEncoding('utf8').on('data', (chunk: string) => (stdout += chunk));
  child.stderr?.setEncoding('utf8').on('data', (chunk: string) => (stderr += chunk.slice(0, 20_000)));
  const timer = timeoutMs > 0 ? setTimeout(() => {
    timedOut = true;
    signal(child, 'SIGTERM');
    setTimeout(() => signal(child, 'SIGKILL'), 3000).unref();
  }, timeoutMs) : null;
  const done = new Promise<Done>((resolve) => {
    child.on('error', (error) => {
      if (timer) clearTimeout(timer);
      resolve({ code: -1, stdout, stderr: String(error), timedOut, ms: Date.now() - begin });
    });
    child.on('close', (code) => {
      if (timer) clearTimeout(timer);
      resolve({ code, stdout, stderr, timedOut, ms: Date.now() - begin });
    });
  });
  const stop = async (sig: NodeJS.Signals = 'SIGINT') => {
    signal(child, sig);
    const kill = setTimeout(() => signal(child, 'SIGKILL'), 5000);
    const result = await done;
    clearTimeout(kill);
    return result;
  };
  return { child, done, stop };
}

function signal(child: ChildProcess, sig: NodeJS.Signals) {
  if (child.pid === undefined || child.exitCode !== null) return;
  try {
    process.kill(child.pid, sig);
  } catch {
    // already gone
  }
}

export async function runAll(commands: Argv[], ignoreErrors = false): Promise<void> {
  for (const argv of commands) {
    const done = await run(asRoot(argv));
    if (done.code !== 0 && !ignoreErrors) {
      throw new Error(`${argv.join(' ')} failed (${done.code}): ${done.stderr.slice(0, 400)}`);
    }
  }
}

export const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function readIn(ns: string, path: string): Promise<string> {
  return must(asRoot(['ip', 'netns', 'exec', ns, 'cat', path]));
}

export async function counter(ns: string, iface: string, name: string): Promise<number> {
  return Number((await readIn(ns, `/sys/class/net/${iface}/statistics/${name}`)).trim());
}

export function clientUser(argv: Argv, env: Record<string, string> = {}): Argv {
  return asUser(NS.client, argv, env);
}
