// Minimal DASH-over-MSE engine for YouTube's MP4 streams. Rust hands us the
// segment index (from `sidx`), so all this does is: fetch init + segments by
// byte range through `stream://`, append, keep ~AHEAD seconds buffered, drop
// what's far behind. No ABR: quality is the user's pick. ~0 work while paused.

export type Segment = { start: number; end: number; t: number; d: number };
export type TrackIndex = { initEnd: number; segments: Segment[] };
export type TrackSource = { url: string; type: string; index: TrackIndex };

const AHEAD = 40; // seconds to keep buffered in front of the playhead
const BEHIND = 30; // seconds to keep behind it (instant short rewinds)
const RETRIES = 3;

export class StreamError extends Error {
  constructor(message: string, readonly status?: number) {
    super(message);
  }
}

const isAbort = (e: unknown) => e instanceof DOMException && e.name === "AbortError";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function fetchRange(url: string, start: number, end: number, signal: AbortSignal) {
  for (let attempt = 0; ; attempt++) {
    try {
      const r = await fetch(url, { headers: { Range: `bytes=${start}-${end}` }, signal });
      if (!r.ok) throw new StreamError(`Stream request failed (${r.status})`, r.status);
      return await r.arrayBuffer();
    } catch (e) {
      // 403/404: the signed URL is gone, retrying won't help.
      const fatal = e instanceof StreamError && (e.status === 403 || e.status === 404);
      if (isAbort(e) || fatal || attempt >= RETRIES) throw e;
      await sleep(500 * 2 ** attempt);
    }
  }
}

function bufferedEnd(b: TimeRanges, t: number): number {
  for (let i = 0; i < b.length; i++) {
    if (b.start(i) <= t + 0.1 && t < b.end(i)) return b.end(i);
  }
  return t;
}

class Track {
  next = 0;
  needInit = true;
  running = false;
  ctrl = new AbortController();
  /** Bumped on seek / quality switch: results of older fetches are dropped. */
  gen = 0;

  constructor(readonly sb: SourceBuffer, public src: TrackSource) {}

  segAt(t: number): number {
    const segs = this.src.index.segments;
    const i = segs.findIndex((s) => s.t + s.d > t + 0.05);
    return i < 0 ? segs.length : i;
  }

  restart() {
    this.ctrl.abort();
    this.ctrl = new AbortController();
    this.gen++;
  }

  op(fn: () => void): Promise<void> {
    return new Promise((resolve, reject) => {
      const done = () => {
        this.sb.removeEventListener("updateend", done);
        this.sb.removeEventListener("error", fail);
        resolve();
      };
      const fail = () => {
        this.sb.removeEventListener("updateend", done);
        this.sb.removeEventListener("error", fail);
        reject(new StreamError("The video decoder rejected a segment"));
      };
      this.sb.addEventListener("updateend", done);
      this.sb.addEventListener("error", fail);
      try {
        fn();
      } catch (e) {
        this.sb.removeEventListener("updateend", done);
        this.sb.removeEventListener("error", fail);
        reject(e);
      }
    });
  }
}

export class Engine {
  private ms = new MediaSource();
  private tracks: Track[] = [];
  private objectUrl: string;
  private destroyed = false;
  private onError: (e: Error) => void;

  constructor(
    private video: HTMLVideoElement,
    srcs: TrackSource[],
    start: number,
    onError: (e: Error) => void,
  ) {
    this.onError = onError;
    this.objectUrl = URL.createObjectURL(this.ms);
    this.ms.addEventListener(
      "sourceopen",
      () => {
        if (this.destroyed) return;
        const last = (s: TrackSource) => s.index.segments[s.index.segments.length - 1];
        this.ms.duration = Math.max(...srcs.map((s) => (last(s) ? last(s)!.t + last(s)!.d : 0)));
        this.tracks = srcs.map((s) => new Track(this.ms.addSourceBuffer(s.type), s));
        // The element seeks to `start` itself once metadata is in; fetch from there right away.
        this.seekTo(start);
      },
      { once: true },
    );
    video.addEventListener("timeupdate", this.tick);
    video.addEventListener("seeking", this.onSeeking);
    video.src = this.objectUrl;
    video.currentTime = start;
  }

  static supported(type: string) {
    return typeof MediaSource !== "undefined" && MediaSource.isTypeSupported(type);
  }

  /** Swap the video track (index 0) without losing the current segment. */
  switchVideo(src: TrackSource) {
    return this.switchTrack(0, src, false);
  }

  /** Swap the audio track (index 1); a new language is heard right away. */
  switchAudio(src: TrackSource) {
    return this.switchTrack(1, src, true);
  }

