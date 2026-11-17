import { useEffect, useState } from "react";
import { Gauge } from "lucide-react";
import type { Bridge, ClaudeState, LimitWindow } from "../../ipc/types";
import { compactNumber, countdown, dollars, usageColor } from "../../lib/format";
import { startOnlineStore, useOnline } from "../../stores/online";
import { anchorOf, usePopover } from "../popover";

/** Re-render once a minute so countdowns stay current while visible. */
function useMinuteClock(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}

/** A window that hasn't reset yet, or null. */
function live(window: LimitWindow | null | undefined, now: number): LimitWindow | null {
  return window && window.resetsAt * 1000 > now ? window : null;
}

function Ring({ percent, size, stroke }: { percent: number | null; size: number; stroke: number }) {
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const value = Math.min(100, Math.max(0, percent ?? 0));
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
      <circle cx={size / 2} cy={size / 2} r={radius} fill="none" stroke="var(--stroke)" strokeWidth={stroke} />
      {percent !== null && (
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke={usageColor(value)}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={circumference * (1 - value / 100)}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
          style={{ transition: "stroke-dashoffset 400ms ease" }}
        />
      )}
    </svg>
  );
}

/** Dock tile: the 5-hour window as a ring, with time until it resets. */
export function ClaudeTile({ size }: { size: number }) {
  const claude = useOnline((s) => s.claude);
  const toggle = usePopover((s) => s.toggle);
  const open = usePopover((s) => s.id === "claude");
  const now = useMinuteClock();
  useEffect(startOnlineStore, []);

  if (!claude || (!claude.hasLogs && !claude.limits)) return null;
  const five = live(claude.limits?.fiveHour, now);
  const ring = size - 10;
  const label = five
    ? `Claude: ${Math.round(five.usedPercentage)}% of the 5-hour limit used, resets in ${countdown(five.resetsAt * 1000, now)}`
    : "Claude usage";

  return (
    <button
      type="button"
      className="dock-chip"
      data-widget="claude"
      data-open={open}
      style={{ height: size, paddingRight: 10 }}
      aria-label={`${label}. Open details`}
      aria-expanded={open}
      onClick={(e) => toggle("claude", anchorOf(e.currentTarget))}
    >
      <span className="relative grid place-items-center">
        <Ring percent={five?.usedPercentage ?? null} size={ring} stroke={3} />
        <span className="absolute text-[10.5px] font-semibold tabular-nums">
          {five ? Math.round(five.usedPercentage) : <Gauge size={13} aria-hidden />}
        </span>
      </span>
      <span className="flex flex-col items-start leading-tight">
        <span className="text-[12px] font-semibold">Claude</span>
        <span className="text-[11px] text-[var(--ink-muted)] tabular-nums">
          {five
            ? countdown(five.resetsAt * 1000, now)
            : claude.block
              ? compactNumber(claude.block.tokens)
              : "idle"}
        </span>
      </span>
    </button>
  );
}

const BRIDGE_NOTES: Partial<Record<Bridge, string>> = {
  otherCommand:
    "Claude Code already has a status line set up, so Glassdock can't read the limits. Usage below is from the local logs.",
  off: "Reading the limits from Claude Code is turned off in Settings. Usage below is from the local logs.",
  devBuild:
    "Development build: the limits bridge is only installed by the installed app. Usage below is from the local logs.",
  noClaudeCode: "Claude Code isn't set up on this PC.",
  error: "Claude Code's settings file couldn't be updated, so the limits aren't available.",
};

function WindowBar({ title, window, now }: { title: string; window: LimitWindow; now: number }) {
  const used = Math.min(100, window.usedPercentage);
  const resets = new Date(window.resetsAt * 1000);
  const when = new Intl.DateTimeFormat(undefined, { weekday: "short", hour: "2-digit", minute: "2-digit" }).format(resets);
  return (
    <div className="mt-3">
      <div className="mb-1 flex justify-between text-[12.5px]">
        <span className="font-semibold">{title}</span>
        <span className="tabular-nums">{Math.round(window.usedPercentage)}%</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-[var(--stroke)]">
        <div className="h-full rounded-full" style={{ width: `${used}%`, background: usageColor(used) }} />
      </div>
      <div className="mt-1 text-[11.5px] text-[var(--ink-muted)]">
        Resets {when} (in {countdown(window.resetsAt * 1000, now)})
      </div>
    </div>
  );
}

