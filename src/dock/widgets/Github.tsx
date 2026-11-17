import { useEffect } from "react";
import {
  CircleDot,
  Flame,
  GitCommitHorizontal,
  GitPullRequest,
  MessageSquare,
  Plus,
  RefreshCw,
  ScanEye,
  type LucideIcon,
} from "lucide-react";
import { api, report } from "../../ipc/api";
import type { ContributionDay, GithubEvent, PullRequest } from "../../ipc/types";
import { ago } from "../../lib/format";
import { startOnlineStore, useOnline } from "../../stores/online";
import { anchorOf, usePopover } from "../popover";

/** The last `weeks` columns of the calendar, each a Monday-first week, oldest first. */
export function lastWeeks(days: ContributionDay[], weeks: number): (ContributionDay | null)[][] {
  if (days.length === 0) return [];
  const columns: (ContributionDay | null)[][] = [];
  let column: (ContributionDay | null)[] = [];
  for (const day of days) {
    // getDay: 0 is Sunday. Rows run Monday to Sunday.
    const row = (new Date(`${day.date}T12:00`).getDay() + 6) % 7;
    if (row === 0 && column.length > 0) {
      columns.push(column);
      column = [];
    }
    while (column.length < row) column.push(null);
    column.push(day);
  }
  if (column.length > 0) columns.push(column);
  return columns.slice(-weeks);
}

/** A heat-map cell's fill for GitHub's 0 to 4 level. */
const cellFill = (level: number) =>
  level === 0
    ? "var(--stroke)"
    : `color-mix(in srgb, var(--good) ${[0, 35, 58, 80, 100][level]}%, transparent)`;

