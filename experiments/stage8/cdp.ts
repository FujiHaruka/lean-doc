/**
 * stage 8 — a minimal Chrome DevTools Protocol client over a raw WebSocket.
 *
 * npm/node are unusable in this environment (bad signature -> SIGKILL), so
 * there is no puppeteer. This is the whole client: request/response by id,
 * events by (sessionId, method), flat sessions.
 */
export type CdpMsg = {
  id?: number;
  method?: string;
  params?: Record<string, unknown>;
  result?: Record<string, unknown>;
  error?: { code: number; message: string };
  sessionId?: string;
};

export class Cdp {
  #ws: WebSocket;
  #id = 0;
  #pending = new Map<number, { ok: (v: any) => void; err: (e: Error) => void }>();
  #listeners: Array<(m: CdpMsg) => void> = [];

  private constructor(ws: WebSocket) {
    this.#ws = ws;
    ws.onmessage = (ev) => {
      const m = JSON.parse(ev.data) as CdpMsg;
      if (m.id !== undefined && this.#pending.has(m.id)) {
        const p = this.#pending.get(m.id)!;
        this.#pending.delete(m.id);
        if (m.error) p.err(new Error(`${m.error.code} ${m.error.message}`));
        else p.ok(m.result ?? {});
        return;
      }
      for (const l of this.#listeners) l(m);
    };
  }

  static async connect(wsUrl: string): Promise<Cdp> {
    const ws = new WebSocket(wsUrl);
    await new Promise<void>((res, rej) => {
      ws.onopen = () => res();
      ws.onerror = () => rej(new Error(`ws error ${wsUrl}`));
    });
    return new Cdp(ws);
  }

  on(fn: (m: CdpMsg) => void) {
    this.#listeners.push(fn);
    return () => {
      const i = this.#listeners.indexOf(fn);
      if (i >= 0) this.#listeners.splice(i, 1);
    };
  }

  send<T = Record<string, unknown>>(
    method: string,
    params: Record<string, unknown> = {},
    sessionId?: string,
  ): Promise<T> {
    const id = ++this.#id;
    const msg: CdpMsg = { id, method, params };
    if (sessionId) msg.sessionId = sessionId;
    return new Promise<T>((ok, err) => {
      this.#pending.set(id, { ok, err });
      this.#ws.send(JSON.stringify(msg));
    });
  }

  /** Fire-and-forget: used for Fetch.continueRequest/failRequest, whose reply
   *  races the navigation that the request itself triggers. */
  post(method: string, params: Record<string, unknown> = {}, sessionId?: string) {
    this.send(method, params, sessionId).catch(() => {});
  }

  close() {
    try {
      this.#ws.close();
    } catch { /* already gone */ }
  }
}

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Launch headless Chrome and return its browser-level WebSocket URL. */
export async function launchChrome(
  bin: string,
  port: number,
  userDataDir: string,
): Promise<{ proc: Deno.ChildProcess; wsUrl: string; version: Record<string, string> }> {
  const proc = new Deno.Command(bin, {
    args: [
      "--headless=new",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${userDataDir}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-default-apps",
      "--disable-sync",
      "--metrics-recording-only",
      "--no-pings",
      "about:blank",
    ],
    stdout: "piped",
    stderr: "piped",
  }).spawn();

  let version: Record<string, string> | null = null;
  for (let i = 0; i < 200; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/json/version`);
      version = await r.json();
      break;
    } catch {
      await sleep(100);
    }
  }
  if (!version) throw new Error("chrome did not open the debugging port");
  return { proc, wsUrl: version.webSocketDebuggerUrl, version };
}
