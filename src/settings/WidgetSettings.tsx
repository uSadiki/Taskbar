import { useEffect, useRef, useState } from "react";
import { LocateFixed, Search } from "lucide-react";
import { api, report } from "../ipc/api";
import type { Bridge, Place, Settings, SettingsPatch } from "../ipc/types";
import { startOnlineStore, useOnline } from "../stores/online";
import { Button, Row, Section, Segmented, Toggle } from "./controls";

type Update = (patch: SettingsPatch) => Promise<void>;

const input =
  "w-full rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] px-3 py-1.5 text-[13px] focus-visible:outline-2 focus-visible:outline-[var(--accent)]";

const placeName = (p: Place) => [p.name, p.region, p.country].filter(Boolean).join(", ");

function WeatherSettings({ settings, update }: { settings: Settings; update: Update }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Place[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const searchId = useRef(0);
  const { location, fahrenheit } = settings.weather;

  // Search as the user types, a moment after they stop.
  useEffect(() => {
    const id = ++searchId.current;
    if (query.trim().length < 2) {
      setResults([]);
      return;
    }
    const timer = window.setTimeout(() => {
      api.weatherSearch(query).then(
        (places) => {
          if (id !== searchId.current) return;
          setResults(places);
          setStatus(places.length === 0 ? "No places found." : null);
        },
        (error) => id === searchId.current && setStatus(String(error)),
      );
    }, 300);
    return () => window.clearTimeout(timer);
  }, [query]);

  const choose = (place: Place) => {
    void update({
      weather: { location: { name: place.name, latitude: place.latitude, longitude: place.longitude } },
    });
    setQuery("");
    setResults([]);
    setStatus(null);
  };

  return (
    <Section title="Weather">
      <Row label="Location" description={location ? location.name : "Not set. The weather tile appears once a city is chosen."}>
        {location && <Button onClick={() => update({ weather: { location: null } })}>Remove</Button>}
      </Row>
      <div className="px-4 pb-3">
        <div className="flex gap-2">
          <div className="relative flex-1">
            <Search size={14} className="absolute top-2.5 left-2.5 text-[var(--text-muted)]" aria-hidden />
            <input
              type="search"
              aria-label="Search for a city"
              placeholder="Search for a city"
              className={`${input} pl-8`}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <Button
            label="Use my approximate location"
            onClick={() => {
              setStatus("Locating");
              api.weatherLocate().then(choose, (error) => setStatus(String(error)));
            }}
          >
            <span className="flex items-center gap-1.5">
              <LocateFixed size={14} aria-hidden /> Locate
            </span>
          </Button>
        </div>
        <p className="mt-1.5 text-[12px] text-[var(--text-muted)]">
          Locate sends your IP address to ipwho.is to find the nearest city. It only runs when you click it.
        </p>
        {status && <p className="mt-1.5 text-[12.5px]">{status}</p>}
        {results.length > 0 && (
          <ul className="mt-2 divide-y divide-[var(--border)] rounded-lg border border-[var(--border)]">
            {results.map((place) => (
              <li key={`${place.latitude},${place.longitude}`}>
                <button
                  type="button"
                  className="w-full px-3 py-2 text-left text-[13px] hover:bg-[var(--surface-sunken)]"
                  onClick={() => choose(place)}
                >
                  {placeName(place)}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <Row label="Temperature">
        <Segmented
          label="Temperature unit"
          value={fahrenheit ? "f" : "c"}
          options={[
            { value: "c", label: "Celsius" },
            { value: "f", label: "Fahrenheit" },
          ]}
          onChange={(unit) => update({ weather: { fahrenheit: unit === "f" } })}
        />
      </Row>
    </Section>
  );
}

function GithubSettings() {
  const github = useOnline((s) => s.github);
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const connected = github && github.status !== "noToken";

  const save = () => {
    setBusy(true);
    setMessage(null);
    api.githubSetToken(token).then(
      (login) => {
        setToken("");
        setMessage(`Connected as ${login}.`);
        setBusy(false);
      },
      (error) => {
        setMessage(String(error));
        setBusy(false);
      },
    );
  };

  return (
    <Section title="GitHub Activity">
      <Row
        label="Account"
        description={
          connected
            ? github.data
              ? `Connected as ${github.data.login}.${github.message ? ` ${github.message}` : ""}`
              : (github.message ?? "Connected.")
            : "Not connected. The GitHub tile appears once a token is added."
        }
      >
        {connected && (
          <Button onClick={() => api.githubClearToken().catch(report("github_clear_token"))}>Remove token</Button>
        )}
      </Row>
      <div className="px-4 pb-3">
        <div className="flex gap-2">
          <input
            type="password"
            aria-label="GitHub token"
            placeholder={connected ? "Paste a new token to replace it" : "Paste a personal access token"}
            className={input}
            value={token}
            autoComplete="off"
            onChange={(e) => setToken(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && token && !busy && save()}
          />
          <Button disabled={!token || busy} onClick={save}>
            {busy ? "Checking" : "Save"}
          </Button>
        </div>
        {message && <p className="mt-1.5 text-[12.5px]">{message}</p>}
        <p className="mt-1.5 text-[12px] leading-snug text-[var(--text-muted)]">
          Create a fine-grained token on GitHub under Settings, Developer settings, Personal access tokens.
          Read-only access is enough: Public repositories, or All repositories with Metadata and Pull requests
          set to read-only. The token is kept in Windows Credential Manager, not in the settings file.
        </p>
        <Button onClick={() => api.openUrl("https://github.com/settings/personal-access-tokens/new").catch(report("open_url"))}>
          Create a token on GitHub
        </Button>
      </div>
    </Section>
  );
}

const BRIDGE_TEXT: Record<Bridge, string> = {
  installed: "Connected. Claude Code reports your limits to Glassdock after each response.",
  otherCommand:
    "Claude Code already runs another status line command, so Glassdock leaves it alone. Usage is estimated from the local logs instead.",
  off: "Off. Usage is estimated from Claude Code's local logs.",
  devBuild: "This is a development build, which doesn't register itself with Claude Code. The installed app does.",
  noClaudeCode: "Claude Code isn't set up on this PC.",
  error: "Claude Code's settings file couldn't be read or updated.",
};

function ClaudeSettings({ settings, update }: { settings: Settings; update: Update }) {
  const bridge = useOnline((s) => s.claude?.bridge);
  return (
    <Section title="Claude Usage">
      <Row
        label="Read limits from Claude Code"
        description={
          <>
            Adds Glassdock as Claude Code's status line command, which is how it learns the real 5-hour and weekly
            limits. Turning this off removes it again.
            {bridge && <span className="mt-1 block">{BRIDGE_TEXT[bridge]}</span>}
          </>
        }
      >
        <Toggle
          label="Read limits from Claude Code"
          checked={settings.claude.statuslineBridge}
          onChange={(on) => update({ claude: { statuslineBridge: on } })}
        />
      </Row>
    </Section>
  );
}

/** Settings for the widgets that need them, under the widget list. */
export function WidgetSettings({ settings, update }: { settings: Settings; update: Update }) {
  useEffect(startOnlineStore, []);
  return (
    <>
      <WeatherSettings settings={settings} update={update} />
      <GithubSettings />
      <ClaudeSettings settings={settings} update={update} />
    </>
  );
}