function HeatMap({ days, weeks, cell, gap }: { days: ContributionDay[]; weeks: number; cell: number; gap: number }) {
  const columns = lastWeeks(days, weeks);
  return (
    <div className="flex" style={{ gap }} aria-hidden>
      {columns.map((column, x) => (
        <div key={x} className="flex flex-col" style={{ gap }}>
          {Array.from({ length: 7 }, (_, y) => {
            const day = column[y];
            return (
              <span
                key={y}
                title={day ? `${day.count} contribution${day.count === 1 ? "" : "s"} on ${day.date}` : undefined}
                style={{
                  width: cell,
                  height: cell,
                  borderRadius: Math.max(1, cell / 4),
                  background: day ? cellFill(day.level) : "transparent",
                }}
              />
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** Dock tile: the last 18 weeks and the current streak. */
export function GithubTile({ size }: { size: number }) {
  const github = useOnline((s) => s.github);
  const toggle = usePopover((s) => s.toggle);
  const open = usePopover((s) => s.id === "github");
  useEffect(startOnlineStore, []);

  if (!github || github.status === "noToken") return null;
  const data = github.data;
  const cell = Math.max(3, Math.floor((size - 10 - 6) / 7));

  return (
    <button
      type="button"
      className="dock-chip"
      data-widget="github"
      data-open={open}
      style={{ height: size, paddingRight: 10 }}
      aria-label={
        data
          ? `GitHub: ${data.today} contributions today, ${data.currentStreak} day streak. Open activity`
          : "GitHub activity"
      }
      aria-expanded={open}
      onClick={(e) => toggle("github", anchorOf(e.currentTarget))}
    >
      {data ? (
        <>
          <HeatMap days={data.days} weeks={18} cell={cell} gap={1} />
          <span className="flex flex-col items-start leading-tight">
            <span className="flex items-center gap-0.5 text-[13px] font-semibold tabular-nums">
              <Flame size={12} aria-hidden className="text-[var(--warn)]" />
              {data.currentStreak}
            </span>
            <span className="text-[11px] text-[var(--ink-muted)] tabular-nums">{data.today} today</span>
          </span>
        </>
      ) : (
        <span className="px-1 text-[12px] text-[var(--ink-muted)]">GitHub</span>
      )}
      {github.status !== "ok" && (
        <span className="status-dot" style={{ background: "var(--warn)" }} aria-hidden />
      )}
    </button>
  );
}

const EVENT_ICONS: Record<GithubEvent["kind"], LucideIcon> = {
  push: GitCommitHorizontal,
  pr: GitPullRequest,
  review: ScanEye,
  issue: CircleDot,
  comment: MessageSquare,
  create: Plus,
  other: CircleDot,
};

const open = (url: string) => api.openUrl(url).catch(report("open_url"));

function PrList({ title, items, total }: { title: string; items: PullRequest[]; total: number }) {
  return (
    <section className="mt-3">
      <h3 className="mb-1 text-[12px] font-semibold text-[var(--ink-muted)]">
        {title} <span className="font-normal tabular-nums">{total}</span>
      </h3>
      {items.length === 0 ? (
        <div className="px-2 py-1 text-[12.5px] text-[var(--ink-faint)]">None</div>
      ) : (
        items.slice(0, 5).map((pr) => (
          <button key={pr.url} type="button" className="list-row" onClick={() => open(pr.url)} title={pr.url}>
            <GitPullRequest size={14} className="flex-none" style={{ color: pr.draft ? "var(--ink-faint)" : "var(--good)" }} aria-hidden />
            <span className="min-w-0 flex-1 truncate">{pr.title}</span>
            <span className="flex-none text-[11px] text-[var(--ink-muted)]">
              {pr.repo.split("/")[1]}#{pr.number}
            </span>
          </button>
        ))
      )}
    </section>
  );
}

export function GithubPopover() {
  const github = useOnline((s) => s.github);
  const data = github?.data;
  const now = Date.now();

  if (!github || github.status === "noToken") {
    return (
      <div className="p-5 text-[13px] text-[var(--ink-muted)]">
        Add a GitHub token in Settings, under Widgets, to see your activity.
      </div>
    );
  }

  const stat = (value: number | string, label: string) => (
    <div className="flex-1 rounded-xl bg-[var(--fill)] px-3 py-2">
      <div className="text-[20px] font-semibold leading-tight tabular-nums">{value}</div>
      <div className="text-[11.5px] text-[var(--ink-muted)]">{label}</div>
    </div>
  );

  return (
    <div className="p-4">
      <div className="mb-3 flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="truncate text-[15px] font-semibold">{data ? data.login : "GitHub"}</div>
          {data && (
            <div className="text-[12px] text-[var(--ink-muted)]">
              {data.totalYear.toLocaleString()} contributions in the last year
            </div>
          )}
        </div>
        <button
          type="button"
          className="glass-button"
          aria-label="Refresh"
          title={data ? `Updated ${ago(data.fetchedAt * 1000, now)}` : undefined}
          onClick={() => api.githubRefresh().catch(report("github_refresh"))}
        >
          <RefreshCw size={15} />
        </button>
      </div>

      {github.message && (
        <div className="mb-3 rounded-xl bg-[var(--fill)] px-3 py-2 text-[12.5px]">{github.message}</div>
      )}

      {data && (
        <>
          <div className="flex justify-end overflow-hidden">
            <HeatMap days={data.days} weeks={53} cell={7} gap={2} />
          </div>
          <div className="mt-3 flex gap-1.5">
            {stat(data.today, "today")}
            {stat(data.currentStreak, "day streak")}
            {stat(data.longestStreak, "longest streak")}
          </div>

          <PrList title="Your open pull requests" items={data.authored} total={data.authoredTotal} />
          <PrList title="Review requests" items={data.reviewRequests} total={data.reviewTotal} />

          <section className="mt-3">
            <h3 className="mb-1 text-[12px] font-semibold text-[var(--ink-muted)]">Recent activity</h3>
            <div className="max-h-[176px] overflow-y-auto">
              {data.events.length === 0 ? (
                <div className="px-2 py-1 text-[12.5px] text-[var(--ink-faint)]">Nothing recent</div>
              ) : (
                data.events.map((event, i) => {
                  const Icon = EVENT_ICONS[event.kind] ?? CircleDot;
                  return (
                    <button
                      key={`${event.url}-${i}`}
                      type="button"
                      className="list-row"
                      onClick={() => open(event.url)}
                      title={event.url}
                    >
                      <Icon size={14} className="flex-none text-[var(--ink-muted)]" aria-hidden />
                      <span className="min-w-0 flex-1 truncate">
                        {event.summary} <span className="text-[var(--ink-muted)]">in {event.repo}</span>
                      </span>
                      <span className="flex-none text-[11px] text-[var(--ink-muted)]">
                        {ago(Date.parse(event.createdAt), now)}
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          </section>
        </>
      )}
    </div>
  );
}

