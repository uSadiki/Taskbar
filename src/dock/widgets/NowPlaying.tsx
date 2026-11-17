import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Music, Pause, Play, Repeat, Repeat1, Shuffle, SkipBack, SkipForward } from "lucide-react";
import { api, report } from "../../ipc/api";
import type { MediaCommand } from "../../ipc/types";
import { formatPlayback, playbackPosition } from "../../lib/time";
import { startSystemStore, useSystem } from "../../stores/system";
import { anchorOf, usePopover } from "../popover";

const send = (command: MediaCommand) => api.mediaCommand(command).catch(report("media_command"));

function Artwork({ size, radius }: { size: number | string; radius: number }) {
  const art = useSystem((s) => s.art);
  return art ? (
    <img
      src={art}
      alt=""
      draggable={false}
      style={{ width: size, height: size, borderRadius: radius, objectFit: "cover" }}
    />
  ) : (
    <span
      className="grid place-items-center bg-[var(--fill-hover)]"
      style={{ width: size, height: size, borderRadius: radius }}
      aria-hidden
    >
      <Music size={18} />
    </span>
  );
}

/** Text that scrolls sideways only when it doesn't fit. */
function Marquee({ text, className }: { text: string; className?: string }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [overflow, setOverflow] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el || !el.parentElement) return;
    setOverflow(Math.max(0, el.scrollWidth - el.parentElement.clientWidth));
  }, [text]);
  return (
    <span className={`block overflow-hidden whitespace-nowrap ${className ?? ""}`}>
      <span
        ref={ref}
        className={overflow > 0 ? "marquee inline-block" : "inline-block"}
        style={{ "--marquee-shift": `-${overflow + 8}px` } as CSSProperties}
      >
        {text}
      </span>
    </span>
  );
}

/** Dock tile: artwork, scrolling title and artist, play/pause. Hidden when nothing is playing. */
export function NowPlayingTile({ size }: { size: number }) {
  const media = useSystem((s) => s.media);
  const toggle = usePopover((s) => s.toggle);
  const open = usePopover((s) => s.id === "nowPlaying");
  useEffect(startSystemStore, []);

  if (!media?.available || !media.title) return null;

  return (
    <div className="dock-chip" style={{ height: size }} data-open={open}>
      <button
        type="button"
        className="flex min-w-0 items-center gap-2 text-left"
        data-widget="nowPlaying"
        aria-label={`Now playing: ${media.title}${media.artist ? ` by ${media.artist}` : ""}. Open player`}
        aria-expanded={open}
        onClick={(e) => toggle("nowPlaying", anchorOf(e.currentTarget.parentElement!))}
      >
        <Artwork size={size - 10} radius={7} />
        <span className="w-[132px] min-w-0 leading-tight">
          <Marquee text={media.title} className="text-[12.5px] font-semibold" />
          <Marquee text={media.artist || media.album} className="text-[11px] text-[var(--ink-muted)]" />
        </span>
      </button>
      {media.canToggle && (
        <button
          type="button"
          className="glass-button flex-none"
          aria-label={media.playing ? "Pause" : "Play"}
          onClick={() => send({ type: "toggle" })}
        >
          {media.playing ? <Pause size={16} fill="currentColor" /> : <Play size={16} fill="currentColor" />}
        </button>
      )}
    </div>
  );
}

/** Popover: large artwork, track details, seek bar and transport controls. */
export function NowPlayingPopover() {
  const media = useSystem((s) => s.media);
  const artColor = useSystem((s) => s.artColor);
  const [now, setNow] = useState(() => Date.now());
  // While the user drags the seek bar, show their value, not the player's.
  const [seeking, setSeeking] = useState<number | null>(null);

  // The position advances locally, once a second, only while this is open.
  const playing = media?.playing ?? false;
  useEffect(() => {
    setNow(Date.now());
    if (!playing) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [playing, media?.updatedAtMs]);

  if (!media?.available) {
    return <div className="p-5 text-[13px] text-[var(--ink-muted)]">Nothing is playing.</div>;
  }

  const position = seeking ?? playbackPosition(media, now);
  const nextRepeat = media.repeat === "none" ? "list" : media.repeat === "list" ? "track" : "none";
  // A wash of the artwork's colour over the glass.
  const tint = artColor
    ? { background: `linear-gradient(rgb(${artColor} / 0.42), rgb(${artColor} / 0.16))` }
    : undefined;

  return (
    <div className="rounded-[22px] p-4" style={tint}>
      <Artwork size="100%" radius={14} />
      <div className="mt-3 min-w-0">
        <div className="truncate text-[16px] font-semibold" title={media.title}>
          {media.title}
        </div>
        <div className="truncate text-[13px] text-[var(--ink-muted)]">{media.artist}</div>
        {media.album && media.album !== media.title && (
          <div className="truncate text-[12px] text-[var(--ink-faint)]">{media.album}</div>
        )}
      </div>

      {media.durationMs > 0 && (
        <div className="mt-3">
          <input
            type="range"
            className="glass-slider"
            aria-label="Position"
            min={0}
            max={media.durationMs}
            step={1000}
            value={position}
            disabled={!media.canSeek}
            style={{ "--fill": `${(position / media.durationMs) * 100}%` } as CSSProperties}
            onChange={(e) => setSeeking(Number(e.target.value))}
            onPointerUp={() => {
              if (seeking !== null) send({ type: "seek", positionMs: seeking });
              setSeeking(null);
            }}
            onKeyUp={() => {
              if (seeking !== null) send({ type: "seek", positionMs: seeking });
              setSeeking(null);
            }}
          />
          <div className="mt-1 flex justify-between text-[11px] tabular-nums text-[var(--ink-muted)]">
            <span>{formatPlayback(position)}</span>
            <span>{formatPlayback(media.durationMs)}</span>
          </div>
        </div>
      )}

      <div className="mt-2 flex items-center justify-center gap-2">
        {media.shuffle !== null && (
          <button
            type="button"
            className="glass-button h-9 w-9"
            aria-label="Shuffle"
            aria-pressed={media.shuffle}
            onClick={() => send({ type: "shuffle", on: !media.shuffle })}
          >
            <Shuffle size={16} />
          </button>
        )}
        <button
          type="button"
          className="glass-button h-10 w-10"
          aria-label="Previous"
          disabled={!media.canPrevious}
          onClick={() => send({ type: "previous" })}
        >
          <SkipBack size={20} fill="currentColor" />
        </button>
        <button
          type="button"
          className="glass-button glass-button-primary h-12 w-12"
          aria-label={media.playing ? "Pause" : "Play"}
          disabled={!media.canToggle}
          onClick={() => send({ type: "toggle" })}
        >
          {media.playing ? <Pause size={22} fill="currentColor" /> : <Play size={22} fill="currentColor" />}
        </button>
        <button
          type="button"
          className="glass-button h-10 w-10"
          aria-label="Next"
          disabled={!media.canNext}
          onClick={() => send({ type: "next" })}
        >
          <SkipForward size={20} fill="currentColor" />
        </button>
        {media.repeat !== null && (
          <button
            type="button"
            className="glass-button h-9 w-9"
            aria-label={`Repeat: ${media.repeat === "track" ? "this track" : media.repeat === "list" ? "all" : "off"}`}
            aria-pressed={media.repeat !== "none"}
            onClick={() => send({ type: "repeat", mode: nextRepeat })}
          >
            {media.repeat === "track" ? <Repeat1 size={16} /> : <Repeat size={16} />}
          </button>
        )}
      </div>
    </div>
  );
}