function Breakdown({ claude }: { claude: ClaudeState }) {
  const maxCost = Math.max(0.01, ...claude.days.map((d) => d.cost));
  const weekday = new Intl.DateTimeFormat(undefined, { weekday: "narrow" });
  return (
    <>
      <section className="mt-4">
        <h3 className="mb-2 flex justify-between text-[12px] font-semibold text-[var(--ink-muted)]">
          <span>Last 7 days</span>
          <span className="font-normal tabular-nums">
            {compactNumber(claude.week.tokens)} tokens, about {dollars(claude.week.cost)}
          </span>
        </h3>
        <div className="flex h-[64px] items-end gap-1.5" aria-label="Estimated cost per day">
          {claude.days.map((d) => (
            <div key={d.date} className="flex h-full flex-1 flex-col items-center justify-end gap-1">
              <div
                className="w-full rounded-[4px]"
                title={`${d.date}: ${compactNumber(d.tokens)} tokens, about ${dollars(d.cost)}`}
                style={{
                  height: `${Math.max(2, (d.cost / maxCost) * 48)}px`,
                  background: d.cost > 0 ? "var(--accent)" : "var(--stroke)",
                }}
              />
              <span className="text-[10px] text-[var(--ink-muted)]">
                {weekday.format(new Date(`${d.date}T12:00`))}
              </span>
            </div>
          ))}
        </div>
      </section>
      {claude.models.length > 0 && (
        <section className="mt-3">
          <h3 className="mb-1 text-[12px] font-semibold text-[var(--ink-muted)]">By model, last 7 days</h3>
          {claude.models.map((m) => (
            <div key={m.model} className="flex justify-between py-0.5 text-[12.5px]">
              <span>{m.label}</span>
              <span className="tabular-nums text-[var(--ink-muted)]">
                {compactNumber(m.tokens)} tokens, {dollars(m.cost)}
              </span>
            </div>
          ))}
        </section>
      )}
    </>
  );
}

export function ClaudePopover() {
  const claude = useOnline((s) => s.claude);
  const now = useMinuteClock();
  if (!claude) return <div className="p-5 text-[13px] text-[var(--ink-muted)]">Loading</div>;

  const five = live(claude.limits?.fiveHour, now);
  const week = live(claude.limits?.sevenDay, now);
  const block = claude.block;
  const note = claude.bridge && !claude.limits ? BRIDGE_NOTES[claude.bridge] : undefined;
  const runsOut = claude.fiveHourRunsOutAt;

  return (
    <div className="p-4">
      <div className="flex items-center gap-4">
        <div className="relative grid flex-none place-items-center">
          <Ring percent={five?.usedPercentage ?? null} size={84} stroke={7} />
          <span className="absolute text-center leading-tight">
            <span className="block text-[20px] font-semibold tabular-nums">
              {five ? `${Math.round(five.usedPercentage)}%` : "–"}
            </span>
            <span className="block text-[10.5px] text-[var(--ink-muted)]">5 hours</span>
          </span>
        </div>
        <div className="min-w-0 text-[13px]">
          {five ? (
            <>
              <div className="font-semibold">Resets in {countdown(five.resetsAt * 1000, now)}</div>
              <div className="mt-0.5 text-[12px] text-[var(--ink-muted)]">
                {runsOut && runsOut * 1000 > now
                  ? `At this pace the limit is reached in ${countdown(runsOut * 1000, now)}.`
                  : "At this pace the limit lasts until the reset."}
              </div>
            </>
          ) : (
            <div className="text-[var(--ink-muted)]">
              {claude.limits
                ? "No 5-hour window is open. It starts with your next message."
                : "The limits appear after Claude Code's next response."}
            </div>
          )}
        </div>
      </div>

      {week && <WindowBar title="This week" window={week} now={now} />}
      {note && <div className="mt-3 rounded-xl bg-[var(--fill)] px-3 py-2 text-[12px]">{note}</div>}

      {block && (
        <section className="mt-4 grid grid-cols-3 gap-1.5 text-center">
          {[
            [compactNumber(block.tokens), "tokens this block"],
            [`${compactNumber(block.tokensPerMinute)}/min`, "burn rate"],
            [dollars(block.projectedCost), "projected cost"],
          ].map(([value, label]) => (
            <div key={label} className="rounded-xl bg-[var(--fill)] px-2 py-2">
              <div className="text-[14px] font-semibold tabular-nums">{value}</div>
              <div className="text-[10.5px] text-[var(--ink-muted)]">{label}</div>
            </div>
          ))}
        </section>
      )}

      {claude.hasLogs && <Breakdown claude={claude} />}
      <div className="mt-3 text-[10.5px] leading-snug text-[var(--ink-faint)]">
        Percentages come from Claude Code. Token counts and costs are estimated from its local logs at API
        list prices; a subscription isn't billed this way.
      </div>
    </div>
  );
}
