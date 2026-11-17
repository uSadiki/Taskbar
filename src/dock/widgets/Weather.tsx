import { useEffect } from "react";
import { Droplets, RefreshCw, Sunrise, Sunset, Thermometer, Umbrella, Wind } from "lucide-react";
import { api, report } from "../../ipc/api";
import { ago } from "../../lib/format";
import { clockTime, compass, describeCode } from "../../lib/weatherCodes";
import { startOnlineStore, useOnline } from "../../stores/online";
import { anchorOf, usePopover } from "../popover";
import { WeatherIcon } from "./WeatherIcon";

const degrees = (value: number) => `${Math.round(value)}°`;

/** Dock tile: icon and temperature. Hidden until a city is chosen. */
export function WeatherTile({ size }: { size: number }) {
  const weather = useOnline((s) => s.weather);
  const toggle = usePopover((s) => s.toggle);
  const open = usePopover((s) => s.id === "weather");
  useEffect(startOnlineStore, []);

  const forecast = weather?.forecast;
  if (!weather?.configured || !forecast) return null;
  const { sky, label } = describeCode(forecast.current.code);

  return (
    <button
      type="button"
      className="dock-chip"
      data-widget="weather"
      data-open={open}
      style={{ height: size, paddingRight: 10 }}
      aria-label={`${forecast.location}: ${label}, ${degrees(forecast.current.temperature)}. Open forecast`}
      aria-expanded={open}
      onClick={(e) => toggle("weather", anchorOf(e.currentTarget))}
    >
      <WeatherIcon sky={sky} day={forecast.current.isDay} size={size - 12} />
      <span className="text-[14px] font-semibold tabular-nums">{degrees(forecast.current.temperature)}</span>
    </button>
  );
}

function Fact({ Icon, label, value }: { Icon: typeof Wind; label: string; value: string }) {
  return (
    <div className="flex items-center gap-2 rounded-xl bg-[var(--fill)] px-2.5 py-2">
      <Icon size={15} className="flex-none text-[var(--ink-muted)]" aria-hidden />
      <div className="min-w-0 leading-tight">
        <div className="text-[11px] text-[var(--ink-muted)]">{label}</div>
        <div className="truncate text-[13px] font-semibold tabular-nums">{value}</div>
      </div>
    </div>
  );
}

