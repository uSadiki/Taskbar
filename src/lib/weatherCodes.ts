/** What a weather icon draws. */
export type Sky =
  | "clear"
  | "partly"
  | "cloudy"
  | "fog"
  | "drizzle"
  | "rain"
  | "sleet"
  | "snow"
  | "showers"
  | "thunder";

/** WMO weather interpretation codes, as Open-Meteo reports them. */
export function describeCode(code: number): { sky: Sky; label: string } {
  switch (code) {
    case 0:
      return { sky: "clear", label: "Clear" };
    case 1:
      return { sky: "partly", label: "Mostly clear" };
    case 2:
      return { sky: "partly", label: "Partly cloudy" };
    case 3:
      return { sky: "cloudy", label: "Overcast" };
    case 45:
    case 48:
      return { sky: "fog", label: "Fog" };
    case 51:
    case 53:
    case 55:
      return { sky: "drizzle", label: "Drizzle" };
    case 56:
    case 57:
      return { sky: "sleet", label: "Freezing drizzle" };
    case 61:
      return { sky: "rain", label: "Light rain" };
    case 63:
      return { sky: "rain", label: "Rain" };
    case 65:
      return { sky: "rain", label: "Heavy rain" };
    case 66:
    case 67:
      return { sky: "sleet", label: "Freezing rain" };
    case 71:
      return { sky: "snow", label: "Light snow" };
    case 73:
      return { sky: "snow", label: "Snow" };
    case 75:
      return { sky: "snow", label: "Heavy snow" };
    case 77:
      return { sky: "snow", label: "Snow grains" };
    case 80:
    case 81:
    case 82:
      return { sky: "showers", label: code === 82 ? "Heavy showers" : "Showers" };
    case 85:
    case 86:
      return { sky: "snow", label: "Snow showers" };
    case 95:
      return { sky: "thunder", label: "Thunderstorm" };
    case 96:
    case 99:
      return { sky: "thunder", label: "Thunderstorm with hail" };
    default:
      return { sky: "cloudy", label: "Unknown" };
  }
}

/** Compass direction the wind blows from: 230 -> "SW". */
export function compass(degrees: number): string {
  const points = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
  return points[Math.round((((degrees % 360) + 360) % 360) / 45) % 8];
}

/** "2026-11-17T08:21" -> "08:21" (already local to the location). */
export function clockTime(isoLocal: string): string {
  return isoLocal.slice(11, 16);
}
