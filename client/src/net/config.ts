/**
 * Server addressing. Same origin by default (under `/codescape` when served
 * through the VisionClaw port); `?server=http://host:4100` overrides for development.
 * `?demo=1` runs without a server.
 *
 * The override is gated (W4COVC-F1): REST, the position/event sockets and the
 * push-to-talk microphone stream all follow it, so a link carrying
 * `?server=https://attacker` would otherwise redirect them. It is honoured only
 * for the page's own origin (scheme, host and port), a loopback host when the
 * page itself is served from loopback, or a host listed in the build-time
 * `VITE_CODESCAPE_SERVER_ALLOW` (comma-separated `host` or `host:port`). Anything else is ignored and reported in `Endpoints.rejectedServer`.
 */

const LOOPBACK = new Set(['localhost', '127.0.0.1', '[::1]']);

/** Hosts allowed by the build, from `VITE_CODESCAPE_SERVER_ALLOW`. */
export function configuredAllowedHosts(): string[] {
  const raw = (import.meta as { env?: Record<string, string | undefined> }).env?.VITE_CODESCAPE_SERVER_ALLOW ?? '';
  return raw.split(',').map((h) => h.trim().toLowerCase()).filter(Boolean);
}

/** First path segments that mark Codescape as served under a shared-port prefix (VisionClaw nginx strips it). */
export const PATH_PREFIXES: readonly string[] = ['codescape'];

/**
 * The path prefix this page is served under: `'/codescape'` when the first
 * path segment is one of `segments`, otherwise `''` (served at the root).
 * Splits on segment boundaries, so `/codescapex` is not a match.
 */
export function detectPrefix(pathname: string, segments: readonly string[] = PATH_PREFIXES): string {
  const first = (pathname.startsWith('/') ? pathname : `/${pathname}`).split('/')[1] ?? '';
  return first && segments.includes(first) ? `/${first}` : '';
}

export interface Endpoints {
  /** HTTP base without trailing slash ('' = same origin root, '/codescape' under the shared port). */
  http: string;
  positions: string;
  events: string;
  voice: string;
  /** Human-readable target for the "connecting to …" state. */
  label: string;
  /** Prefix the page itself is served under, independent of `?server=` ('' = served at the root). */
  pagePrefix: string;
  /** The `?server=` value that was ignored because it failed the origin gate (undefined when none or accepted). */
  rejectedServer?: string;
}

/** Parse a `?server=` value; null when it is not an http(s) URL with a host and no userinfo. */
function parseOverride(override: string): URL | null {
  let u: URL;
  try {
    u = new URL(override.includes('://') ? override : `http://${override}`);
  } catch {
    return null;
  }
  if ((u.protocol !== 'http:' && u.protocol !== 'https:') || !u.hostname || u.username || u.password) return null;
  return u;
}

/** Hostname of a `host[:port]` string, lower-cased ('' when it does not parse). */
function hostnameOf(host: string): string {
  try {
    return new URL(`http://${host}`).hostname.toLowerCase();
  } catch {
    return '';
  }
}

function overrideAllowed(u: URL, page: { protocol: string; host: string }, allowed: readonly string[]): boolean {
  const host = u.host.toLowerCase();
  const hostname = u.hostname.toLowerCase();
  // The page's own origin: scheme, host and port all equal, so an https page is
  // never pointed at its plain-http twin.
  if (u.protocol === page.protocol && host === page.host.toLowerCase()) return true;
  // Loopback only from a page that is itself served from loopback (development).
  // A remotely served page must not be steered at a local listener by a link.
  if (LOOPBACK.has(hostname) && LOOPBACK.has(hostnameOf(page.host))) return true;
  return allowed.includes(host) || allowed.includes(hostname);
}

export function resolveEndpoints(
  loc: { protocol: string; host: string; search: string; pathname?: string },
  allowedHosts: readonly string[] = configuredAllowedHosts(),
): Endpoints {
  const params = new URLSearchParams(loc.search);
  const override = params.get('server');
  const pagePrefix = detectPrefix(loc.pathname ?? '/');
  let httpBase: string;
  let wsBase: string;
  let label: string;
  let rejectedServer: string | undefined;
  const u = override ? parseOverride(override) : null;
  if (override && (!u || !overrideAllowed(u, loc, allowedHosts))) rejectedServer = override;
  if (override && u && !rejectedServer) {
    const p = u.pathname.replace(/\/+$/, '');
    httpBase = `${u.protocol}//${u.host}${p}`;
    wsBase = `${u.protocol === 'https:' ? 'wss:' : 'ws:'}//${u.host}${p}`;
    label = httpBase;
  } else {
    httpBase = pagePrefix;
    wsBase = `${loc.protocol === 'https:' ? 'wss:' : 'ws:'}//${loc.host}${pagePrefix}`;
    label = `${loc.protocol}//${loc.host}${pagePrefix}`;
  }
  return {
    http: httpBase,
    positions: `${wsBase}/ws/positions`,
    events: `${wsBase}/ws/events`,
    voice: `${wsBase}/ws/voice`,
    label,
    pagePrefix,
    ...(rejectedServer !== undefined ? { rejectedServer } : {}),
  };
}

export function isDemo(search: string): boolean {
  const v = new URLSearchParams(search).get('demo');
  return v !== null && v !== '0' && v !== 'false';
}

/** Reconnect delay: exponential from 500 ms, capped at 15 s, with ±20% jitter from `rand` in [0,1). */
export function backoffDelay(attempt: number, rand: number = Math.random()): number {
  const base = Math.min(15_000, 500 * Math.pow(2, Math.max(0, attempt)));
  return Math.round(base * (0.8 + 0.4 * rand));
}

/** Append query parameters (skipping empty values) to an absolute ws/http URL. */
export function withQuery(url: string, params: Record<string, string | null | undefined>): string {
  const u = new URL(url);
  for (const [k, v] of Object.entries(params)) if (v) u.searchParams.set(k, v);
  return u.toString();
}

/** A random per-page session key linking /ws/events, /ws/positions and /ws/voice server-side. */
export function newSessionKey(rand: () => number = Math.random): string {
  const c = (globalThis as { crypto?: Crypto }).crypto;
  if (c?.randomUUID) return c.randomUUID();
  return Array.from({ length: 4 }, () => Math.floor(rand() * 0x1_0000_0000).toString(16).padStart(8, '0')).join('-');
}
