import { create } from "zustand";
import { api, events, report } from "../ipc/api";
import type { ClaudeState, GithubState, WeatherState } from "../ipc/types";

interface OnlineState {
  weather: WeatherState | null;
  github: GithubState | null;
  claude: ClaudeState | null;
}

/** The widgets whose data comes from the network or from Claude Code. */
export const useOnline = create<OnlineState>(() => ({ weather: null, github: null, claude: null }));

let started = false;

export function startOnlineStore() {
  if (started) return;
  started = true;
  void events.weatherChanged((weather) => useOnline.setState({ weather }));
  void events.githubChanged((github) => useOnline.setState({ github }));
  void events.claudeChanged((claude) => useOnline.setState({ claude }));
  api.weatherState().then((weather) => useOnline.setState({ weather }), report("weather_state"));
  api.githubState().then((github) => useOnline.setState({ github }), report("github_state"));
  api.claudeState().then((claude) => useOnline.setState({ claude }), report("claude_state"));
}