  private async switchTrack(i: number, src: TrackSource, now: boolean) {
    const tr = this.tracks[i];
    if (!tr || this.destroyed) return;
    const t = this.video.currentTime;
    tr.restart();
    const gen = tr.gen;
    while (tr.sb.updating) await new Promise((r) => tr.sb.addEventListener("updateend", r, { once: true }));
    if (gen !== tr.gen || this.destroyed) return;

    if (src.type !== tr.src.type && "changeType" in tr.sb) tr.sb.changeType(src.type);
    tr.src = src;
    tr.needInit = true;
    // Quality: keep what plays right now, refill the rest. Language: replace
    // from the playhead, the segment under it is fetched again.
    const cur = tr.segAt(t);
    const segs = src.index.segments;
    const keepUntil = now ? t : cur < segs.length ? segs[cur].t + segs[cur].d : t;
    try {
      if (keepUntil < this.ms.duration) await tr.op(() => tr.sb.remove(keepUntil, Infinity));
    } catch (e) {
      return this.fail(e);
    }
    tr.next = now ? cur : cur + 1;
    this.pump(tr);
  }

  destroy() {
    this.destroyed = true;
    this.tracks.forEach((t) => t.restart());
    this.video.removeEventListener("timeupdate", this.tick);
    this.video.removeEventListener("seeking", this.onSeeking);
    this.video.removeAttribute("src");
    this.video.load();
    URL.revokeObjectURL(this.objectUrl);
  }

  private tick = () => this.tracks.forEach((t) => this.pump(t));

  private onSeeking = () => this.seekTo(this.video.currentTime);

  private seekTo(t: number) {
    for (const tr of this.tracks) {
      // Continue from the end of what's already buffered around `t`.
      const from = bufferedEnd(tr.sb.buffered, t);
      const next = tr.segAt(from);
      if (tr.running && next === tr.next) continue;
      tr.restart();
      tr.next = next;
      this.pump(tr);
    }
  }

  private fail(e: unknown) {
    if (this.destroyed || isAbort(e)) return;
    this.onError(e instanceof Error ? e : new Error(String(e)));
  }

  private async pump(tr: Track) {
    if (tr.running || this.destroyed) return;
    tr.running = true;
    try {
      while (!this.destroyed) {
        try {
          if (!(await this.step(tr))) break;
        } catch (e) {
          // A seek or quality switch aborted the fetch. The pump started for it
          // returned at once (this one was still running), so carry on here.
          if (isAbort(e)) continue;
          throw e;
        }
      }
    } catch (e) {
      this.fail(e);
    } finally {
      tr.running = false;
    }
    this.maybeEnd();
  }

  /** One unit of work; false when there's nothing to do until the playhead moves. */
  private async step(tr: Track): Promise<boolean> {
    const gen = tr.gen;
    const signal = tr.ctrl.signal;
    const t = this.video.currentTime;
    const { url, index } = tr.src;

    if (tr.needInit) {
      const init = await fetchRange(url, 0, index.initEnd, signal);
      if (gen !== tr.gen) return true;
      await tr.op(() => tr.sb.appendBuffer(init));
      tr.needInit = false;
      return true;
    }

    const b = tr.sb.buffered;
    if (b.length && b.start(0) < t - BEHIND - 10) {
      await tr.op(() => tr.sb.remove(0, t - BEHIND));
      return true;
    }
    // Left over from before a seek back: far ahead and detached from the playhead.
    const lastStart = b.length ? b.start(b.length - 1) : 0;
    if (b.length && lastStart > bufferedEnd(b, t) + 1 && lastStart > t + AHEAD) {
      await tr.op(() => tr.sb.remove(lastStart, Infinity));
      return true;
    }

    if (tr.next >= index.segments.length || bufferedEnd(b, t) - t > AHEAD) return false;

    const seg = index.segments[tr.next];
    const data = await fetchRange(url, seg.start, seg.end, signal);
    if (gen !== tr.gen) return true;
    try {
      await tr.op(() => tr.sb.appendBuffer(data));
    } catch (e) {
      if (!(e instanceof DOMException && e.name === "QuotaExceededError")) throw e;
      // Buffer full: drop more of the past, try again on the next tick.
      await tr.op(() => tr.sb.remove(0, Math.max(0, t - 5)));
      return false;
    }
    if (gen === tr.gen) tr.next++;
    return true;
  }

  private maybeEnd() {
    const done = this.tracks.every(
      (t) => !t.running && !t.sb.updating && t.next >= t.src.index.segments.length,
    );
    if (done && this.tracks.length && this.ms.readyState === "open") this.ms.endOfStream();
  }
}