export function WeatherPopover() {
  const weather = useOnline((s) => s.weather);
  const forecast = weather?.forecast;

  if (!forecast) {
    return (
      <div className="p-5 text-[13px] text-[var(--ink-muted)]">
        {weather?.error ?? "Choose a city in Settings to see the weather."}
      </div>
    );
  }

  const { current, hours, days } = forecast;
  const { sky, label } = describeCode(current.code);
  const today = days[0];
  const weekLow = Math.min(...days.map((d) => d.low));
  const weekHigh = Math.max(...days.map((d) => d.high));
  const span = Math.max(1, weekHigh - weekLow);
  const weekday = new Intl.DateTimeFormat(undefined, { weekday: "short" });

  return (
    <div className="p-4">
      <div className="flex items-start gap-3">
        <WeatherIcon sky={sky} day={current.isDay} size={58} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] text-[var(--ink-muted)]">{forecast.location}</div>
          <div className="text-[36px] font-light leading-none tabular-nums">{degrees(current.temperature)}</div>
          <div className="mt-1 text-[13px]">
            {label}
            {today && (
              <span className="ml-2 text-[var(--ink-muted)] tabular-nums">
                H {degrees(today.high)}
                <span className="ml-1.5">L {degrees(today.low)}</span>
              </span>
            )}
          </div>
        </div>
        <button
          type="button"
          className="glass-button"
          aria-label="Refresh"
          title={`Updated ${ago(forecast.fetchedAt * 1000, Date.now())}`}
          onClick={() => api.weatherRefresh().catch(report("weather_refresh"))}
        >
          <RefreshCw size={15} />
        </button>
      </div>

      {weather?.error && (
        <div className="mt-2 text-[12px] text-[var(--ink-muted)]">
          {weather.error}. Showing the forecast from {ago(forecast.fetchedAt * 1000, Date.now())}.
        </div>
      )}

      <div className="mt-3 grid grid-cols-3 gap-1.5">
        <Fact Icon={Thermometer} label="Feels like" value={degrees(current.feelsLike)} />
        <Fact
          Icon={Wind}
          label="Wind"
          value={`${Math.round(current.windSpeed)} m/s ${compass(current.windDirection)}`}
        />
        <Fact Icon={Droplets} label="Humidity" value={`${Math.round(current.humidity)}%`} />
        <Fact Icon={Umbrella} label="Precipitation" value={`${current.precipitation.toFixed(1)} mm`} />
        {today && <Fact Icon={Sunrise} label="Sunrise" value={clockTime(today.sunrise)} />}
        {today && <Fact Icon={Sunset} label="Sunset" value={clockTime(today.sunset)} />}
      </div>
      <div className="mt-1.5 text-[11.5px] text-[var(--ink-muted)]">
        UV index {current.uvIndex.toFixed(0)}
        {today ? `, up to ${today.uvIndexMax.toFixed(0)} today` : ""}
      </div>

      <div
        className="scroll-x mt-3 -mx-1 flex gap-0.5 overflow-x-auto"
        aria-label="Next 24 hours"
        onWheel={(e) => {
          // A mouse wheel scrolls the hours sideways.
          if (e.deltaY !== 0) e.currentTarget.scrollLeft += e.deltaY;
        }}
      >
        {hours.map((h, i) => {
          const hour = describeCode(h.code);
          return (
            <div key={h.time} className="flex w-[38px] flex-none flex-col items-center gap-1 py-1">
              <span className="text-[11px] text-[var(--ink-muted)] tabular-nums">
                {i === 0 ? "Now" : clockTime(h.time).slice(0, 2)}
              </span>
              <WeatherIcon sky={hour.sky} day={h.isDay} size={22} animate={false} />
              <span className="text-[12.5px] font-semibold tabular-nums">{degrees(h.temperature)}</span>
              <span className="h-3 text-[10px] text-[var(--accent)] tabular-nums">
                {h.precipitationChance >= 20 ? `${Math.round(h.precipitationChance)}%` : ""}
              </span>
            </div>
          );
        })}
      </div>

      <div className="mt-2 border-t border-[var(--stroke)] pt-2" aria-label="Seven days">
        {days.map((d, i) => {
          const day = describeCode(d.code);
          const left = ((d.low - weekLow) / span) * 100;
          const width = ((d.high - d.low) / span) * 100;
          return (
            <div key={d.date} className="flex items-center gap-2 py-1 text-[13px]">
              <span className="w-10 flex-none">
                {i === 0 ? "Today" : weekday.format(new Date(`${d.date}T12:00`))}
              </span>
              <WeatherIcon sky={day.sky} size={20} animate={false} />
              <span className="w-8 flex-none text-right text-[11px] text-[var(--accent)] tabular-nums">
                {d.precipitationChance >= 20 ? `${Math.round(d.precipitationChance)}%` : ""}
              </span>
              <span className="w-8 flex-none text-right text-[var(--ink-muted)] tabular-nums">
                {degrees(d.low)}
              </span>
              <span className="relative h-1.5 flex-1 rounded-full bg-[var(--stroke)]" aria-hidden>
                <span
                  className="absolute inset-y-0 rounded-full"
                  style={{
                    left: `${left}%`,
                    width: `${Math.max(width, 4)}%`,
                    background: "linear-gradient(to right, #6fb6ff, #ffc642)",
                  }}
                />
              </span>
              <span className="w-8 flex-none tabular-nums">{degrees(d.high)}</span>
            </div>
          );
        })}
      </div>
      <div className="mt-1 text-right text-[10.5px] text-[var(--ink-faint)]">Weather data by Open-Meteo.com</div>
    </div>
  );
}
