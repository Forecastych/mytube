// Comments under the description. Nothing is fetched until the section is
// close to the viewport: most videos are watched without scrolling down.
import { memo, useCallback, useEffect, useRef, useState } from "react";
import { BadgeCheck, ChevronDown, ChevronUp, MessageSquare, Pin, RotateCw, ThumbsUp } from "lucide-react";
import { api, errorText, type Comment, type CommentsPage } from "../../lib/api";
import { Linkified } from "./Linkified";

type Load = { comments: Comment[]; next: string | null; busy: boolean; error: string | null };
const empty: Load = { comments: [], next: null, busy: false, error: null };

/** A list that grows page by page from continuation tokens. */
function usePages(first: () => Promise<CommentsPage>, onFirst?: (p: CommentsPage) => void) {
  const [s, setS] = useState(empty);
  const gen = useRef(0);

  const run = useCallback(
    async (get: () => Promise<CommentsPage>, append: boolean) => {
      const g = ++gen.current;
      setS((x) => ({ ...(append ? x : empty), busy: true, error: null }));
      try {
        const p = await get();
        if (g !== gen.current) return;
        if (!append) onFirst?.(p);
        setS((x) => ({ comments: append ? [...x.comments, ...p.comments] : p.comments, next: p.next, busy: false, error: null }));
      } catch (e) {
        if (g === gen.current) setS((x) => ({ ...x, busy: false, error: errorText(e) }));
      }
    },
    [onFirst],
  );

  const start = useCallback((get = first) => run(get, false), [run, first]);
  const more = useCallback(() => s.next && !s.busy && run(() => api.commentsMore(s.next!), true), [run, s.next, s.busy]);
  return { ...s, start, more };
}

export function Comments({ id, onSeek }: { id: string; onSeek: (t: number) => void }) {
  const ref = useRef<HTMLElement>(null);
  const [head, setHead] = useState<CommentsPage | null>(null);
  const [sort, setSort] = useState(0);
  const first = useCallback(() => api.comments(id), [id]);
  const list = usePages(first, useCallback((p: CommentsPage) => setHead((h) => h ?? p), []));
  const started = useRef(false);

  // Start when the section comes near the screen.
  useEffect(() => {
    const el = ref.current!;
    const io = new IntersectionObserver(
      ([e]) => {
        if (!e.isIntersecting || started.current) return;
        started.current = true;
        io.disconnect();
        void list.start();
      },
      { rootMargin: "600px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Next page when the end of the list comes near.
  const sentinel = useRef<HTMLDivElement>(null);
  const more = list.more;
  useEffect(() => {
    const el = sentinel.current;
    if (!el || !list.next) return;
    const io = new IntersectionObserver(([e]) => e.isIntersecting && more(), { rootMargin: "800px 0px" });
    io.observe(el);
    return () => io.disconnect();
  }, [list.next, more]);

  const pickSort = (i: number) => {
    if (!head || i === sort) return;
    setSort(i);
    void list.start(() => api.commentsMore(head.sort[i].token));
  };

  return (
    <section className="watch-comments" ref={ref}>
      <div className="comments-head">
        <h2>
          Comments
          {head?.count && <span className="muted comments-count">{head.count}</span>}
        </h2>
        {head && head.sort.length > 1 && (
          <div className="comments-sort" role="radiogroup" aria-label="Sort comments">
            {head.sort.map((o, i) => (
              <button key={o.title} role="radio" aria-checked={i === sort} className={i === sort ? "active" : ""} onClick={() => pickSort(i)}>
                {o.title}
              </button>
            ))}
          </div>
        )}
      </div>

      {list.comments.map((c) => (
        <CommentItem key={c.id} c={c} onSeek={onSeek} />
      ))}

      {list.busy && <CommentSkeleton />}
      {list.error && (
        <div className="comments-note muted">
          <span>{list.error}</span>
          <button className="btn" onClick={() => (list.comments.length ? list.more() : list.start())}>
            <RotateCw size={15} /> Retry
          </button>
        </div>
      )}
      {head?.disabled && (
        <div className="comments-note muted">
          <MessageSquare size={18} /> Comments are turned off.
        </div>
      )}
      <div ref={sentinel} />
    </section>
  );
}

const CLAMP_CHARS = 420;

const CommentItem = memo(function CommentItem({ c, onSeek, reply }: { c: Comment; onSeek: (t: number) => void; reply?: boolean }) {
  const [full, setFull] = useState(false);
  const long = c.text.length > CLAMP_CHARS || c.text.split("\n").length > 6;
  return (
    <div className={`comment${reply ? " reply" : ""}`}>
      {c.avatar ? (
        <img className="comment-avatar" src={c.avatar} alt="" loading="lazy" decoding="async" referrerPolicy="no-referrer" />
      ) : (
        <span className="comment-avatar" />
      )}
      <div className="comment-body">
        <div className="comment-meta">
          {c.pinned && (
            <span className="comment-pin muted small">
              <Pin size={12} /> Pinned
            </span>
          )}
          <span className={`comment-author${c.creator ? " creator" : ""}`}>{c.author}</span>
          {c.verified && <BadgeCheck size={14} className="muted" aria-label="Verified" />}
          <span className="muted small">{c.published}</span>
        </div>
        <div className={`comment-text${long && !full ? " clamped" : ""}`}>
          <Linkified text={c.text} onSeek={onSeek} />
        </div>
        {long && (
          <button className="comment-link" onClick={() => setFull(!full)}>
            {full ? "Show less" : "Read more"}
          </button>
        )}
        <div className="comment-actions muted small">
          {c.likes && (
            <span>
              <ThumbsUp size={13} /> {c.likes}
            </span>
          )}
        </div>
        {c.replies && <Replies token={c.replies} count={c.replyCount} onSeek={onSeek} />}
      </div>
    </div>
  );
});

function Replies({ token, count, onSeek }: { token: string; count: string | null; onSeek: (t: number) => void }) {
  const [open, setOpen] = useState(false);
  const first = useCallback(() => api.commentsMore(token), [token]);
  const list = usePages(first);

  const toggle = () => {
    if (!open && list.comments.length === 0 && !list.busy) void list.start();
    setOpen(!open);
  };

  return (
    <div className="replies">
      <button className="comment-link accent" onClick={toggle}>
        {open ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
        {count ? `${count} ${count === "1" ? "reply" : "replies"}` : "Replies"}
      </button>
      {open && (
        <>
          {list.comments.map((c) => (
            <CommentItem key={c.id} c={c} onSeek={onSeek} reply />
          ))}
          {list.busy && <div className="muted small replies-note">Loading…</div>}
          {list.error && (
            <button className="comment-link replies-note" onClick={() => (list.comments.length ? list.more() : list.start())}>
              {list.error} · Retry
            </button>
          )}
          {list.next && !list.busy && (
            <button className="comment-link accent replies-note" onClick={() => list.more()}>
              Show more replies
            </button>
          )}
        </>
      )}
    </div>
  );
}

function CommentSkeleton() {
  return (
    <div aria-hidden>
      {[0, 1, 2].map((i) => (
        <div key={i} className="comment skeleton">
          <span className="comment-avatar" />
          <div className="comment-body">
            <span className="sk-line" style={{ width: "28%" }} />
            <span className="sk-line" style={{ width: "86%" }} />
            <span className="sk-line" style={{ width: "62%" }} />
          </div>
        </div>
      ))}
    </div>
  );
}
